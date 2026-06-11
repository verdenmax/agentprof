# ADR-0024: Web Dashboard (`agentprof serve`) Architecture

**Status:** Accepted (2026-06-11)
**Context:** M2.3 web dashboard (`docs/superpowers/specs/2026-06-11-m2.3-web-dashboard-design.md`)
**Implements:** Q-7.2 closure ("纯静态够用，还是要 server 模式") from `docs/plan.md` §7.2
**Supersedes:** None
**Superseded by:** None
**Related:**
- [ADR-0019](adr-0019-hybrid-storage-mode.md) — hybrid storage (store mode is the **required** datasource for `serve`; D-5 below)
- [ADR-0021](adr-0021-otlp-receiver-architecture.md) — axum + tokio HTTP stack reused from the M2.2 OTLP HTTP receiver (D-1 below)
- [ADR-0022](adr-0022-otlp-capacity-caps-and-lru-eviction.md) — capacity hardening pattern (precedent for the loopback-default + warn-on-non-loopback policy in D-6)
- [ADR-0023](adr-0023-cache-metrics.md) — cache analytics surfaced verbatim inside the per-session dashboard chunk (handled by reusing `format::html::render_body_only`)

---

## Context

AgentProf historically exposed session data through **five read surfaces**, all of
which are either terminal-bound or rebuild-required:

1. `analyze --export {md,html,json,speedscope}` — single-session report.
2. `list` — table of sessions.
3. `aggregate --by {tool,mcp-server,day,model}` — cross-session rollup.
4. `mcp-waste` — MCP server waste analysis.
5. TUI views (`watch`, `analyze --export tui`, `aggregate --export tui`).

The TUI is live but tied to a terminal; static HTML reports become stale until
manually rebuilt. There is no comfortable "second monitor" experience — i.e. a
browser tab a user can leave open during an agent session and that updates on
its own.

After M2.1 shipped the hybrid SQLite store (ADR-0019) and M2.2 added the OTLP
HTTP receiver on top of axum + tokio (ADR-0021), the missing piece for a live
browser dashboard is small: a handful of axum routes that read the same SQLite
store, render the existing `format::*` output verbatim, and a few lines of
vanilla JS to poll the chunk endpoints.

This ADR codifies the seven design decisions taken in the M2.3 brainstorm and
records four implementation-time discoveries that fell out of T5–T11.

---

## Decisions

### D-1: Reuse existing axum + askama infrastructure

`agentprof serve` is built on the same axum 0.7 + tokio + tower stack that
M2.2's OTLP HTTP receiver already brought into the build. Templates use
askama 0.16, identical to the existing `format::html` and
`format::aggregate_html` modules.

**Rationale.** Zero new top-level workspace dependencies for the HTTP layer
itself; only `tower-http` (for the `TraceLayer`) and `open` (for the
"launch browser on start" UX) are added on top of what M2.2 + M1.6.4 already
require. Maintainers already know these libraries.

**Alternatives considered.**

- **`warp` / `actix-web` / `rocket`.** Switching frameworks would fragment the
  project's HTTP knowledge surface for no benefit; the dashboard's routing
  needs (~12 routes, path params, static asset extraction) are well inside
  what axum 0.7 already provides.
- **Bare `hyper` + manual routing.** Re-invents the route DSL we already use.
- **Different template engine** (`tera` / `handlebars` / `maud`).
  `format::html` and `format::aggregate_html` are already askama; using
  anything else would require duplicating renderers or a refactor outside
  M2.3's scope.

**Trade-off accepted.** Locking into axum 0.7 means a future migration to
axum 0.8 (path-param syntax change from `:name` to `{name}`) will require a
sweep; not a problem until tonic's MSRV-blocking axum dep is bumped.

### D-2: Vanilla JS poller (no framework)

Live refresh is implemented in ~80 lines of vanilla JavaScript wrapping
`setInterval` + `fetch` + DOM `innerHTML` swap. Bundled via `include_str!`
at compile time; no build step, no `npm`, no transpilation, no SPA shell.

**Rationale.** The "swap a chunk of HTML on a timer" pattern is trivial.
A JavaScript framework would impose a build pipeline disproportionate to
the JS surface. Matches the project's "no JS framework" precedent —
existing HTML reports embed CSS but no JS.

**Alternatives considered.**

- **htmx.** Excellent fit for the swap-HTML-on-poll pattern (~14 KB
  bundled). Reasonable; declined to minimize external surface. If the
  vanilla JS ever grows past ~150 LOC, swap to htmx in a single PR.
- **React / Vue / Svelte / Alpine.** Require a build toolchain (Vite /
  esbuild / etc.) and a runtime VM. Disproportionate for this size.
- **WebSocket / SSE push.** Sub-second latency is unjustified for a
  "monitor my agent" dashboard; polling at 5 s default is responsive
  enough and trivially debuggable.

**Trade-off accepted.** Users who disable JavaScript get a degraded
experience (F5 to refresh); banner explains.

### D-3: HTML-chunk endpoint pattern (not JSON API)

For each dynamic view, the server exposes **two** endpoints:

- `GET /<view>` → full HTML page (chrome + main content).
- `GET /api/<view>.html` → ONLY the `<main>` content fragment.

The vanilla JS poller calls the `/api/*.html` endpoint and replaces
`document.getElementById('main').innerHTML`. No JSON parsing on the client;
no client-side templating; the server reuses the same render code path for
both endpoints — the chrome wraps `render_body_only(...)` on the page
endpoint and emits it bare on the chunk endpoint.

**Rationale.** Server already has all the HTML render code (askama templates
from M1.6.4 + M1.6.5 + M2.5). htmx-style without htmx.

**Alternatives considered.**

- **JSON API + client-side DOM building.** Requires a JS templating library
  or hand-rolled `document.createElement` calls. More code, more bugs,
  duplicates the render logic that already lives in `format::*`.
- **Full-page reload via `<meta http-equiv=refresh>`.** Loses scroll
  position and form state; visible flicker every 5 s; bad UX.
- **One endpoint that toggles JSON vs HTML on `Accept` header.** Adds
  conditional branching to every handler for no real client win — the
  poller is the only consumer that ever needs the chunk form.

**Trade-off accepted.** Two routes per view. The duplication is mechanical
(both call the same render fn; the chunk variant just skips the layout
`render!` wrap) and is exhaustively covered by `router_tests`.

### D-4: Single SQLite read per request (no caching layer)

Every HTTP request hits the SQLite store directly via
`agentprof_storage::query::{query_sessions_since, load_session,
load_episodes}` (M2.1). No in-memory cache layer, no result memoization,
no per-route invalidation logic. The handler acquires the `Arc<Mutex<Db>>`
on entry, runs the query, releases the lock, then renders.

**Rationale.** M2.1's SQLite reads are fast — well under 5 ms for the
typical query shapes (sessions list capped to 200 rows, single-session
load by id). At the default 5 s poll interval (or even at the 1 s minimum)
the per-poll cost is negligible. A cache would introduce staleness bugs
and invalidation complexity for no measurable user-facing win at the scale
the dashboard is designed for (≤200 sessions per typical user store).

**Alternatives considered.**

- **In-memory LRU keyed on query params.** Saves 1–2 ms per repeat poll;
  not worth the complexity.
- **Long-lived in-process state synced from SQLite.** Same trade-off plus
  stale-data risk if external writers (CLI ingest, OTLP receiver) mutate
  the store while serve is running.
- **`ETag` + `If-None-Match` on the chunk endpoints.** Useful optimization
  if poll volume ever becomes a concern; deferred — current scale doesn't
  warrant it.

**Trade-off accepted.** Mutex serializes reads, but rusqlite's WAL mode
(M2.1 default) means SQLite itself supports concurrent reads — the bottleneck
is only the `Arc<Mutex<Db>>` we wrap around the connection handle, which is
single-process and only contended by the poller at low rate.

### D-5: Server requires store mode (no adapter scan fallback)

`agentprof serve` exits with `ExitKind::UserError` (exit code 1) if
`--storage-path` is missing or points to a non-existent file. Users must
first run `agentprof db init` + `agentprof db ingest --agent X` (or
`agentprof ingest-otlp`) to populate the store. There is **no** adapter-scan
fallback that would re-parse JSONL files on every poll.

**Rationale.** The live-refresh pattern (poll every 5 s) is fundamentally
incompatible with adapter scans, which can take seconds for a directory
with hundreds of session JSONLs. Forcing the store-mode dependency makes
per-request latency predictable. Users who don't want to ingest first
already have `analyze --export html` for ad-hoc no-store report generation.

**Alternatives considered.**

- **Auto-bootstrap (run ingest then serve).** Surprising side effects on
  filesystem and DB; user better off doing it explicitly with a clear
  error message ("run `agentprof db ingest --agent copilot --all` first").
- **Adapter fallback that re-parses on every poll.** Degrades responsiveness
  unpredictably; mixes two data paths in a single server; D-4's
  "single SQLite read per request" assumption breaks.
- **Adapter fallback once on startup, snapshot in memory, never refresh.**
  Loses the entire live-refresh point of the dashboard.

**Trade-off accepted.** Extra step for first-time users (run `db ingest`
before `serve`). The error message is explicit and points at the fix.

### D-6: Default port `4329`, loopback only, no auth

The default bind is `127.0.0.1:4329`. Port `4329` is close enough to OTLP's
`4317`/`4318` to be recognizable as agentprof family, and distinct enough
to avoid collision. `--bind 0.0.0.0:N` (or any non-loopback address) is
permitted but emits a `tracing::warn!` recommending a reverse proxy for
authentication. **No bearer, no TLS, no auth in this wave.**

**Rationale.** The primary use case (Q-1: "live monitoring while using an
agent on my own laptop") is single-user-local. Adding auth complexity to a
localhost dashboard is over-engineering. Any future hardening for
non-loopback deployment can mirror the M2.4 OTLP pattern (bearer + TLS +
mTLS) when the need materializes.

**Alternatives considered.**

- **Always require a bearer token, even on loopback.** Friction without
  benefit; users `curl` localhost without auth in any other dev tool.
- **Bind to a Unix socket by default.** Better isolation but breaks the
  "open browser to URL" muscle memory and complicates Windows support
  (which agentprof targets via cargo-dist binaries).
- **Embedded basic auth.** One more config surface to maintain for a
  feature that should be reverse-proxied anyway.

**Trade-off accepted.** Users who bind to LAN expose data with no auth;
the startup warning + README guidance are the mitigation.

### D-7: Release as v0.3.3 (not v0.4.0)

`agentprof serve` is a substantial new feature (new subcommand, new public
API surface in `format::{html, aggregate_html}::render_body_only` + the
`compute_*_from_store` aggregators, new templates, new tests). Under strict
SemVer-for-applications this might justify a minor bump. However, the
post-v0.3.1 numbering decision **reserves v0.4.0 for the Phase 3
multi-agent milestone** (Claude + Codex adapters per `docs/plan.md` §7.3).
M2.3 ships as **v0.3.3** to preserve that reservation, matching the
precedent set by M2.5 cache analytics — which also added public API surface
but shipped as v0.3.1.

**Rationale.** In pre-1.0 territory SemVer is loose enough that "feature
shipped" doesn't dictate major-vs-minor; the project narrative does, and
the narrative reserves v0.4.0 for the multi-agent unlock.

**Alternatives considered.**

- **v0.4.0 now, multi-agent as v0.5.0.** Drifts the reserved-version
  numbering and re-opens a question we already settled at v0.3.1.
- **v0.3.3-alpha.1 / v0.3.3-rc.1 series.** Unnecessary; the wave is
  feature-complete and well-tested. Reserve pre-release tags for cases
  where we explicitly want feedback before locking the public surface.

**Trade-off accepted.** Documented in the CHANGELOG `[0.3.3]` preamble
and cross-referenced from this ADR.

---

## Implementation Notes (post-spec discoveries)

These four notes record concrete decisions made during T5–T11 implementation
that the design spec did not anticipate.

### Note 1: axum 0.7 path-param syntax is `:name`, not `{name}`

The design spec (§5 implementation outline) used `/static/{name}` for path
parameters. That's axum 0.8 syntax. The workspace pins `axum = "0.7"`
because the OTLP receiver (M2.2) was built before axum 0.8 stabilized and
tonic 0.12 (the OTLP gRPC dependency) has not yet bumped its axum
constraint. axum 0.7's `matchit` router requires `/static/:name` instead.

Discovered during T6 implementation (commit `8aebb89`). All routes in
`router.rs` use the `:name` form. Migration to axum 0.8 is deferred until
tonic catches up; the change will be a mechanical sweep of every `.route()`
call.

### Note 2: matchit treats `:server.html` as a single path-param

For routes like `/api/session/:id.html` (T8) and
`/api/mcp-waste/:server` (T10), axum 0.7's `matchit` captures the entire
trailing segment **including** any literal `.html` suffix as the single
parameter value. Handlers strip the `.html` explicitly after extraction.

The simpler-looking alternative (`/api/session-html/:id` with a different
URL shape) was rejected because the T11 vanilla JS poller maps
`window.location.pathname` to its chunk URL by simple string concatenation
(`'/api' + pathname + '.html'`) — keeping the URL shape symmetric between
the page route (`/session/:id`) and the chunk route
(`/api/session/:id.html`) makes the poller a one-liner instead of a
per-view URL builder.

### Note 3: askama 0.16 lacks dedicated framework integration crates

`askama_axum` on crates.io is `0.5.0+deprecated`: askama 0.13 removed the
per-framework integration crates in favor of the direct
`Template::render() -> String` pattern. All dashboard handlers therefore
use `axum::response::Html(template.render()?)` directly. There is no
`impl IntoResponse for SessionsTemplate` shortcut.

Discovered during T1 (commit `78a2423`). The pattern is verbose but
explicit and matches the `format::html` module's existing usage.

### Note 4: `render_body_only` extracted as a thin slice of existing renderers

T8 and T9 added two new public functions:

- `agentprof_cli::cmd::format::html::render_body_only(report) -> Result<String, FormatError>`
- `agentprof_cli::cmd::format::aggregate_html::render_body_only(report) -> Result<String, FormatError>`

Each takes the existing full-page render output (`<!DOCTYPE html>...<html>...
<head><style>...</style></head><body>...</body></html>`) and returns just
the inline `<style>...</style>` block concatenated with the `<body>...</body>`
contents — i.e. exactly what the dashboard chrome needs to embed inside its
own `<main>` element without nested `<html>` or `<head>` blocks.

The implementation uses `str::find` to locate `<style>` / `</body>` markers
in the rendered output. This is slightly hacky compared to a "clean"
refactor that would split the askama templates into reusable partials, but
the slice approach has **zero blast radius** on the M1.6.4 + M1.6.5
static-HTML snapshot tests (insta fixtures pinned byte-for-byte). The
cleaner template refactor was rejected to keep the M2.3 diff surgical;
F4 in Followups tracks the eventual cleanup.

---

## Consequences

**Positive.**

- Live dashboard with five polling views, sub-100 ms per-request latency on
  typical stores (≤200 sessions), zero rebuild required on store update.
- The per-session dashboard view reuses the existing static-HTML report
  markup verbatim — single source of truth for "what does a session report
  look like".
- Crate footprint stays close to M2.2: no new top-level workspace HTTP deps;
  `tower-http` and `open` are the only additions and both are already in
  the `cargo deny` allowlist.
- Cleanly feature-gated under `agentprof-cli/features = ["web"]` (included
  in `full`); `cargo build --no-default-features` still produces a working
  CLI without the HTTP stack.

**Negative.**

- **No authentication** in v0.3.3. D-6 mitigation: loopback default +
  startup warning + reverse-proxy guidance in `crates/agentprof-cli/README.md`.
- **`/aggregate?by=mcp-server` returns HTTP 400** rather than rendering.
  Reason: `--by=mcp-server` is the only aggregate key that needs the MCP
  sidecar config plumbing (`--mcp-config` / `--tool-descriptions`), which
  the store mode currently doesn't capture in a polling-friendly shape.
  The 400 body points the user at `/mcp-waste` for the equivalent view.
- **MCP-waste dashboard view is heuristic-only** (T10). No sidecar
  resolution, no tiktoken-exact token counts. A banner directs users to
  `agentprof mcp-waste --tool-descriptions ...` on the CLI for accurate
  numbers.
- New external dep on `open` (cross-platform `xdg-open` shim) for the
  default "launch browser on start" UX. Suppressible via `--no-open`.

---

## Followups (not blocking v0.3.3 release)

- **F1.** Add `--bind <unix-socket>` for socket-based local access (no port).
  Mitigates D-6's "binding to LAN" risk for users on shared machines while
  keeping the browser-URL UX intact via `curl --unix-socket`.
- **F2.** Add SSE or WebSocket push (bypass polling). Measure first — at the
  current scale (5 s poll, ≤200 sessions, ≤5 ms/request) the win is
  marginal and the complexity cost is real.
- **F3.** Add `--auth-bearer <TOKEN>` for trivial token auth (M2.4 OTLP
  reuse). Promotes from "documented reverse-proxy advice" to "first-class
  flag" once the first user binds non-loopback in earnest.
- **F4.** Address the ratchet on raw HTML scraping in `render_body_only`
  (Note 4 above). If the M3 multi-agent overhaul shifts the template
  structure radically, switch to a clean template refactor that exposes
  `<style>` / `<body>` partials directly to the dashboard chrome instead
  of post-rendering string splicing.
- **F5.** `/api/sessions.html` currently returns all sessions in a single
  HTML table. If users routinely keep dashboards open against stores with
  >500 sessions, add cursor-based pagination + a chunked render path.
- **F6.** Surface ADR-0023 cache analytics in the cross-session aggregate
  dashboard view (today they only appear inside the per-session report
  chunk). Requires designing how to roll cache hit% across sessions —
  currently undefined.
