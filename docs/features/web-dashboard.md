# Web Dashboard (M2.3 — `agentprof serve`)

> **One-line:** spin up `agentprof serve` to get a localhost web dashboard
> that auto-polls your SQLite-backed agent activity in real time.

This is the **L2 cross-crate feature index** for the M2.3 web dashboard.
The authoritative L3 design rationale lives in
[ADR-0024](../internals/adr-0024-web-dashboard-architecture.md); the L1
CLI / dependency / feature-gate facts live in
[`docs/architecture.md`](../architecture.md) §3, §8, §14.4, §15.4.

---

## Crate landscape

| Crate | Role in M2.3 | Files touched |
|---|---|---|
| `agentprof-cli` | CLI dispatcher + axum router + handlers + templates + JS poller. **All M2.3 code lives here**, gated on the `web` feature. | `src/cmd/serve/{mod,state,router,handlers,static_assets}.rs` + `templates/dashboard/**/*.html` + `templates/dashboard/static/{dashboard.css,dashboard.js,favicon.svg}` |
| `agentprof-core` | Reused `analyzer::aggregate` + `analyzer::waste::{compute_waste, aggregate_waste}` + the `AnalysisReport` model. | No new code. |
| `agentprof-storage` | Reused `query::{query_sessions_since, load_session, load_episodes}` from M2.1. | No new code. |
| `agentprof-tui`, `agentprof-adapters` | Untouched. | — |

The dashboard is a **dispatch + render layer** on top of existing core
analytics — no new core abstractions were introduced, and no lib crate
gained new public API.

---

## Quick start

```bash
# 1. Ingest some sessions into the SQLite store (one-time per checkout)
agentprof db init --storage-path ~/.local/share/agentprof/store.sqlite
agentprof db ingest --agent copilot --all --storage-path ~/.local/share/agentprof/store.sqlite

# 2. Start the dashboard
agentprof serve --storage-path ~/.local/share/agentprof/store.sqlite

# 3. Browser opens automatically to http://127.0.0.1:4329/sessions
#    Suppress with --no-open.
```

The toolbar in the header lets the user pause polling or change the poll
interval (1 s / 2 s / 5 s / 10 s / 30 s); the choice persists in
`localStorage` under the key `agentprof.interval`.

---

## Architecture summary

Five dashboard views, each with two endpoints (full page + chunk for the
JS poller), plus `/healthz` and `/static/:name`. Every request:

1. Acquires the `Arc<Mutex<Db>>` held by `serve::state::ServeState`.
2. Runs a single SQLite read via the M2.1 `query::*` API.
3. Releases the lock.
4. Renders an askama template to `String`.
5. Returns `axum::response::Html(...)` (no `askama_axum`, see ADR-0024
   Note 3).

Templates extend the shared `dashboard/layout.html` chrome on the page
route, and bypass the chrome on the chunk route (chunk = `<style>` +
`<body>`-content only; the JS poller drops it straight into
`#main.innerHTML`).

See [ADR-0024](../internals/adr-0024-web-dashboard-architecture.md) for
the seven design decisions (D-1..D-7) and the four implementation notes.

---

## Endpoints

| Path | Returns |
|---|---|
| `GET /` | 303 → `/sessions` |
| `GET /sessions` | Sessions list page (chrome + chunk; 30 d window, capped at 200 rows) |
| `GET /api/sessions.html` | Sessions list chunk (`#main` innerHTML target) |
| `GET /session/:id` | Per-session page (chrome + chunk embedding `format::html::render_body_only`) |
| `GET /api/session/:id.html` | Per-session chunk |
| `GET /aggregate?by=model\|tool\|day&since=7d` | Cross-session aggregate page |
| `GET /api/aggregate.html?by=...&since=...` | Aggregate chunk |
| `GET /mcp-waste?since=7d` | MCP-waste server list page (heuristic-only) |
| `GET /api/mcp-waste.html?since=...` | MCP-waste list chunk |
| `GET /mcp-waste/:server?since=7d` | Per-server MCP-waste detail page |
| `GET /api/mcp-waste/:server?since=...` | Per-server detail chunk |
| `GET /healthz` | Liveness probe — `200 OK` with body `healthy` |
| `GET /static/:name` | Bundled CSS / JS / favicon (compile-time `include_str!` / `include_bytes!`) |

**Note.** `/aggregate?by=mcp-server` returns `HTTP 400` with a body that
points at `/mcp-waste`; see ADR-0024 D-3 + Consequences for the rationale
(MCP sidecar plumbing not yet store-mode-friendly).

---

## Config

Two config surfaces:

- **CLI flags** — `--storage-path`, `--bind`, `--interval-default`,
  `--no-open`, `--quiet`. See
  [`crates/agentprof-cli/README.md`](../../crates/agentprof-cli/README.md#agentprof-serve)
  for the exact flag table.
- **`[serve]` block in `agentprof.toml`** — `bind` / `interval_default` /
  `auto_open`. Precedence: CLI flag > file > built-in default. The
  storage path is separate: pass `--storage-path` or set
  `AGENTPROF_STORAGE_PATH`; `[storage].path` is not used by `serve`.

```toml
# Example $XDG_CONFIG_HOME/agentprof/config.toml fragment
[serve]
bind             = "127.0.0.1:4329"
interval_default = 5
auto_open        = true
```

---

## Feature gate

Cargo: `agentprof-cli/features = ["web"]` (included in `full`, which is
the default). Disabling the feature removes the `cmd::serve` module tree,
the askama dashboard templates, and the bundled `dashboard.css` /
`dashboard.js` / `favicon.svg` — plus the runtime deps `axum`, `tower`,
`tower-http`, `tokio` (shared with `otlp` when both are on), and `open`.

A minimal build that omits both dashboards and OTLP:

```bash
cargo build -p agentprof-cli --no-default-features
```

---

## Caveats & known limitations

- **No authentication** (ADR-0024 D-6). Default bind is loopback;
  non-loopback bind logs a `tracing::warn!` recommending a reverse proxy.
- **`--by=mcp-server` returns HTTP 400** in the aggregate view; use the
  `/mcp-waste` views instead.
- **MCP-waste view is heuristic-only** — no MCP sidecar resolution, no
  `--tool-descriptions` plumbing in the dashboard. A banner directs users
  to the CLI `agentprof mcp-waste` for accurate counts.
- **Store mode is required.** Missing or non-existent `--storage-path`
  exits `ExitKind::UserError`; ingest first with `agentprof db ingest`
  (or via `agentprof ingest-otlp`).
- **v0.3.3 only** — Phase 3 multi-agent (Claude + Codex adapters) remains
  reserved for v0.4.0; the dashboard today reads whatever the store
  contains, which in practice is Copilot CLI + any OTLP-pushed sessions.

---

## Tests

Two test suites cover M2.3 end-to-end:

| Suite | File | Pattern | Count |
|---|---|---|---|
| Router unit tests (`tower::ServiceExt::oneshot`) | `crates/agentprof-cli/src/cmd/serve/router_tests.rs` (in `#[cfg(test)] mod`) | In-process Axum `Router::oneshot` with a fixture-populated `Arc<Mutex<Db>>`; per-route status / content-type / body-shape assertions + 2 insta snapshots (sessions-empty-store, session-page-fixture). | ~24 |
| E2E integration tests | `crates/agentprof-cli/tests/cli_serve_e2e.rs` | Spawn `agentprof serve` as a real subprocess on an ephemeral port; probe with `reqwest`. Covers healthz roundtrip, missing / nonexistent storage UserError, sessions page render, static asset MIME types. | 5 |

---

## Change history

See [`CHANGELOG.md`](../../CHANGELOG.md) `[0.3.3]` for the per-task wave
log (T1 → T12). The full Stage-1 spec is at
[`docs/superpowers/specs/2026-06-11-m2.3-web-dashboard-design.md`](../superpowers/specs/2026-06-11-m2.3-web-dashboard-design.md);
the Stage-3 plan is at
[`docs/superpowers/plans/2026-06-11-m2.3-web-dashboard.md`](../superpowers/plans/2026-06-11-m2.3-web-dashboard.md).
