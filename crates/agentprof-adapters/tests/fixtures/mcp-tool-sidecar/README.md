# mcp-tool-sidecar fixture

Synthetic MCP tool-description sidecars used by M1.6.6 token-cost tests.

## Layout

```
mcp-tool-sidecar/
├── global.json              ← Format A (single-file global JSON)
└── per-server/              ← Format B (one file per server)
    ├── github.json          ←   uses {"tools": [...]} wrapper
    └── filesystem.json      ←   uses bare [...] array
```

## Coverage

- Both file & dir formats (auto-detect by path type)
- Both per-server file shapes (`{"tools": [...]}` and bare `[...]`)
- 3 tools across 2 servers — small enough for snapshot pinning, large
  enough that token counts vary across tools (~50-200 each)

## Used by

- `crates/agentprof-adapters/tests/tool_sidecar.rs` — integration tests
- `crates/agentprof-cli/tests/mcp_waste.rs` — `--tool-descriptions` snapshot

## Token count expectations (cl100k_base, full-entry serialized JSON)

| Tool                              | Approx tokens |
|-----------------------------------|--------------:|
| github · search_issues (long)     |        ~130   |
| github · create_issue             |         ~70   |
| filesystem · read_file            |         ~50   |

Actual values are pinned in test snapshots; consult those for exact figures.
