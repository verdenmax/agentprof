# Fixture: with-session-shutdown

**Purpose**: end-to-end test for F1.7 Models view — Copilot CLI session
that emits a `session.shutdown` event carrying `modelMetrics` for two
distinct models. Anchors:

- `Event::payload_model_metrics` extraction (Task 3 of F1.7 plan)
- `derive_episodes` population of `Episodes.model_metrics` (Task 5)
- `analyze()` cloning to `AnalysisReport.model_metrics` (Task 6)
- TUI Models view with-data render branch (Task 11 snapshot)

**Shape**:

- 1 user message
- 1 assistant turn with 1 `bash` tool call (proves derive_episodes
  still produces normal `Episodes.turns` / `Episodes.tools` rollups)
- `session.shutdown` with `modelMetrics` for `claude-opus-4.7-1m-internal`
  (input 98327, output 47523, cache_read 3444639, cache_write 721860)
  and `gpt-5-mini` (input 12500, output 3400, cache_read 8200,
  cache_write 0). Both have `usage` subtree.

**Counterpart fixture for empty-state**: any existing fixture without
a `session.shutdown` event (e.g. `builtin-tools-only`) exercises the
"no model usage data" empty-state render branch.

**Wire shape sources**: 2026-06-03 empirical survey of real
`~/.copilot/session-state/252068e5-…/events.jsonl`. See F1.7 spec §1.
