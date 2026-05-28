# xtask

Build / release / maintenance driver for the agentprof workspace.
Follows the [cargo-xtask](https://github.com/matklad/cargo-xtask) convention.

## Subcommands

### `schema-audit`

Audit Copilot CLI session data against the current `CopilotEvent` schema.

```bash
# default: audit ~/.copilot/session-state, print markdown to stdout
cargo run -p xtask -- schema-audit

# write to file
cargo run -p xtask -- schema-audit --output audit-2026-05-27.md

# limit to 50 most recent sessions
cargo run -p xtask -- schema-audit --sample-limit 50

# audit a specific session
cargo run -p xtask -- schema-audit --sessions 252068e5-ca16-4186-a181-719462643d83

# audit a custom root (e.g. test fixtures)
cargo run -p xtask -- schema-audit --root crates/agentprof-adapters/tests/fixtures/copilot
```

The report has four sections: Session 覆盖, Unknown 事件分类 (with candidate Rust
variant names), ParseWarning 分布, 事件类型平衡分析.

Run this after Copilot CLI upgrades to detect schema drift. See
`docs/internals/adr-0002-copilot-event-schema.md` for the variant table
maintained based on these audits.
