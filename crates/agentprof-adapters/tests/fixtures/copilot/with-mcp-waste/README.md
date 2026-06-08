# with-mcp-waste

Synthetic Copilot CLI session that exercises the M1.6.5 MCP waste
analyzer. Used by:

- `crates/agentprof-adapters/tests/analyzer_on_fixtures.rs` (snapshot)
- `crates/agentprof-cli/tests/mcp_waste.rs` (subcommand integration)
- doctests in `agentprof_core::analyzer::waste`

## Shape

| Element | Value |
|---|---|
| Tools loaded (via `<tools_changed_notice>`) | `mcp__github__search_issues`, `mcp__github__create_issue`, `mcp__filesystem__read_file` |
| Tools actually called | `mcp__filesystem__read_file` (1×) |
| Expected `WasteReport` | github: 2 loaded / 0 called / fully_unused=true · filesystem: 1 loaded / 1 called |

## Notes

- The `<tools_changed_notice>` block lives inside
  `user.message.data.transformedContent` per the real wire format
  discovered in the 2026-06-08 audit.
- `transformedContent` deliberately contains content AROUND the
  notice block (the user's actual prompt "hi") to verify the parser
  tolerates mid-text positioning per `find_tools_changed_notices`'s
  contract.
- Session id `0...099` (last two digits `99`) signals "fixture #99"
  in the deterministic-uuid convention used by the other copilot
  fixtures.
