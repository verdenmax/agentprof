//! Write or overwrite a single session's three rows atomically.
//!
//! [`upsert_report`] wraps the parent `sessions` row plus the per-row
//! `tools_loaded` and `turn_buckets` fan-outs in a single `SQLite`
//! transaction. Either all three table writes commit, or none do.
//!
//! ## Why explicit `DELETE` for child tables
//!
//! `INSERT OR REPLACE` on the parent `sessions` row triggers `ON CONFLICT
//! REPLACE` semantics — but the `ON DELETE CASCADE` on `tools_loaded` /
//! `turn_buckets` only fires when the parent row is *actually deleted*,
//! which `OR REPLACE` does **not** do on a same-PK update (it updates in
//! place, no row delete). To make re-upserts idempotent (e.g. a session
//! that used to have 3 tools is re-analyzed and now has 0), we must
//! `DELETE FROM <child> WHERE session_id = ?` before re-inserting.
//!
//! The whole sequence runs inside a transaction, so observers either see
//! the old contents or the new contents — never an inconsistent middle.
//!
//! ## Schema-vs-domain notes (deviations from plan §T2.4 skeleton)
//!
//! The plan's skeleton assumed an older `AnalysisReport` shape with
//! `tool_metrics: BTreeMap<String, ToolMetric>` / `turns: Vec<Turn>` /
//! `meta.raw_path` / `meta.duration_ms`. The actual M1.6.6 model is:
//!
//! - `tool_rank: Vec<ToolRankRow>` — no token info; `tokens` /
//!   `token_source` columns are written as `NULL`.
//! - `turn_summary: Vec<TurnSummaryRow>` — has `output_tokens` + `model`
//!   only; `input_tokens` / `cache_read` / `cache_creation` columns are
//!   written as `NULL`.
//! - `meta.raw_path` does not exist; the source path is passed in as a
//!   separate `raw_path: &Path` parameter (callers — write-through in
//!   T5.3, `ingest-files` in T5.x — have it on hand).
//! - `meta.started_at: DateTime<Utc>` → `started_at_ms` via
//!   `.timestamp_millis()`.
//! - `duration_ms` column is left `NULL` for now; a future task may
//!   derive it from `turn_summary`.
//! - `dominant_model` selection mirrors `agentprof-cli`'s
//!   `cmd::model_hint::dominant_model` (max by [`ModelUsage::total`],
//!   tie-break by ascending name).
//!
//! [`ModelUsage::total`]: agentprof_core::analyzer::ModelUsage::total

use std::path::Path;

use rusqlite::params;

use agentprof_core::analyzer::AnalysisReport;

use crate::{error::SqliteError, Db};

/// Insert-or-replace one session's row in `sessions` plus all derived rows
/// in `tools_loaded` and `turn_buckets`, atomically.
///
/// `raw_path` is the source `events.jsonl` file the report was parsed
/// from; its filesystem `mtime` is read (best-effort) and stored in
/// `sessions.raw_mtime` for the dual-path freshness compare. If the file
/// cannot be `stat`ed, `raw_mtime` falls back to `0` rather than
/// failing the upsert (the warning surface lives at the caller layer in
/// T8).
///
/// `ingested_at_secs` is the unix-epoch (seconds) write time, supplied
/// by the caller — typically `chrono::Utc::now().timestamp()` so unit
/// tests can pin it. It is multiplied by 1000 internally to match the
/// `INTEGER` millisecond convention of `sessions.ingested_at`.
///
/// Returns `1` on success (one `sessions` row written or replaced).
///
/// # Errors
///
/// - [`SqliteError::Serde`] if the full report cannot be serialized into
///   `analysis_report_json`.
/// - [`SqliteError::Rusqlite`] on any underlying transaction, `DELETE`,
///   or `INSERT` failure (the transaction is rolled back on error via
///   `Drop`).
///
/// # Examples
///
/// ```no_run
/// use agentprof_storage::{Db, upsert::upsert_report};
/// use agentprof_core::analyzer::AnalysisReport;
/// use agentprof_core::adapter::AgentKind;
/// use agentprof_core::model::SessionMeta;
/// use chrono::Utc;
/// use std::path::Path;
///
/// let mut db = Db::open_in_memory().unwrap();
/// let report = AnalysisReport::new(
///     SessionMeta::new("session-1".into(), AgentKind::Copilot, Utc::now(), false),
/// );
/// let n = upsert_report(
///     &mut db,
///     &report,
///     Path::new("/tmp/session.jsonl"),
///     Utc::now().timestamp(),
/// ).unwrap();
/// assert_eq!(n, 1);
/// ```
#[allow(clippy::too_many_lines)]
pub fn upsert_report(
    db: &mut Db,
    report: &AnalysisReport,
    raw_path: &Path,
    ingested_at_secs: i64,
) -> Result<usize, SqliteError> {
    let json = serde_json::to_string(report).map_err(|source| SqliteError::Serde {
        context: "serialize AnalysisReport".to_owned(),
        source,
    })?;

    let meta = &report.meta;
    let agent_str = meta.agent.to_string();
    let raw_path_str = raw_path.to_string_lossy().into_owned();
    let raw_mtime_ms = raw_mtime_ms(raw_path);
    let dominant_model = dominant_model(report);
    let started_at_ms = meta.started_at.timestamp_millis();
    let ingested_at_ms = ingested_at_secs.saturating_mul(1000);

    let tx = db
        .conn_mut()
        .transaction()
        .map_err(|source| SqliteError::Rusqlite {
            context: "begin transaction".to_owned(),
            source,
        })?;

    // --- sessions (parent) ---
    tx.execute(
        "INSERT OR REPLACE INTO sessions (
            id, agent, dominant_model, started_at, duration_ms,
            raw_path, raw_mtime,
            total_input_tokens, total_output_tokens, total_cache_read, total_cache_creation,
            schema_version, ingested_at, analysis_report_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            meta.id,
            agent_str,
            dominant_model,
            started_at_ms,
            None::<i64>, // duration_ms — not yet derived (future task)
            raw_path_str,
            raw_mtime_ms,
            report.total_input_tokens(),
            report.total_output_tokens(),
            report.total_cache_read(),
            report.total_cache_creation(),
            1_i64, // schema_version
            ingested_at_ms,
            json,
        ],
    )
    .map_err(|source| SqliteError::Rusqlite {
        context: "INSERT sessions".to_owned(),
        source,
    })?;

    // --- tools_loaded — explicit clear, then re-insert. OR REPLACE on
    //     parent does NOT cascade child deletes (FK CASCADE only fires
    //     on actual row delete, not in-place update). ---
    tx.execute(
        "DELETE FROM tools_loaded WHERE session_id = ?1",
        params![meta.id],
    )
    .map_err(|source| SqliteError::Rusqlite {
        context: "DELETE tools_loaded".to_owned(),
        source,
    })?;
    for row in &report.tool_rank {
        tx.execute(
            "INSERT INTO tools_loaded (
                session_id, tool_name, source, call_count, total_duration_ms,
                tokens, token_source
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                meta.id,
                row.name,
                row.source.to_string(),
                i64::try_from(row.call_count).unwrap_or(i64::MAX),
                row.total_duration.num_milliseconds(),
                None::<i64>,    // tokens — not tracked on ToolRankRow yet
                None::<String>, // token_source
            ],
        )
        .map_err(|source| SqliteError::Rusqlite {
            context: "INSERT tools_loaded".to_owned(),
            source,
        })?;
    }

    // --- turn_buckets — same explicit clear semantics. ---
    tx.execute(
        "DELETE FROM turn_buckets WHERE session_id = ?1",
        params![meta.id],
    )
    .map_err(|source| SqliteError::Rusqlite {
        context: "DELETE turn_buckets".to_owned(),
        source,
    })?;
    for (idx, turn) in report.turn_summary.iter().enumerate() {
        tx.execute(
            "INSERT INTO turn_buckets (
                session_id, turn_index, input_tokens, output_tokens,
                cache_read, cache_creation, model
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                meta.id,
                i64::try_from(idx).unwrap_or(i64::MAX),
                None::<i64>, // input_tokens — not on TurnSummaryRow
                turn.output_tokens.map(i64::from),
                None::<i64>, // cache_read
                None::<i64>, // cache_creation
                turn.model,
            ],
        )
        .map_err(|source| SqliteError::Rusqlite {
            context: "INSERT turn_buckets".to_owned(),
            source,
        })?;
    }

    tx.commit().map_err(|source| SqliteError::Rusqlite {
        context: "commit upsert_report transaction".to_owned(),
        source,
    })?;
    Ok(1)
}

/// Best-effort `mtime` (ms since epoch) for `path`. Returns `0` on any
/// I/O failure — the caller (T8 dual-path warning surface) is
/// responsible for noticing the suspicious value if it matters.
fn raw_mtime_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0_i64, |d| {
            let ms = d.as_millis();
            // Clamp at i64::MAX; SQLite INTEGER is signed 64-bit.
            i64::try_from(ms.min(i64::MAX as u128)).unwrap_or(i64::MAX)
        })
}

/// Mirror of `agentprof-cli::cmd::model_hint::dominant_model` (M1.6.6):
/// largest [`ModelUsage::total`] wins, ascending-name tiebreak.
///
/// Duplicated here (rather than depending on `agentprof-cli`) to keep
/// the `lib → bin` dependency arrow one-way per the architecture rules.
/// The CLI helper remains the canonical user-facing tokenizer hint;
/// this copy services only the storage-layer column write.
///
/// [`ModelUsage::total`]: agentprof_core::analyzer::ModelUsage::total
fn dominant_model(report: &AnalysisReport) -> Option<String> {
    report.model_metrics.as_ref().and_then(|m| {
        m.iter()
            .max_by(|a, b| a.1.total().cmp(&b.1.total()).then_with(|| b.0.cmp(a.0)))
            .map(|(name, _)| name.clone())
    })
}
