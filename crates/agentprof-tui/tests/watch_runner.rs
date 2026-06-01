//! `WatchRunner` integration tests (M1.6.3). Static mode (no refresh channel)
//! is covered here; live-watcher tests are deferred to manual smoke per
//! spec D-14.

#![allow(clippy::unwrap_used)]

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::aggregate::{
    AggregateKey, AggregateReport, AnyAggregateReport, ToolBucket,
};
use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::episode::Episodes;
use agentprof_core::model::SessionMeta;
use agentprof_tui::watch::{WatchData, WatchRunner};
use chrono::{Duration, Utc};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn fake_single() -> WatchData {
    let meta = SessionMeta::new("s-abc".into(), AgentKind::Copilot, Utc::now(), false);
    let report = AnalysisReport::new(meta.clone());
    let episodes = Episodes::new();
    WatchData::Single {
        report,
        episodes,
        meta,
    }
}

#[allow(clippy::missing_const_for_fn)]
fn fake_cross_tool() -> WatchData {
    let inner: AggregateReport<ToolBucket> = AggregateReport::new(
        AggregateKey::Tool,
        Duration::days(30),
        0,
        0,
        Duration::zero(),
        Vec::new(),
    );
    WatchData::Cross(AnyAggregateReport::Tool(inner))
}

#[test]
fn constructs_in_static_single_mode() {
    let runner = WatchRunner::new_static(fake_single());
    assert_eq!(runner.refresh_count(), 0);
    assert!(runner.last_error().is_none());
    assert!(matches!(runner.data(), WatchData::Single { .. }));
}

#[test]
fn constructs_in_static_cross_mode() {
    let runner = WatchRunner::new_static(fake_cross_tool());
    assert_eq!(runner.refresh_count(), 0);
    assert!(matches!(runner.data(), WatchData::Cross(_)));
}

#[test]
fn refresh_swaps_data_and_increments_count() {
    use std::sync::mpsc::channel;

    let (tx, rx) = channel();
    let reload: Box<dyn FnMut() -> Result<WatchData, agentprof_tui::watch::ReloadError>> =
        Box::new(move || Ok(fake_single()));

    let mut runner = WatchRunner::with_watcher(fake_cross_tool(), rx, reload);
    assert!(matches!(runner.data(), WatchData::Cross(_)));

    tx.send(agentprof_tui::watch::RefreshKind::DataChanged)
        .unwrap();

    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    runner.run_one_iteration_for_test(&mut term).unwrap();
    assert!(matches!(runner.data(), WatchData::Single { .. }));
    assert_eq!(runner.refresh_count(), 1);
    assert!(runner.last_error().is_none());
}

#[test]
fn reload_error_populates_banner_and_renders_footer() {
    let mut runner = WatchRunner::new_static(fake_cross_tool());
    runner.set_last_error_for_test("could not parse events.jsonl");
    assert_eq!(runner.last_error(), Some("could not parse events.jsonl"));

    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    runner.draw_frame(&mut term).unwrap();
    let buf = term.backend().buffer().clone();
    let rendered = format!("{buf:?}");
    assert!(
        rendered.contains("reload error"),
        "footer banner missing 'reload error'; got: {rendered}"
    );
}

#[test]
fn help_overlay_toggles() {
    let mut runner = WatchRunner::new_static(fake_single());
    assert!(!runner.help_overlay_for_test());
    runner.toggle_help_for_test();
    assert!(runner.help_overlay_for_test());
    runner.toggle_help_for_test();
    assert!(!runner.help_overlay_for_test());
}

#[test]
fn cross_mode_draw_frame_renders_by_tool_header() {
    let runner = WatchRunner::new_static(fake_cross_tool());
    let mut term = Terminal::new(TestBackend::new(120, 10)).unwrap();
    runner.draw_frame(&mut term).unwrap();
    let buf = term.backend().buffer().clone();
    let rendered = format!("{buf:?}");
    assert!(
        rendered.contains("by tool"),
        "expected 'by tool' in cross-mode header; got: {rendered}"
    );
}

#[test]
fn reload_success_after_failure_clears_banner() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::mpsc::channel;

    let calls = Rc::new(RefCell::new(0u8));
    let calls_for_closure = Rc::clone(&calls);
    let reload: Box<dyn FnMut() -> Result<WatchData, agentprof_tui::watch::ReloadError>> =
        Box::new(move || {
            let mut c = calls_for_closure.borrow_mut();
            *c += 1;
            if *c == 1 {
                Err(agentprof_tui::watch::ReloadError::Pipeline(
                    "synthetic first-call failure".to_string(),
                ))
            } else {
                Ok(fake_single())
            }
        });

    let (tx, rx) = channel();
    let mut runner = WatchRunner::with_watcher(fake_cross_tool(), rx, reload);
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();

    tx.send(agentprof_tui::watch::RefreshKind::DataChanged)
        .unwrap();
    runner.run_one_iteration_for_test(&mut term).unwrap();
    assert!(
        runner.last_error().is_some(),
        "expected last_error after first failed reload"
    );
    assert_eq!(runner.refresh_count(), 0);

    tx.send(agentprof_tui::watch::RefreshKind::DataChanged)
        .unwrap();
    runner.run_one_iteration_for_test(&mut term).unwrap();
    assert!(
        runner.last_error().is_none(),
        "expected last_error cleared after successful reload; got: {:?}",
        runner.last_error()
    );
    assert_eq!(runner.refresh_count(), 1);
    assert!(matches!(runner.data(), WatchData::Single { .. }));
}
