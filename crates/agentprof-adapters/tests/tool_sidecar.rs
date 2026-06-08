//! Integration tests for `copilot::tool_sidecar::load_sidecar` against the
//! `mcp-tool-sidecar` fixture (M1.6.6 T2.2).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use agentprof_adapters::copilot::tool_sidecar::load_sidecar;

#[test]
fn tool_sidecar_global_file_loads_3_tools() {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mcp-tool-sidecar/global.json");
    let s = load_sidecar(&p).expect("load");
    assert_eq!(s.len(), 3);
    assert!(s.lookup_concrete("mcp__github__search_issues").is_some());
    assert!(s.lookup_concrete("mcp__github__create_issue").is_some());
    assert!(s.lookup_concrete("mcp__filesystem__read_file").is_some());
}

#[test]
fn tool_sidecar_per_server_dir_loads_3_tools_mixed_shapes() {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mcp-tool-sidecar/per-server");
    let s = load_sidecar(&p).expect("load");
    assert_eq!(s.len(), 3);
    assert!(s.lookup_concrete("mcp__github__search_issues").is_some());
    assert!(s.lookup_concrete("mcp__filesystem__read_file").is_some());
}
