//! Integration tests: run `to_speedscope` against committed fixtures.
//!
//! Placed under `agentprof-adapters/tests/` (not `agentprof-core/tests/`)
//! to avoid a dev-dependency cycle: this test needs both
//! `agentprof-adapters` (to load fixtures via `CopilotAdapter`) and
//! `agentprof-core::export::speedscope` (the function under test).
//! Mirrors the structure of `episode_derive.rs` and
//! `analyzer_on_fixtures.rs`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use agentprof_adapters::copilot::CopilotAdapter;
use agentprof_core::adapter::Adapter;
use agentprof_core::episode::{derive_episodes, Episodes};
use agentprof_core::export::speedscope::{to_speedscope, Event, EventType};
use agentprof_core::export::ExportWarning;
use agentprof_core::model::SessionMeta;

fn fixture(slug: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/copilot")
        .join(slug)
}

fn load_fixture(slug: &str) -> (Episodes, SessionMeta) {
    let adapter = CopilotAdapter;
    let root = fixture(slug).parent().unwrap().to_path_buf();
    let sessions = adapter.discover_sessions(&root).expect("discover_sessions");
    let sref = sessions
        .into_iter()
        .find(|s| s.path.parent().unwrap().ends_with(slug))
        .unwrap_or_else(|| panic!("fixture {slug} not discovered"));
    let raw = adapter.load_session(&sref).expect("load_session");
    let episodes = derive_episodes(&raw.events, &raw.meta);
    (episodes, raw.meta)
}

fn nesting_holds(events: &[Event]) -> bool {
    let mut stack: Vec<usize> = Vec::new();
    for ev in events {
        match ev.ty {
            EventType::Open => stack.push(ev.frame),
            EventType::Close => match stack.pop() {
                Some(top) if top == ev.frame => {}
                _ => return false,
            },
        }
    }
    stack.is_empty()
}

#[test]
fn speedscope_strict_nesting_on_with_skill_invoked() {
    let (episodes, meta) = load_fixture("with-skill-invoked");
    let (p, _) = to_speedscope(&episodes, &meta, "0.0.0");
    assert!(
        nesting_holds(&p.profiles[0].events),
        "events not strictly nested: {:?}",
        p.profiles[0].events
    );
}

#[test]
fn speedscope_frame_dedup_across_turns() {
    let (episodes, meta) = load_fixture("cross-turn-tool");
    let (p, _) = to_speedscope(&episodes, &meta, "0.0.0");
    let names: Vec<&str> = p.shared.frames.iter().map(|f| f.name.as_str()).collect();
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for n in &names {
        assert!(seen.insert(n), "duplicate frame name in shared.frames: {n}");
    }
}

#[test]
fn speedscope_overlap_adjustment_warns_on_with_span_overlap() {
    let (episodes, meta) = load_fixture("with-span-overlap");
    let (p, warnings) = to_speedscope(&episodes, &meta, "0.0.0");
    assert_eq!(warnings.len(), 1, "expected one warning, got {warnings:?}");
    assert!(matches!(
        warnings[0],
        ExportWarning::SpanAdjustedForSpeedscope { .. }
    ));
    assert!(
        nesting_holds(&p.profiles[0].events),
        "events not strictly nested after overlap adjustment: {:?}",
        p.profiles[0].events
    );
}

#[test]
fn speedscope_open_turn_frame_name_carries_open_suffix() {
    let (episodes, meta) = load_fixture("with-aborts");
    let (p, _) = to_speedscope(&episodes, &meta, "0.0.0");
    let any_open = p.shared.frames.iter().any(|f| f.name.contains("(open)"));
    if episodes.turns.iter().any(|t| t.ended_at.is_none()) {
        assert!(
            any_open,
            "expected an '(open)' frame name; frames: {:?}",
            p.shared.frames
        );
    }
}

#[test]
fn speedscope_unit_is_milliseconds() {
    let (episodes, meta) = load_fixture("with-skill-invoked");
    let (p, _) = to_speedscope(&episodes, &meta, "0.0.0");
    assert_eq!(p.profiles[0].unit, "milliseconds");
    assert_eq!(p.profiles[0].start_value, 0);
    assert!(p.profiles[0].end_value >= 0);
}

#[test]
fn speedscope_skill_frame_is_aggregated() {
    // All invocations of a single skill collapse to one frame named
    // `skill:<name>` in `shared.frames` (mirrors tool dedup; see D-11
    // and the rustdoc on `to_speedscope`).
    let (episodes, meta) = load_fixture("with-skill-invoked");
    let (p, _) = to_speedscope(&episodes, &meta, "0.0.0");
    let count = p
        .shared
        .frames
        .iter()
        .filter(|f| f.name == "skill:synthetic-example")
        .count();
    assert_eq!(
        count, 1,
        "expected exactly one 'skill:synthetic-example' frame, got {count}; frames: {:?}",
        p.shared.frames
    );
}
