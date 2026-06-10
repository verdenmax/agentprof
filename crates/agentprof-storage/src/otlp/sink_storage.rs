//! [`FlushSink`] implementation that persists OTLP buffers to `SQLite`
//! through the M2.1 [`upsert_report`] path (M2.2 T7.1).
//!
//! # Design
//!
//! [`StorageFlushSink`] is the production sink wired into the
//! [`crate::otlp::router::SessionRouter`]. When the router decides to
//! close a [`crate::otlp::router::SessionBuffer`] (explicit end / OOM /
//! idle / shutdown), it calls [`FlushSink::flush`], handing over a
//! [`PersistableSession`]. This impl translates that into the
//! [`AnalysisReport`] shape M2.1 already knows how to persist and
//! re-uses the atomic three-table [`upsert_report`] write path.
//!
//! `raw_path` is synthesised as `otlp://<session_id>` so the
//! `sessions.raw_path` column is non-NULL (schema requirement) and OTLP-
//! sourced rows are visually distinguishable from filesystem-ingested
//! ones in the CLI listing.
//!
//! # Lossy mappings (spec §6)
//!
//! The M2.1 schema was designed for jsonl-derived `AnalysisReport`s
//! and cannot 1:1 represent every [`TypedEvent`] variant. Per spec §6
//! "OTLP shares the same storage backend but is allowed to be lossy",
//! the following variants are dropped with a `tracing::warn!`:
//!
//! - [`TypedEvent::Unrecognized`] — no schema column models "unknown
//!   wire event"; the mapper has already logged identity.
//! - Per-prompt `prompt_size_bytes` from [`TypedEvent::UserPrompt`] —
//!   the `turn_buckets` table tracks output tokens / model only.
//! - `cwd` from [`TypedEvent::SessionStart`] — there is no `sessions.cwd`
//!   column; we surface it on the `meta.cwd` `Option` instead, so it
//!   survives in the JSON blob but is not indexed.
//! - [`CloseReason`] — not persisted; only surfaced in tracing so an
//!   operator can correlate OOM-cap incidents with row appearance.
//!
//! All other fields land in the report blob (`sessions.analysis_report_json`)
//! or in the indexed columns via [`upsert_report`].
//!
//! [`CloseReason`]: crate::otlp::router::CloseReason

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::{AnalysisReport, ModelUsage, ToolRankRow};
use agentprof_core::episode::tool::ToolCallStatus;
use agentprof_core::model::tool_source::ToolSource;
use agentprof_core::model::SessionMeta;
use chrono::{Duration as ChronoDuration, Utc};
use tracing::{debug, warn};

use crate::otlp::router::{FlushResult, FlushSink, PersistableSession, SessionId};
use crate::otlp::typed::{TokenDirection, TypedEvent};
use crate::upsert::upsert_report;
use crate::Db;

/// `now() -> unix-epoch seconds` factory used for `ingested_at` on each
/// flush. Boxed so tests can inject a deterministic clock without a
/// generic parameter leaking into the trait object.
type NowFn = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Map key for in-flight tool start lookups: (tool name, optional turn id).
type OpenStartKey = (String, Option<String>);

/// Map value for in-flight tool start lookups:
/// (start timestamp, original source, user-approved flag).
type OpenStartVal = (chrono::DateTime<chrono::Utc>, ToolSource, bool);

/// [`FlushSink`] backed by the M2.1 `SQLite` write path.
///
/// Holds a shared `Arc<Mutex<Db>>` connection handle. Flush calls run
/// synchronously on whatever thread the router was driven from — for
/// the OTLP receiver that is a tokio worker thread, so flush latency
/// briefly blocks one runtime worker per close. This is identical to
/// the M2.1 ingest path's blocking pattern and is acceptable for the
/// expected flush rate (one per session close, not per event).
///
/// # Examples
///
/// ```
/// use std::sync::{Arc, Mutex};
/// use agentprof_storage::Db;
/// use agentprof_storage::otlp::sink_storage::StorageFlushSink;
///
/// let db = Arc::new(Mutex::new(Db::open_in_memory().expect("memory db")));
/// let _sink = StorageFlushSink::new(db);
/// ```
pub struct StorageFlushSink {
    db: Arc<Mutex<Db>>,
    now_fn: NowFn,
}

impl std::fmt::Debug for StorageFlushSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageFlushSink").finish_non_exhaustive()
    }
}

impl StorageFlushSink {
    /// Wrap a shared `SQLite` handle as a [`FlushSink`].
    ///
    /// `ingested_at_secs` defaults to [`chrono::Utc::now`]; override with
    /// [`Self::with_now_fn`] for deterministic tests.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::{Arc, Mutex};
    /// use agentprof_storage::Db;
    /// use agentprof_storage::otlp::sink_storage::StorageFlushSink;
    ///
    /// let db = Arc::new(Mutex::new(Db::open_in_memory().expect("memory db")));
    /// let _sink = StorageFlushSink::new(db);
    /// ```
    #[must_use]
    pub fn new(db: Arc<Mutex<Db>>) -> Self {
        Self {
            db,
            now_fn: Arc::new(|| Utc::now().timestamp()),
        }
    }

    /// Override the `ingested_at_secs` clock — for deterministic tests.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::{Arc, Mutex};
    /// use agentprof_storage::Db;
    /// use agentprof_storage::otlp::sink_storage::StorageFlushSink;
    ///
    /// let db = Arc::new(Mutex::new(Db::open_in_memory().expect("memory db")));
    /// let sink = StorageFlushSink::new(db).with_now_fn(Arc::new(|| 1_700_000_000));
    /// drop(sink);
    /// ```
    #[must_use]
    pub fn with_now_fn(mut self, now_fn: NowFn) -> Self {
        self.now_fn = now_fn;
        self
    }
}

impl FlushSink for StorageFlushSink {
    fn flush(&self, session_id: &SessionId, persistable: PersistableSession) -> FlushResult {
        let close_reason = persistable.close_reason;
        let report = persistable_to_report(session_id, persistable);
        let raw_path = PathBuf::from(format!("otlp://{session_id}"));
        let ingested_at = (self.now_fn)();

        let mut guard = self.db.lock().unwrap_or_else(PoisonError::into_inner);
        match upsert_report(&mut guard, &report, &raw_path, ingested_at) {
            Ok(_) => {
                debug!(
                    target: "agentprof::otlp::sink_storage",
                    session_id = %session_id,
                    close_reason = ?close_reason,
                    "otlp session persisted",
                );
                Ok(())
            }
            Err(e) => {
                warn!(
                    target: "agentprof::otlp::sink_storage",
                    session_id = %session_id,
                    close_reason = ?close_reason,
                    error = %e,
                    "failed to persist otlp session; row will be missing from `agentprof list`",
                );
                Err(e.into())
            }
        }
    }
}

/// Translate a closed OTLP session buffer into the M2.1 `AnalysisReport`
/// shape ready for [`upsert_report`].
///
/// Mapping rules:
///
/// - The first [`TypedEvent::SessionStart`] (if any) seeds
///   `meta.agent` / `meta.started_at` / `meta.cwd`. Without a start
///   event, `meta.agent` falls back to [`AgentKind::Claude`] (the
///   default OTLP source) and `started_at` to `persistable.started_at`
///   or `Utc::now()` as last resort.
/// - [`TypedEvent::TokenUsage`] points are aggregated into
///   `model_metrics` keyed by `model`.
/// - [`TypedEvent::ToolDecisionStart`] + [`TypedEvent::ToolResult`]
///   pairs roll up into [`ToolRankRow`]s by `(tool_name, source)`.
///   Per-call latencies are computed from paired timestamps; unpaired
///   starts are not seen here because [`crate::otlp::router::SessionBuffer::into_persistable`]
///   synthesises closing results.
/// - [`TypedEvent::UserPrompt`] events are counted but not modeled as
///   turn rows in this iteration (lossy — see module-level doc).
/// - [`TypedEvent::Unrecognized`] / [`TypedEvent::SessionEnd`] / any
///   leftover variant: dropped (the timestamps drove
///   `meta.started_at` already).
#[allow(clippy::too_many_lines)]
fn persistable_to_report(session_id: &str, persistable: PersistableSession) -> AnalysisReport {
    let PersistableSession {
        events, started_at, ..
    } = persistable;

    let mut agent: AgentKind = AgentKind::Claude;
    let mut model_at_start: Option<String> = None;
    let mut cwd_at_start: Option<String> = None;
    let mut meta_started_at = started_at;

    let mut model_metrics: std::collections::BTreeMap<String, ModelUsage> =
        std::collections::BTreeMap::new();

    // (tool_name) -> aggregate; source kept from first observation.
    let mut tool_rollups: std::collections::HashMap<String, ToolAggregator> =
        std::collections::HashMap::new();

    // open starts keyed by (tool_name, turn_id) for pairing on result.
    let mut open_starts: std::collections::HashMap<OpenStartKey, OpenStartVal> =
        std::collections::HashMap::new();

    let mut user_prompts = 0_usize;
    let mut unrecognized = 0_usize;

    for ev in events {
        match ev {
            TypedEvent::SessionStart {
                agent: a,
                started_at: ts,
                model,
                cwd,
                ..
            } => {
                agent = a;
                model_at_start = model;
                cwd_at_start = cwd.map(|p| p.to_string_lossy().into_owned());
                meta_started_at = meta_started_at.or(Some(ts));
            }
            TypedEvent::UserPrompt { .. } => {
                user_prompts += 1;
            }
            TypedEvent::ToolDecisionStart {
                tool_name,
                turn_id,
                source,
                timestamp,
                user_approved,
                ..
            } => {
                open_starts.insert(
                    (tool_name.clone(), turn_id),
                    (timestamp, source.clone(), user_approved),
                );
                tool_rollups
                    .entry(tool_name)
                    .or_insert_with(|| ToolAggregator::new(source));
            }
            TypedEvent::ToolResult {
                tool_name,
                turn_id,
                timestamp,
                status,
                ..
            } => {
                let agg = tool_rollups
                    .entry(tool_name.clone())
                    .or_insert_with(|| ToolAggregator::new(ToolSource::Builtin));
                if let Some((start_ts, src, user_req)) = open_starts.remove(&(tool_name, turn_id)) {
                    agg.source = src;
                    let dur = timestamp.signed_duration_since(start_ts);
                    let dur = if dur < ChronoDuration::zero() {
                        ChronoDuration::zero()
                    } else {
                        dur
                    };
                    agg.record(&status, dur, user_req);
                } else {
                    // Result with no matching start; count as orphan with zero duration.
                    agg.record(&status, ChronoDuration::zero(), false);
                }
            }
            TypedEvent::TokenUsage {
                model,
                direction,
                value,
                ..
            } => {
                let entry = model_metrics.entry(model).or_default();
                match direction {
                    TokenDirection::Input => {
                        entry.input_tokens = entry.input_tokens.saturating_add(value);
                    }
                    TokenDirection::Output => {
                        entry.output_tokens = entry.output_tokens.saturating_add(value);
                    }
                    TokenDirection::CacheRead => {
                        entry.cache_read_tokens = entry.cache_read_tokens.saturating_add(value);
                    }
                    TokenDirection::CacheCreation => {
                        entry.cache_write_tokens = entry.cache_write_tokens.saturating_add(value);
                    }
                }
            }
            TypedEvent::SessionEnd { .. } => {
                // ended_at is on PersistableSession.ended_at — already used
                // to drive started_at fallback decisions. We intentionally
                // don't surface ended_at in the report (no schema column);
                // duration_ms in `upsert_report` is also left NULL today.
            }
            TypedEvent::Unrecognized { signal, identity } => {
                unrecognized += 1;
                debug!(
                    target: "agentprof::otlp::sink_storage",
                    %identity,
                    ?signal,
                    session_id = %session_id,
                    "dropping unrecognized otlp event",
                );
            }
        }
    }

    if unrecognized > 0 || user_prompts > 0 {
        debug!(
            target: "agentprof::otlp::sink_storage",
            session_id = %session_id,
            user_prompts,
            unrecognized,
            "otlp lossy mapping summary",
        );
    }

    // Any leftover open starts indicate the pairing in `into_persistable`
    // already synthesised a `OpenAtEndOfSession` result — but we may still
    // have entries here if the synthesised result was already drained above.
    // The match-arm for `ToolResult` would have removed them; what remains
    // is leftover from race-y synthetic generation. Roll them up as orphans.
    for ((tool_name, _turn), (_ts, src, user_req)) in open_starts {
        let agg = tool_rollups
            .entry(tool_name)
            .or_insert_with(|| ToolAggregator::new(src.clone()));
        agg.source = src;
        agg.record(
            &ToolCallStatus::OpenAtEndOfSession,
            ChronoDuration::zero(),
            user_req,
        );
    }

    let mut tool_rank: Vec<ToolRankRow> = tool_rollups
        .into_iter()
        .map(|(name, agg)| agg.into_row(name))
        .collect();
    tool_rank.sort_by(|a, b| {
        b.total_duration
            .cmp(&a.total_duration)
            .then_with(|| a.name.cmp(&b.name))
    });

    let started_at_final = meta_started_at.unwrap_or_else(Utc::now);
    let mut meta = SessionMeta::new(session_id.to_owned(), agent, started_at_final, false);
    meta.cwd = cwd_at_start;
    // model_at_start is preserved as a hint for analyzer downstream; we
    // don't have a SessionMeta field for it (only agent / cwd / branch /
    // repository are surfaced), so we let it ride in tool_rank only when
    // tokens land. Stash it on the report's blob through model_metrics if
    // we've never seen a TokenUsage event so a downstream reader still
    // sees the model identifier.
    if model_metrics.is_empty() {
        if let Some(model) = model_at_start {
            model_metrics.insert(model, ModelUsage::new());
        }
    }

    let mut report = AnalysisReport::new(meta);
    if !model_metrics.is_empty() {
        report.model_metrics = Some(model_metrics);
    }
    report.tool_rank = tool_rank;
    report
}

/// Per-tool rollup accumulator used while walking the event stream.
#[derive(Debug)]
struct ToolAggregator {
    source: ToolSource,
    call_count: usize,
    success_count: usize,
    failure_count: usize,
    orphan_count: usize,
    user_requested_count: usize,
    total_duration: ChronoDuration,
    max_duration: ChronoDuration,
    durations: Vec<ChronoDuration>,
}

impl ToolAggregator {
    const fn new(source: ToolSource) -> Self {
        Self {
            source,
            call_count: 0,
            success_count: 0,
            failure_count: 0,
            orphan_count: 0,
            user_requested_count: 0,
            total_duration: ChronoDuration::zero(),
            max_duration: ChronoDuration::zero(),
            durations: Vec::new(),
        }
    }

    fn record(&mut self, status: &ToolCallStatus, dur: ChronoDuration, user_requested: bool) {
        self.call_count += 1;
        match status {
            ToolCallStatus::Success => self.success_count += 1,
            ToolCallStatus::Failure { .. } => self.failure_count += 1,
            ToolCallStatus::OrphanSynthesizedStart | ToolCallStatus::OpenAtEndOfSession => {
                self.orphan_count += 1;
            }
            _ => {}
        }
        if user_requested {
            self.user_requested_count += 1;
        }
        self.total_duration += dur;
        if dur > self.max_duration {
            self.max_duration = dur;
        }
        self.durations.push(dur);
    }

    fn into_row(mut self, name: String) -> ToolRankRow {
        self.durations.sort();
        let p50 = nearest_rank(&self.durations, 50);
        let p95 = nearest_rank(&self.durations, 95);
        ToolRankRow::new(
            name,
            self.source,
            self.call_count,
            self.success_count,
            self.failure_count,
            self.orphan_count,
            self.user_requested_count,
            self.total_duration,
            p50,
            p95,
            self.max_duration,
        )
    }
}

/// Nearest-rank percentile (matches `agentprof-core::analyzer::tool_rank`'s
/// semantics for the OTLP path). Returns zero duration for an empty
/// sample so the resulting row stays well-formed.
fn nearest_rank(sorted: &[ChronoDuration], pct: u32) -> ChronoDuration {
    if sorted.is_empty() {
        return ChronoDuration::zero();
    }
    let len = sorted.len();
    // ceil(pct/100 * len) - 1, clamped to [0, len-1]
    let rank = (pct as usize * len).div_ceil(100);
    let idx = rank.saturating_sub(1).min(len - 1);
    sorted[idx]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::otlp::router::CloseReason;

    fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(secs, 0).expect("valid ts")
    }

    #[test]
    fn nearest_rank_handles_empty_and_singleton() {
        assert_eq!(nearest_rank(&[], 50), ChronoDuration::zero());
        let one = vec![ChronoDuration::milliseconds(100)];
        assert_eq!(nearest_rank(&one, 50), ChronoDuration::milliseconds(100));
        assert_eq!(nearest_rank(&one, 95), ChronoDuration::milliseconds(100));
    }

    #[test]
    fn persistable_with_only_start_and_end_produces_meta() {
        let p = PersistableSession {
            session_id: "s1".to_owned(),
            close_reason: CloseReason::ExplicitEnd,
            events: vec![
                TypedEvent::SessionStart {
                    session_id: "s1".to_owned(),
                    agent: AgentKind::Claude,
                    started_at: ts(1_700_000_000),
                    model: Some("claude-sonnet-4.6".into()),
                    cwd: None,
                },
                TypedEvent::SessionEnd {
                    session_id: "s1".to_owned(),
                    ended_at: ts(1_700_000_300),
                },
            ],
            started_at: Some(ts(1_700_000_000)),
            ended_at: Some(ts(1_700_000_300)),
        };
        let r = persistable_to_report("s1", p);
        assert_eq!(r.meta.id, "s1");
        assert_eq!(r.meta.agent, AgentKind::Claude);
        assert_eq!(r.meta.started_at, ts(1_700_000_000));
        let mm = r
            .model_metrics
            .as_ref()
            .expect("model_metrics seeded by start-model");
        assert!(mm.contains_key("claude-sonnet-4.6"));
    }

    #[test]
    fn token_usage_rolls_up_into_model_metrics() {
        let p = PersistableSession {
            session_id: "s2".to_owned(),
            close_reason: CloseReason::Shutdown,
            events: vec![
                TypedEvent::TokenUsage {
                    session_id: "s2".to_owned(),
                    model: "claude-sonnet-4.6".into(),
                    direction: TokenDirection::Input,
                    value: 100,
                    timestamp: ts(1_700_000_010),
                },
                TypedEvent::TokenUsage {
                    session_id: "s2".to_owned(),
                    model: "claude-sonnet-4.6".into(),
                    direction: TokenDirection::Output,
                    value: 50,
                    timestamp: ts(1_700_000_020),
                },
                TypedEvent::TokenUsage {
                    session_id: "s2".to_owned(),
                    model: "claude-sonnet-4.6".into(),
                    direction: TokenDirection::Input,
                    value: 30,
                    timestamp: ts(1_700_000_030),
                },
            ],
            started_at: Some(ts(1_700_000_000)),
            ended_at: None,
        };
        let r = persistable_to_report("s2", p);
        let mm = r.model_metrics.expect("model_metrics");
        let usage = &mm["claude-sonnet-4.6"];
        assert_eq!(usage.input_tokens, 130);
        assert_eq!(usage.output_tokens, 50);
    }
}
