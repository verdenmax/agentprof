# docs/internals — L3 implementation notes and ADRs

This directory holds **L3 documentation** that does not belong in rustdoc:

- Algorithm explanations too long for a `///` block
- Architecture Decision Records (ADRs) — *why* something is the way it is, what
  was considered, what was rejected
- Cross-cutting technical investigations (file format reverse-engineering,
  performance characterizations)

Note: For function- and type-level documentation, **prefer rustdoc** (`///` +
`# Examples` + `# Errors` + `# Panics`). This directory is for material that
genuinely benefits from being separate from source.

## ADR template

```markdown
# <Topic>

## Context
What problem are we solving, why now?

## Considered options
1. Option A — pros / cons
2. Option B — pros / cons

## Decision
What was chosen, why.

## Consequences
Benefits, costs, follow-ups, escape hatches.
```

See [`docs/architecture.md`](../architecture.md) §14.4 for the full L3 spec.

## Planned files

- `waste-formula.md` — derivation of `waste_estimate_usd`
- `tokenizer-strategy.md` — `cl100k_base` approximation vs Anthropic API trade-offs
- `adapter-wire-format.md` — how each agent serializes tools into the prompt
- `panic-safe-tui.md` — terminal raw-mode restoration on panic
