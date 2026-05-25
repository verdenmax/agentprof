# docs/superpowers/specs — per-feature specifications

This directory holds the **specification** (design document) for each feature
or major change, produced by the `brainstorming` skill before implementation.

Each file is named `YYYY-MM-DD-<topic>-design.md` and is committed alongside
the PR that delivers the design. The corresponding implementation plan
(produced by `writing-plans`) lives in a sibling file
`YYYY-MM-DD-<topic>-plan.md`.

## Workflow

1. `brainstorming` → write `YYYY-MM-DD-<topic>-design.md` here
2. User approves the design
3. `writing-plans` → write `YYYY-MM-DD-<topic>-plan.md` here
4. Implement following the plan (often TDD-driven)
5. Spec stays in this directory as a permanent record of the original
   intent and rationale

## Relationship to other docs

- The **project-wide** architecture is `docs/architecture.md` (L1).
- Specs in this directory are **per-feature** L1/L2 design records — they
  feed into updates to `docs/architecture.md`, `docs/features/`, and
  `docs/internals/`.

## Planned files

- `2026-05-25-agentprof-architecture-design.md` — superseded by
  `docs/architecture.md` (this whole-project skeleton was the brainstorming
  output)
- `YYYY-MM-DD-phase0-prototype.md` — next on the roadmap
