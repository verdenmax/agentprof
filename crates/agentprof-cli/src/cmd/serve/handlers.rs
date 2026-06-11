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
