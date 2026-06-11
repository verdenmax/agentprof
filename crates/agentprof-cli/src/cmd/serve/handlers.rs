//! HTTP route handlers for the dashboard (M2.3).
//!
//! T5 ships only the liveness probe (`/healthz`); T6+ adds the
//! dynamic view handlers (overview / ROI / waste / aggregate).

use std::time::Duration;

use askama::Template;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

use super::state::AppState;

/// `GET /healthz` — always returns `200 OK` with body `"healthy"`.
///
/// Suitable for liveness probes; no tracing emitted to avoid log
/// spam under load-balancer health checks. Ignores the [`AppState`]
/// but still takes it via the extractor so the type-state matches
/// the rest of the router.
///
/// # Examples
///
/// ```text
/// $ curl -s http://127.0.0.1:4329/healthz
/// healthy
/// ```
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn healthz(State(_state): State<AppState>) -> impl IntoResponse {
    (StatusCode::OK, "healthy")
}

/// `GET /static/:name` — serves bundled CSS / JS / favicon.
///
/// Assets are baked into the binary via `include_str!` / `include_bytes!`
/// (see [`super::static_assets`]). `Cache-Control: immutable` because the
/// assets only change when the agentprof binary itself changes (the
/// browser will only re-fetch on a server upgrade).
///
/// # Examples
///
/// ```text
/// $ curl -sI http://127.0.0.1:4329/static/dashboard.css | head -1
/// HTTP/1.1 200 OK
/// $ curl -sI http://127.0.0.1:4329/static/missing.png | head -1
/// HTTP/1.1 404 Not Found
/// ```
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn static_asset(Path(name): Path<String>) -> impl IntoResponse {
    if let Some((mime, body)) = super::static_assets::lookup(&name) {
        let headers = [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ];
        return (StatusCode::OK, headers, body).into_response();
    }
    (StatusCode::NOT_FOUND, "asset not found").into_response()
}

// ----------------------------------------------------------------------------
// /sessions view (M2.3 T7)
// ----------------------------------------------------------------------------

/// Full-page render for `GET /sessions` (extends the dashboard
/// `layout.html` and includes `chunks/sessions.html`).
#[derive(Template)]
#[template(path = "dashboard/sessions.html")]
struct SessionsPage {
    active_nav: &'static str,
    interval_default: u8,
    version: &'static str,
    sessions: Vec<SessionRow>,
}

/// Chunk-only render for `GET /api/sessions.html` — main-panel
/// fragment used by the JS poller's `innerHTML` swap.
#[derive(Template)]
#[template(path = "dashboard/chunks/sessions.html")]
struct SessionsChunk {
    sessions: Vec<SessionRow>,
}

/// One row in the sessions list view — all fields pre-formatted as
/// `String` so the askama template needs no filters. Mirrors the
/// columns of `agentprof list`.
struct SessionRow {
    id: String,
    id_short: String,
    started_at_utc: String,
    model: String,
    turns: u32,
    out_tokens: u64,
    /// Pre-formatted `"82.7%"` or empty string when the session had
    /// no cache activity (mirrors `agentprof list` empty-cell
    /// behavior). Done in Rust rather than in the template because
    /// the askama `format` filter is unreliable across versions.
    cache_pct_str: String,
    duration: String,
    size_human: String,
}

/// Default look-back window for the sessions view. T11 adds a query
/// param (`?since=...`); T7 hard-codes 30 days for the MVP page so
/// freshly-ingested stores aren't dominated by stale entries.
const DEFAULT_SESSIONS_WINDOW: Duration = Duration::from_secs(30 * 86_400);

/// Maximum rows to render. Keeps the `SQLite` store responsive under
/// polling even with thousands of sessions. T11 adds pagination if
/// needed.
const SESSIONS_RENDER_LIMIT: usize = 200;

#[allow(
    clippy::significant_drop_tightening,
    reason = "db_guard is held across the loop intentionally to amortize lock acquisition"
)]
fn load_sessions(state: &AppState) -> Vec<SessionRow> {
    let db_guard = state
        .db
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let refs = match agentprof_storage::query::query_sessions_since(
        &db_guard,
        DEFAULT_SESSIONS_WINDOW,
        now_ms,
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "query_sessions_since failed");
            return Vec::new();
        }
    };

    let mut rows: Vec<SessionRow> = Vec::with_capacity(refs.len().min(SESSIONS_RENDER_LIMIT));
    for sref in refs.iter().take(SESSIONS_RENDER_LIMIT) {
        match agentprof_storage::query::load_session(&db_guard, &sref.id) {
            Ok(report) => rows.push(session_row_from_report(sref, &report)),
            Err(e) => {
                tracing::warn!(session_id = %sref.id, error = %e, "load_session failed; skipping row");
            }
        }
    }
    rows
}

fn session_row_from_report(
    sref: &agentprof_core::datasource::SessionRef,
    report: &agentprof_core::analyzer::AnalysisReport,
) -> SessionRow {
    let model = report
        .turn_summary
        .iter()
        .find_map(|t| t.model.clone())
        .unwrap_or_else(|| "-".to_owned());
    let turns = u32::try_from(report.turn_summary.len()).unwrap_or(u32::MAX);
    let out_tokens: u64 = report
        .turn_summary
        .iter()
        .filter_map(|t| t.output_tokens)
        .map(u64::from)
        .sum();
    let duration = match (report.turn_summary.first(), report.turn_summary.last()) {
        (Some(first), Some(last)) => Some(last.started_at - first.started_at),
        _ => None,
    };
    let size_bytes = sref
        .raw_path
        .as_deref()
        .and_then(|p| std::fs::metadata(p).ok())
        .map_or(0_u64, |m| m.len());
    let cache_pct_str = report
        .cache_metrics()
        .map(|m| format!("{:.1}%", m.hit_rate_honest_pct))
        .unwrap_or_default();

    SessionRow {
        id: sref.id.clone(),
        id_short: sref.id.chars().take(8).collect(),
        started_at_utc: report.meta.started_at.to_rfc3339(),
        model,
        turns,
        out_tokens,
        cache_pct_str,
        duration: format_chrono_duration(duration),
        size_human: format_size_bytes(size_bytes),
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "display-only formatting for the dashboard sessions list"
)]
fn format_chrono_duration(d: Option<chrono::Duration>) -> String {
    let Some(d) = d else {
        return String::new();
    };
    let ms = d.num_milliseconds();
    if ms < 0 {
        String::new()
    } else if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{:.1}m", ms as f64 / 60_000.0)
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "display-only formatting for the dashboard sessions list"
)]
fn format_size_bytes(bytes: u64) -> String {
    if bytes == 0 {
        "0".to_owned()
    } else if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}k", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}M", bytes as f64 / 1_048_576.0)
    }
}

/// `GET /sessions` — full page (chrome + sessions chunk).
///
/// Renders the dashboard layout with the sessions table embedded.
/// Cache headers are default (no `Cache-Control`) because the full
/// page is the user's entry point; the JS poller refreshes via
/// `/api/sessions.html` (which sets `no-store`).
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn sessions_page(State(state): State<AppState>) -> impl IntoResponse {
    let tmpl = SessionsPage {
        active_nav: "sessions",
        interval_default: state.interval_default,
        version: env!("CARGO_PKG_VERSION"),
        sessions: load_sessions(&state),
    };
    render_html(&tmpl, "sessions_page")
}

/// `GET /api/sessions.html` — chunk-only fragment for the JS poller's
/// `innerHTML` swap. Same data as [`sessions_page`] minus the chrome,
/// plus `Cache-Control: no-store` so the browser always hits the
/// server.
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn sessions_chunk(State(state): State<AppState>) -> impl IntoResponse {
    let tmpl = SessionsChunk {
        sessions: load_sessions(&state),
    };
    render_html_no_store(&tmpl, "sessions_chunk")
}

fn render_html<T: Template>(tmpl: &T, name: &str) -> axum::response::Response {
    match tmpl.render() {
        Ok(html) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html,
        )
            .into_response(),
        Err(e) => {
            tracing::error!(template = %name, error = %e, "render failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "render error").into_response()
        }
    }
}

fn render_html_no_store<T: Template>(tmpl: &T, name: &str) -> axum::response::Response {
    match tmpl.render() {
        Ok(html) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            html,
        )
            .into_response(),
        Err(e) => {
            tracing::error!(template = %name, error = %e, "chunk render failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "render error").into_response()
        }
    }
}

// ----------------------------------------------------------------------------
// /session/:id view (M2.3 T8)
// ----------------------------------------------------------------------------

/// Full-page render for `GET /session/:id` (extends the dashboard
/// `layout.html` and includes `chunks/session.html`).
#[derive(Template)]
#[template(path = "dashboard/session.html")]
struct SessionPage {
    active_nav: &'static str,
    interval_default: u8,
    version: &'static str,
    session_id_short: String,
    agent: String,
    model: String,
    started_at: String,
    duration: String,
    turn_count: usize,
    report_body_html: String,
}

/// Chunk-only render for `GET /api/session/:id.html` — main-panel
/// fragment used by the JS poller's `innerHTML` swap.
#[derive(Template)]
#[template(path = "dashboard/chunks/session.html")]
struct SessionChunk {
    session_id_short: String,
    agent: String,
    model: String,
    started_at: String,
    duration: String,
    turn_count: usize,
    report_body_html: String,
}

/// Outcome of [`load_session_for_dashboard`] — three-valued because
/// the unknown-id path must surface as `404` while real `SQLite`
/// failures must surface as `500`.
enum SessionLoadOutcome {
    Found {
        agent: String,
        model: String,
        started_at: String,
        duration: String,
        turn_count: usize,
        report_body_html: String,
        session_id_short: String,
    },
    NotFound,
    Error(String),
}

#[allow(
    clippy::significant_drop_tightening,
    reason = "db_guard is dropped explicitly before the (heavy) render call"
)]
fn load_session_for_dashboard(state: &AppState, id: &str) -> SessionLoadOutcome {
    let db_guard = state
        .db
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let report = match agentprof_storage::query::load_session(&db_guard, id) {
        Ok(r) => r,
        Err(agentprof_storage::error::SqliteError::Rusqlite { source, .. })
            // `rusqlite` is not a direct dep of agentprof-cli (only
            // pulled in via agentprof-storage), so we match on the
            // `Display` of the underlying error rather than its
            // variant. The exact text is fixed in rusqlite — see
            // `rusqlite::Error`'s `Display` impl for
            // `QueryReturnedNoRows`.
            if source.to_string() == "Query returned no rows" =>
        {
            return SessionLoadOutcome::NotFound;
        }
        Err(e) => {
            tracing::error!(session_id = %id, error = %e, "load_session failed");
            return SessionLoadOutcome::Error(format!("load_session: {e}"));
        }
    };
    let episodes = agentprof_storage::query::load_episodes(&db_guard, id).unwrap_or_default();
    drop(db_guard); // release lock before the (heavy) render

    // Render ALL non-MCP analysis sections — the dashboard chunk
    // should be informationally complete. MCP-waste is deliberately
    // omitted: the /mcp-waste/* views (T10) own that surface and
    // computing the WasteReport from a single stored session is
    // non-trivial without the adapter context.
    let sections = [
        crate::cmd::analyze::AnalysisSection::TurnSummary,
        crate::cmd::analyze::AnalysisSection::ToolRank,
        crate::cmd::analyze::AnalysisSection::HookRank,
    ];
    let report_body_html = crate::cmd::format::html::render_body_only(
        &report,
        &episodes,
        &report.meta,
        &sections,
        None,
        env!("CARGO_PKG_VERSION"),
    );

    let agent = report.meta.agent.to_string();
    let model = report
        .turn_summary
        .iter()
        .find_map(|t| t.model.clone())
        .unwrap_or_else(|| "-".to_owned());
    let started_at = report.meta.started_at.to_rfc3339();
    let duration = match (report.turn_summary.first(), report.turn_summary.last()) {
        (Some(f), Some(l)) => format_chrono_duration(Some(l.started_at - f.started_at)),
        _ => String::new(),
    };
    let turn_count = report.turn_summary.len();
    let session_id_short: String = id.chars().take(8).collect();

    SessionLoadOutcome::Found {
        agent,
        model,
        started_at,
        duration,
        turn_count,
        report_body_html,
        session_id_short,
    }
}

/// `GET /session/:id` — full page (chrome + per-session chunk).
///
/// Renders the dashboard layout with a session meta header followed
/// by the full analytical report body (flamegraph + turn / tool /
/// hook tables + cache section). Returns `404` for unknown ids and
/// `500` for real `SQLite` failures (see [`SessionLoadOutcome`]).
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn session_page(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match load_session_for_dashboard(&state, &id) {
        SessionLoadOutcome::NotFound => {
            (StatusCode::NOT_FOUND, "session not found").into_response()
        }
        SessionLoadOutcome::Error(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response(),
        SessionLoadOutcome::Found {
            agent,
            model,
            started_at,
            duration,
            turn_count,
            report_body_html,
            session_id_short,
        } => {
            let tmpl = SessionPage {
                active_nav: "sessions",
                interval_default: state.interval_default,
                version: env!("CARGO_PKG_VERSION"),
                session_id_short,
                agent,
                model,
                started_at,
                duration,
                turn_count,
                report_body_html,
            };
            render_html(&tmpl, "session_page")
        }
    }
}

/// `GET /api/session/:id.html` — chunk-only fragment for the JS
/// poller's `innerHTML` swap. Same data as [`session_page`] minus
/// the chrome, plus `Cache-Control: no-store`.
///
/// matchit 0.7 (axum 0.7's router) treats `:id.html` as a single
/// parameter whose name is `id.html` and whose value is the whole
/// path segment, so we strip the trailing `.html` here to recover
/// the bare session id.
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn session_chunk(
    State(state): State<AppState>,
    Path(id_with_ext): Path<String>,
) -> impl IntoResponse {
    let id = id_with_ext.strip_suffix(".html").unwrap_or(&id_with_ext);
    match load_session_for_dashboard(&state, id) {
        SessionLoadOutcome::NotFound => {
            (StatusCode::NOT_FOUND, "session not found").into_response()
        }
        SessionLoadOutcome::Error(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response(),
        SessionLoadOutcome::Found {
            agent,
            model,
            started_at,
            duration,
            turn_count,
            report_body_html,
            session_id_short,
        } => {
            let tmpl = SessionChunk {
                session_id_short,
                agent,
                model,
                started_at,
                duration,
                turn_count,
                report_body_html,
            };
            render_html_no_store(&tmpl, "session_chunk")
        }
    }
}

// ----------------------------------------------------------------------------
// /aggregate view (M2.3 T9)
// ----------------------------------------------------------------------------

/// Query string for `GET /aggregate` and `GET /api/aggregate.html`.
///
/// `by` defaults to `"model"` (the most useful single-key rollup for
/// new users); `since` defaults to `"7d"` (matching the documented
/// `agentprof aggregate` CLI default window).
#[derive(serde::Deserialize)]
pub struct AggregateQuery {
    #[serde(default = "default_by")]
    by: String,
    #[serde(default = "default_since")]
    since: String,
}

fn default_by() -> String {
    "model".to_owned()
}
fn default_since() -> String {
    "7d".to_owned()
}

/// Full-page render for `GET /aggregate` (extends `layout.html` and
/// includes `chunks/aggregate.html`).
#[derive(Template)]
#[template(path = "dashboard/aggregate.html")]
struct AggregatePage {
    active_nav: &'static str,
    interval_default: u8,
    version: &'static str,
    by_label: String,
    since_label: String,
    session_count: usize,
    failure_count: usize,
    report_body_html: String,
}

/// Chunk-only render for `GET /api/aggregate.html` — main-panel
/// fragment used by the JS poller's `innerHTML` swap.
#[derive(Template)]
#[template(path = "dashboard/chunks/aggregate.html")]
struct AggregateChunk {
    by_label: String,
    since_label: String,
    session_count: usize,
    failure_count: usize,
    report_body_html: String,
}

/// Outcome of [`load_aggregate_for_dashboard`] — three-valued so
/// `BadRequest` (unknown / unsupported `by` or malformed `since`)
/// surfaces as `400` while real `SQLite` failures surface as `500`.
enum AggregateLoadOutcome {
    Ok {
        by_label: String,
        since_label: String,
        session_count: usize,
        failure_count: usize,
        report_body_html: String,
    },
    BadRequest(String),
    Error(String),
}

fn parse_by(s: &str) -> Result<agentprof_core::analyzer::aggregate::AggregateKey, String> {
    use agentprof_core::analyzer::aggregate::AggregateKey;
    match s {
        "tool" => Ok(AggregateKey::Tool),
        "model" => Ok(AggregateKey::Model),
        "day" => Ok(AggregateKey::Day),
        "mcp-server" => Err(
            "--by=mcp-server requires --tool-descriptions sidecar + MCP config; use /mcp-waste view instead".to_owned(),
        ),
        other => Err(format!(
            "unknown by={other}; use one of: model, tool, day"
        )),
    }
}

#[allow(
    clippy::significant_drop_tightening,
    reason = "db_guard is dropped explicitly before the (heavy) render call"
)]
fn load_aggregate_for_dashboard(state: &AppState, q: &AggregateQuery) -> AggregateLoadOutcome {
    let key = match parse_by(&q.by) {
        Ok(k) => k,
        Err(msg) => return AggregateLoadOutcome::BadRequest(msg),
    };
    let since_dur = match crate::cmd::since::parse_since(&q.since) {
        Ok(d) => d,
        Err(msg) => {
            return AggregateLoadOutcome::BadRequest(format!("invalid since={}: {}", q.since, msg));
        }
    };

    let db_guard = state
        .db
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let report = match crate::cmd::aggregate::compute_aggregate_from_store(
        &db_guard, key, since_dur, 20.0,
    ) {
        Ok(r) => r,
        Err(e) => {
            // BadRequest comes via `ExitKind::UserError` (e.g. the
            // mcp-server rejection inside compute_aggregate_from_store
            // — currently the parse_by gate catches it first, but
            // the defense-in-depth check is cheap).
            if matches!(
                e.downcast_ref::<crate::cmd::exit::ExitKind>(),
                Some(crate::cmd::exit::ExitKind::UserError)
            ) {
                return AggregateLoadOutcome::BadRequest(e.to_string());
            }
            tracing::error!(error = %e, "compute_aggregate_from_store failed");
            return AggregateLoadOutcome::Error(format!("aggregate compute failed: {e}"));
        }
    };
    drop(db_guard);

    let (session_count, failure_count) = match &report {
        agentprof_core::analyzer::aggregate::AnyAggregateReport::Tool(r) => {
            (r.session_count, r.failure_count)
        }
        agentprof_core::analyzer::aggregate::AnyAggregateReport::Model(r) => {
            (r.session_count, r.failure_count)
        }
        agentprof_core::analyzer::aggregate::AnyAggregateReport::Day(r) => {
            (r.session_count, r.failure_count)
        }
        agentprof_core::analyzer::aggregate::AnyAggregateReport::McpServer(r) => {
            (r.session_count, r.failure_count)
        }
        _ => (0, 0),
    };
    let report_body_html = crate::cmd::format::aggregate_html::render_body_only(
        &report,
        20.0,
        env!("CARGO_PKG_VERSION"),
    );

    AggregateLoadOutcome::Ok {
        by_label: q.by.clone(),
        since_label: q.since.clone(),
        session_count,
        failure_count,
        report_body_html,
    }
}

/// `GET /aggregate?by=...&since=...` — full page (chrome + aggregate chunk).
///
/// `by` ∈ `{model, tool, day}` (default `model`). `by=mcp-server`
/// returns `400` with a pointer to `/mcp-waste` because the
/// dashboard's store-mode aggregator does not capture the
/// `--tool-descriptions` sidecar / MCP config plumbing required by
/// mcp-server rollup. Unknown `by` values likewise return `400`.
///
/// `since` defaults to `"7d"` and accepts the same syntax as the
/// `agentprof aggregate --since` flag (see [`crate::cmd::since::parse_since`]).
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn aggregate_page(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<AggregateQuery>,
) -> impl IntoResponse {
    match load_aggregate_for_dashboard(&state, &q) {
        AggregateLoadOutcome::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
        AggregateLoadOutcome::Error(msg) => {
            (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
        }
        AggregateLoadOutcome::Ok {
            by_label,
            since_label,
            session_count,
            failure_count,
            report_body_html,
        } => {
            let tmpl = AggregatePage {
                active_nav: "aggregate",
                interval_default: state.interval_default,
                version: env!("CARGO_PKG_VERSION"),
                by_label,
                since_label,
                session_count,
                failure_count,
                report_body_html,
            };
            render_html(&tmpl, "aggregate_page")
        }
    }
}

/// `GET /api/aggregate.html?by=...&since=...` — chunk-only fragment
/// for the JS poller's `innerHTML` swap. Same data as
/// [`aggregate_page`] minus the chrome, plus `Cache-Control: no-store`.
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn aggregate_chunk(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<AggregateQuery>,
) -> impl IntoResponse {
    match load_aggregate_for_dashboard(&state, &q) {
        AggregateLoadOutcome::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
        AggregateLoadOutcome::Error(msg) => {
            (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
        }
        AggregateLoadOutcome::Ok {
            by_label,
            since_label,
            session_count,
            failure_count,
            report_body_html,
        } => {
            let tmpl = AggregateChunk {
                by_label,
                since_label,
                session_count,
                failure_count,
                report_body_html,
            };
            render_html_no_store(&tmpl, "aggregate_chunk")
        }
    }
}

// ----------------------------------------------------------------------------
// /mcp-waste list + detail views (M2.3 T10)
// ----------------------------------------------------------------------------

/// Query string for `GET /mcp-waste*` routes (list + detail).
///
/// `since` defaults to `"7d"`, matching the documented
/// `agentprof mcp-waste --since` CLI default.
#[derive(serde::Deserialize)]
pub struct McpWasteQuery {
    #[serde(default = "default_since")]
    since: String,
}

/// Per-server row in the `/mcp-waste` list view.
///
/// `tool_usage_len` is pre-computed in the handler because askama
/// templates can't call `.len()` on a `Vec` without the `len` filter
/// (not enabled in this project).
struct ServerRowVm {
    server: String,
    sessions_loaded: usize,
    sessions_with_zero_calls: usize,
    tool_usage_len: usize,
    total_unused_tokens: u64,
}

/// Full-page render for `GET /mcp-waste` (extends `layout.html`).
#[derive(Template)]
#[template(path = "dashboard/mcp_waste_list.html")]
struct McpWasteListPage {
    active_nav: &'static str,
    interval_default: u8,
    version: &'static str,
    since_label: String,
    session_count: usize,
    per_server: Vec<ServerRowVm>,
}

/// Chunk-only render for `GET /api/mcp-waste.html`.
#[derive(Template)]
#[template(path = "dashboard/chunks/mcp_waste_list.html")]
struct McpWasteListChunk {
    since_label: String,
    session_count: usize,
    per_server: Vec<ServerRowVm>,
}

/// Per-tool row in the `/mcp-waste/:server` detail view.
struct ToolRowVm {
    tool_name: String,
    sessions_loaded: usize,
    sessions_called: usize,
    total_call_count: usize,
}

/// Full-page render for `GET /mcp-waste/:server` (extends `layout.html`).
#[derive(Template)]
#[template(path = "dashboard/mcp_waste_detail.html")]
struct McpWasteDetailPage {
    active_nav: &'static str,
    interval_default: u8,
    version: &'static str,
    server_name: String,
    since_label: String,
    session_count: usize,
    sessions_loaded: usize,
    sessions_with_zero_calls: usize,
    total_unused_tokens: u64,
    tool_usage: Vec<ToolRowVm>,
}

/// Chunk-only render for `GET /api/mcp-waste/:server` (the `.html`
/// suffix is captured into the `:server` parameter and stripped by
/// the handler — see [`mcp_waste_detail_chunk`]).
#[derive(Template)]
#[template(path = "dashboard/chunks/mcp_waste_detail.html")]
struct McpWasteDetailChunk {
    server_name: String,
    since_label: String,
    session_count: usize,
    sessions_loaded: usize,
    sessions_with_zero_calls: usize,
    total_unused_tokens: u64,
    tool_usage: Vec<ToolRowVm>,
}

/// Shared loader for both `mcp-waste` routes: parses `since`,
/// acquires the DB lock, calls
/// [`crate::cmd::mcp_waste::compute_aggregate_waste_from_store`], and
/// maps errors onto HTTP status codes (400 for bad `since`, 500 for
/// compute failures).
#[allow(
    clippy::significant_drop_tightening,
    reason = "db_guard is dropped explicitly after the heuristic compute"
)]
fn load_mcp_waste(
    state: &AppState,
    since_str: &str,
) -> Result<agentprof_core::model::AggregateWasteReport, (StatusCode, String)> {
    let since_dur = crate::cmd::since::parse_since(since_str).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid since={since_str}: {e}"),
        )
    })?;
    let db_guard = state
        .db
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let agg = crate::cmd::mcp_waste::compute_aggregate_waste_from_store(&db_guard, since_dur)
        .map_err(|e| {
            tracing::error!(error = %e, "compute_aggregate_waste_from_store failed");
            (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
        })?;
    drop(db_guard);
    Ok(agg)
}

fn server_rows(agg: &agentprof_core::model::AggregateWasteReport) -> Vec<ServerRowVm> {
    agg.per_server
        .iter()
        .map(|s| ServerRowVm {
            server: s.server.clone(),
            sessions_loaded: s.sessions_loaded,
            sessions_with_zero_calls: s.sessions_with_zero_calls,
            tool_usage_len: s.tool_usage.len(),
            total_unused_tokens: s.total_unused_tokens,
        })
        .collect()
}

fn tool_rows(server: &agentprof_core::model::McpServerCrossWaste) -> Vec<ToolRowVm> {
    server
        .tool_usage
        .iter()
        .map(|t| ToolRowVm {
            tool_name: t.tool_name.clone(),
            sessions_loaded: t.sessions_loaded,
            sessions_called: t.sessions_called,
            total_call_count: t.total_call_count,
        })
        .collect()
}

/// `GET /mcp-waste?since=...` — full page (chrome + server-list chunk).
///
/// Heuristic-only mode: no `--tool-descriptions` sidecar, no
/// `mcp.json` config. Banner on the page directs users to the
/// `agentprof mcp-waste` CLI for accurate counts. `since` defaults
/// to `"7d"` and accepts the same syntax as the CLI flag (see
/// [`crate::cmd::since::parse_since`]).
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn mcp_waste_list_page(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<McpWasteQuery>,
) -> impl IntoResponse {
    let agg = match load_mcp_waste(&state, &q.since) {
        Ok(v) => v,
        Err((code, msg)) => return (code, msg).into_response(),
    };
    let tmpl = McpWasteListPage {
        active_nav: "mcp-waste",
        interval_default: state.interval_default,
        version: env!("CARGO_PKG_VERSION"),
        since_label: q.since,
        session_count: agg.sessions,
        per_server: server_rows(&agg),
    };
    render_html(&tmpl, "mcp_waste_list_page")
}

/// `GET /api/mcp-waste.html?since=...` — chunk-only fragment for the
/// JS poller's `innerHTML` swap. Same data as
/// [`mcp_waste_list_page`] minus the chrome, plus `Cache-Control: no-store`.
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn mcp_waste_list_chunk(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<McpWasteQuery>,
) -> impl IntoResponse {
    let agg = match load_mcp_waste(&state, &q.since) {
        Ok(v) => v,
        Err((code, msg)) => return (code, msg).into_response(),
    };
    let tmpl = McpWasteListChunk {
        since_label: q.since,
        session_count: agg.sessions,
        per_server: server_rows(&agg),
    };
    render_html_no_store(&tmpl, "mcp_waste_list_chunk")
}

/// `GET /mcp-waste/:server?since=...` — per-server detail page
/// (tool-usage table).
///
/// Returns `404` when the named server is not present in the
/// `since`-window aggregate.
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn mcp_waste_detail_page(
    State(state): State<AppState>,
    Path(server): Path<String>,
    axum::extract::Query(q): axum::extract::Query<McpWasteQuery>,
) -> impl IntoResponse {
    let agg = match load_mcp_waste(&state, &q.since) {
        Ok(v) => v,
        Err((code, msg)) => return (code, msg).into_response(),
    };
    let Some(found) = agg.per_server.iter().find(|s| s.server == server) else {
        return (
            StatusCode::NOT_FOUND,
            format!("server {server} not found in window"),
        )
            .into_response();
    };
    let tmpl = McpWasteDetailPage {
        active_nav: "mcp-waste",
        interval_default: state.interval_default,
        version: env!("CARGO_PKG_VERSION"),
        server_name: found.server.clone(),
        since_label: q.since,
        session_count: agg.sessions,
        sessions_loaded: found.sessions_loaded,
        sessions_with_zero_calls: found.sessions_with_zero_calls,
        total_unused_tokens: found.total_unused_tokens,
        tool_usage: tool_rows(found),
    };
    render_html(&tmpl, "mcp_waste_detail_page")
}

/// `GET /api/mcp-waste/:server?since=...` — chunk-only fragment for
/// the JS poller's `innerHTML` swap.
///
/// matchit 0.7 (axum 0.7's router) treats `:server.html` as a single
/// parameter whose value is the whole path segment, so we strip the
/// trailing `.html` here to recover the bare server name (mirrors
/// the [`session_chunk`] deviation pattern).
#[allow(clippy::unused_async, reason = "axum handler signature requires async")]
pub async fn mcp_waste_detail_chunk(
    State(state): State<AppState>,
    Path(server_raw): Path<String>,
    axum::extract::Query(q): axum::extract::Query<McpWasteQuery>,
) -> impl IntoResponse {
    let server = server_raw
        .strip_suffix(".html")
        .unwrap_or(&server_raw)
        .to_owned();
    let agg = match load_mcp_waste(&state, &q.since) {
        Ok(v) => v,
        Err((code, msg)) => return (code, msg).into_response(),
    };
    let Some(found) = agg.per_server.iter().find(|s| s.server == server) else {
        return (
            StatusCode::NOT_FOUND,
            format!("server {server} not found in window"),
        )
            .into_response();
    };
    let tmpl = McpWasteDetailChunk {
        server_name: found.server.clone(),
        since_label: q.since,
        session_count: agg.sessions,
        sessions_loaded: found.sessions_loaded,
        sessions_with_zero_calls: found.sessions_with_zero_calls,
        total_unused_tokens: found.total_unused_tokens,
        tool_usage: tool_rows(found),
    };
    render_html_no_store(&tmpl, "mcp_waste_detail_chunk")
}
