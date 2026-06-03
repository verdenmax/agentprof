# Fixture: with-ask-user-mid-session

## Purpose
B-7 (M1.6.4 follow-up wave, 2026-06-03): regression-lock for the
`b5c1429` `FlamegraphView::max_dur` fix. A 3-turn session where the
middle turn calls the user-blocking `ask_user` builtin and the user
takes ~10 minutes to answer, sandwiched between two fast 5-second
turns. The 120× wall-time ratio is THE scenario where, without the
fix, `max_dur` would be dominated by the blocking turn and every
normal turn would scale to ~0 cells in the gantt.

## Scenarios covered
- Turn 0 (warm-up): one `bash` call (`ls`), turn duration ≈ 5 s
  (start 10:00:00.6 → end 10:00:05.0).
- Turn 1 (user-blocking): one `ask_user` call; the `tool.execution_complete`
  arrives 10 minutes after `tool.execution_start` because the user is
  thinking. Turn duration ≈ 605 s (start 10:00:10.5 → end 10:10:15.0).
- Turn 2 (follow-up): one `bash` call (`echo done`), turn duration
  ≈ 5 s (start 10:10:30.5 → end 10:10:35.0).
- Wall-time ratio between Turn 1 and Turns 0/2 is ~120×.

## FRs / fixes exercised
- FR-2.3 (tool classification: `ask_user` → `ToolSource::Builtin`,
  `is_user_blocking = true` in `tool_rank`).
- FR-2.6 path-through: no skills, so the renderer's skill path is
  unexercised — that's intentional, this fixture isolates the
  user-blocking scenario.
- `b5c1429` fix: `agentprof-tui::views::flamegraph::render` excludes
  user-blocking turns from `max_dur` scaling. End-to-end coverage was
  missing — this fixture closes the loop.

## Expected
- 20 events, 0 parse warnings.
- 3 turns, all `TurnStatus::Completed`.
- Exactly 1 turn satisfies `Turn::is_user_blocking() == true` (Turn 1).
- Wall-time of the blocking turn is ≥ 10× the wall-time of either
  normal turn (actual ratio ≈ 121×).
- `tool_rank` has 2 rows: `bash` (not user-blocking) and `ask_user`
  (user-blocking).
- HTML / Speedscope / Markdown renderers all show clearly distinct
  durations between the 3 turns; the markdown Turn Summary table
  displays ~5 s / ~605 s / ~5 s.
