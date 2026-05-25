# .github/instructions — VS Code Copilot instructions

This directory holds **VS Code Copilot–style instruction files** sourced from
[`github/awesome-copilot`](https://github.com/github/awesome-copilot). Each file
uses YAML frontmatter with an `applyTo` glob and is automatically picked up by
VS Code Copilot when the matching files are open.

For the Copilot **CLI** (the agent that drives this repo's day-to-day work),
these instructions are referenced from [`../copilot-instructions.md`](../copilot-instructions.md)
§5 and §6 and treated as **always-on constant rules**.

## Files

| File | Applies to | Purpose |
|---|---|---|
| `rust.instructions.md` | `**/*.rs` | GitHub-curated idiomatic Rust style: Rust API Guidelines + RFC 430 naming, error handling, API design, testing/documentation, quality checklist |
| `update-docs-on-code-change.instructions.md` | `**/*.{md,rs,...}` | Forces docs-and-code-in-the-same-PR; aligns with `docs/architecture.md` §14 "L1/L2/L3 documentation system" |

## Provenance

Both files are vendored from upstream and carry the upstream MIT License
(`github/awesome-copilot`). They are intentionally **not modified locally** so
that future sync runs from upstream remain conflict-free.

## How they relate to other docs

```
.github/instructions/*.instructions.md
   └─▶ referenced from .github/copilot-instructions.md §5 / §6 (CLI workflow)
   └─▶ referenced from docs/architecture.md §14 / §16 (L1 authority)
   └─▶ auto-picked up by VS Code Copilot (applyTo globs)
```

These are **Stage 0 constants** in the agentprof skill pipeline (see
`.github/copilot-instructions.md` §6.6): they apply on every edit and never
need to be "invoked" — they are read context, not procedural skills.
