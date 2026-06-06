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
        Some(Duration::days(30)),
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

#[test]
fn watch_view_state_persists_detail_view_field() {
    use agentprof_tui::watch::WatchViewState;
    let s = WatchViewState::default();
    assert!(
        s.detail_view.is_none(),
        "WatchViewState defaults to detail_view = None"
    );
}

#[test]
fn reload_drops_detail_view_when_turn_disappears() {
    use agentprof_core::episode::{
        CallRef, Episodes, Span as EpSpan, ToolCall, ToolCallStatus, ToolEpisode, Turn,
    };
    use agentprof_core::model::ToolSource;

    fn fixture_with_t1() -> (AnalysisReport, Episodes) {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        let report = AnalysisReport::new(meta);
        let mut episodes = Episodes::new();
        let start = Utc::now();
        let span = EpSpan::new(start, start + Duration::seconds(1));
        let mut tc = ToolCall::new(span);
        tc.status = ToolCallStatus::Success;
        let mut tool_ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
        tool_ep.calls.push(tc);
        episodes.tools.insert("bash".into(), tool_ep);
        let mut turn = Turn::new("T1".into(), start);
        turn.tool_calls.push(CallRef::new("bash".into(), 0));
        episodes.turns.push(turn);
        (report, episodes)
    }

    fn fixture_empty() -> (AnalysisReport, Episodes) {
        let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
        let report = AnalysisReport::new(meta);
        (report, Episodes::new())
    }

    let (r1, e1) = fixture_with_t1();
    let mut runner = WatchRunner::new_static(WatchData::Single {
        report: r1,
        episodes: e1,
        meta: SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false),
    });
    runner.view_state_mut().detail_view = Some(
        agentprof_tui::views::turn_detail::TurnDetailState::new("T1"),
    );

    let reload_call = std::sync::Arc::new(std::sync::Mutex::new(0u32));
    let rc = reload_call.clone();
    runner.set_reload(Box::new(move || {
        *rc.lock().unwrap() += 1;
        let (r, e) = fixture_empty();
        Ok(WatchData::Single {
            report: r,
            episodes: e,
            meta: SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false),
        })
    }));
    runner.do_reload_for_test();
    assert!(
        runner.view_state().detail_view.is_none(),
        "turn-disappeared reload should drop detail_view"
    );
    assert!(
        runner.last_error().unwrap_or("").contains("disappeared"),
        "expected 'disappeared' message in last_error; got: {:?}",
        runner.last_error()
    );
    assert_eq!(*reload_call.lock().unwrap(), 1, "reload was called once");
}

#[test]
fn watch_runner_dispatch_enter_opens_detail_view() {
    // End-to-end: simulate a key event going through WatchRunner's
    // dispatch round-trip. Asserts:
    // 1. detail_view is None before
    // 2. dispatching Enter on Flamegraph with a valid turn opens it
    // 3. WatchViewState.detail_view is updated (write-back path works)
    use agentprof_core::episode::{
        CallRef, Span as EpSpan, ToolCall, ToolCallStatus, ToolEpisode, Turn,
    };
    use agentprof_core::model::ToolSource;
    use agentprof_tui::app::event::Event;

    let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
    let report = AnalysisReport::new(meta.clone());

    let now = Utc::now();
    let mut episodes = Episodes::new();
    let span = EpSpan::new(now, now + Duration::seconds(1));
    let mut tc = ToolCall::new(span);
    tc.status = ToolCallStatus::Success;
    let mut tool_ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
    tool_ep.calls.push(tc);
    episodes.tools.insert("bash".into(), tool_ep);

    let mut turn = Turn::new("T1".into(), now);
    turn.ended_at = Some(now + Duration::seconds(2));
    turn.tool_calls.push(CallRef::new("bash".into(), 0));
    episodes.turns.push(turn);

    let mut runner = WatchRunner::new_static(WatchData::Single {
        report,
        episodes,
        meta,
    });
    assert!(
        runner.view_state().detail_view.is_none(),
        "detail_view starts None"
    );

    // F1.7 T10 amend: WatchViewState.view now defaults to Aggregate
    // (matches M1.6.3 behavior). Explicitly switch to Flamegraph so
    // Enter has the documented effect of opening turn-detail.
    runner.dispatch_event_for_test(Event::Key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('1'),
        crossterm::event::KeyModifiers::empty(),
    )));

    let quit = runner.dispatch_event_for_test(Event::Key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::empty(),
    )));
    assert!(!quit, "Enter must not quit");

    assert!(
        runner.view_state().detail_view.is_some(),
        "Enter on Flamegraph row should open detail_view"
    );
    assert_eq!(
        runner.view_state().detail_view.as_ref().unwrap().turn_id,
        "T1",
        "detail_view should point at the selected turn id"
    );
}

#[test]
fn watch_view_state_persists_models_selected_field() {
    use agentprof_tui::watch::WatchViewState;
    let s = WatchViewState::default();
    assert_eq!(s.models_selected, 0, "default is 0");
}

#[test]
fn watch_runner_dispatch_4_switches_to_models_view() {
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::{AnalysisReport, ModelUsage};
    use agentprof_core::episode::Episodes;
    use agentprof_core::model::SessionMeta;
    use agentprof_tui::app::event::Event;
    use agentprof_tui::views::View;
    use agentprof_tui::watch::{WatchData, WatchRunner};
    use chrono::Utc;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::collections::BTreeMap;

    let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
    let mut report = AnalysisReport::new(meta.clone());
    let mut m = BTreeMap::new();
    let mut usage = ModelUsage::new();
    usage.input_tokens = 100;
    usage.output_tokens = 50;
    m.insert("test-model".into(), usage);
    report.model_metrics = Some(m);
    let mut runner = WatchRunner::new_static(WatchData::Single {
        report,
        episodes: Episodes::default(),
        meta,
    });

    runner.dispatch_event_for_test(Event::Key(KeyEvent::new(
        KeyCode::Char('4'),
        KeyModifiers::empty(),
    )));

    runner.dispatch_event_for_test(Event::Key(KeyEvent::new(
        KeyCode::Char('j'),
        KeyModifiers::empty(),
    )));
    assert_eq!(runner.view_state().models_selected, 0);

    // Verify the view round-trip works — pressing '4' should have set
    // WatchViewState.view to View::Models (otherwise watch users
    // pressing '4' would silently see no effect — pre-T10 architectural
    // bug found in T10 self-review).
    assert_eq!(
        runner.view_state().view,
        View::Models,
        "key '4' must round-trip view to Models in watch mode"
    );
}

#[test]
fn watch_runner_dispatch_number_keys_persist_view_across_events() {
    // Regression test for pre-F1.7 architectural bug: WatchRunner
    // didn't round-trip `view` across the transient AppState, so
    // pressing 1/2/3/4 had no observable effect after the next render.
    // Fixed in F1.7 Task 10 amend.
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::AnalysisReport;
    use agentprof_core::episode::Episodes;
    use agentprof_core::model::SessionMeta;
    use agentprof_tui::app::event::Event;
    use agentprof_tui::views::View;
    use agentprof_tui::watch::{WatchData, WatchRunner};
    use chrono::Utc;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let meta = SessionMeta::new("s".into(), AgentKind::Copilot, Utc::now(), false);
    let report = AnalysisReport::new(meta.clone());
    let mut runner = WatchRunner::new_static(WatchData::Single {
        report,
        episodes: Episodes::default(),
        meta,
    });

    // Default WatchViewState.view is Aggregate (backward-compat).
    assert_eq!(runner.view_state().view, View::Aggregate);

    // Press '1' → switches to Flamegraph.
    runner.dispatch_event_for_test(Event::Key(KeyEvent::new(
        KeyCode::Char('1'),
        KeyModifiers::empty(),
    )));
    assert_eq!(runner.view_state().view, View::Flamegraph, "1 → Flamegraph");

    // Press '2' → switches to Roi.
    runner.dispatch_event_for_test(Event::Key(KeyEvent::new(
        KeyCode::Char('2'),
        KeyModifiers::empty(),
    )));
    assert_eq!(runner.view_state().view, View::Roi, "2 → Roi");

    // Press '3' → switches to Aggregate.
    runner.dispatch_event_for_test(Event::Key(KeyEvent::new(
        KeyCode::Char('3'),
        KeyModifiers::empty(),
    )));
    assert_eq!(runner.view_state().view, View::Aggregate, "3 → Aggregate");

    // '4' covered by the existing `watch_runner_dispatch_4_switches_to_models_view` test.
}

// ──────────────────────────────────────────────────────────────────────
// TUI #3 — '?' toggle gating for Cross mode (full-review)
// ──────────────────────────────────────────────────────────────────────

#[test]
fn watch_cross_mode_question_mark_does_not_toggle_help_overlay() {
    use agentprof_tui::app::event::Event;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    // TUI #3 regression: pre-fix, pressing '?' in Cross mode flipped
    // view_state.help_overlay even though render_into's Cross arm has
    // no help-overlay render path — the keystroke went into a black
    // hole, mutating state with no visible effect. Post-fix the gate
    // returns `false` from handle_watch_key so the toggle is skipped.
    let mut runner = WatchRunner::new_static(fake_cross_tool());
    assert!(!runner.help_overlay_for_test(), "default = false");

    let handled = runner.handle_watch_key_for_test(&Event::Key(KeyEvent::new(
        KeyCode::Char('?'),
        KeyModifiers::empty(),
    )));
    assert!(
        !handled,
        "Cross-mode '?' must NOT be consumed (fall through to no-op)"
    );
    assert!(
        !runner.help_overlay_for_test(),
        "Cross-mode '?' must NOT mutate help_overlay"
    );
}

#[test]
fn watch_single_mode_question_mark_still_toggles_help_overlay() {
    use agentprof_tui::app::event::Event;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    // Negative-case sibling of the test above — verify the TUI #3
    // gate is Cross-mode-only and Single-mode '?' still works.
    let mut runner = WatchRunner::new_static(fake_single());
    assert!(!runner.help_overlay_for_test(), "default = false");

    let handled = runner.handle_watch_key_for_test(&Event::Key(KeyEvent::new(
        KeyCode::Char('?'),
        KeyModifiers::empty(),
    )));
    assert!(handled, "Single-mode '?' must still be consumed");
    assert!(
        runner.help_overlay_for_test(),
        "Single-mode '?' must toggle help_overlay"
    );
}

// ──────────────────────────────────────────────────────────────────────
// F1.7.1 — full 4-view render dispatch in Single mode
// ──────────────────────────────────────────────────────────────────────

/// Test helper: render a single frame in Single mode with the given
/// `View`, return the rendered text grid as a single string for
/// substring assertions.
fn render_single_with_view(view: agentprof_tui::views::View) -> String {
    let mut runner = WatchRunner::new_static(fake_single());
    runner.view_state_mut().view = view;
    let mut term = Terminal::new(TestBackend::new(120, 24)).unwrap();
    runner.draw_frame(&mut term).unwrap();
    let buf = term.backend().buffer().clone();
    format!("{buf:?}")
}

#[test]
fn watch_single_renders_flamegraph_view_when_view_is_flamegraph() {
    use agentprof_tui::views::View;
    let rendered = render_single_with_view(View::Flamegraph);
    // FlamegraphView's bordered block title contains "Flamegraph".
    assert!(
        rendered.contains("Flamegraph"),
        "Single + view=Flamegraph must render FlamegraphView (titled `Flamegraph (1/3)`); \
         pre-F1.7.1 this fell through to aggregate. Got: {rendered}"
    );
}

#[test]
fn watch_single_renders_roi_view_when_view_is_roi() {
    use agentprof_tui::views::View;
    let rendered = render_single_with_view(View::Roi);
    // RoiView's bordered block title contains "RoiView (2/3)".
    assert!(
        rendered.contains("RoiView"),
        "Single + view=Roi must render RoiView (titled `RoiView (2/3) — Sort: ...`); \
         pre-F1.7.1 this fell through to aggregate. Got: {rendered}"
    );
}

#[test]
fn watch_single_renders_aggregate_view_when_view_is_aggregate() {
    use agentprof_tui::views::View;
    let rendered = render_single_with_view(View::Aggregate);
    // AggregateView's bordered block title contains "Aggregate".
    assert!(
        rendered.contains("Aggregate"),
        "Single + view=Aggregate must render AggregateView (titled \
         `Aggregate (3/3) — By Mode (single session)`). Got: {rendered}"
    );
}

#[test]
fn watch_single_renders_models_view_when_view_is_models() {
    use agentprof_tui::views::View;
    let rendered = render_single_with_view(View::Models);
    // ModelsView's empty-state contains "no model usage" (the fake_single
    // helper builds a report without model_metrics).
    assert!(
        rendered.contains("Models") || rendered.contains("no model usage"),
        "Single + view=Models must render Models view (was already working pre-F1.7.1; \
         keeping the test as a regression guard for the full match). Got: {rendered}"
    );
}

#[test]
fn watch_single_renders_help_overlay_when_help_open() {
    // F1.7.1 — pre-fix, pressing '?' in Single mode toggled
    // view_state.help_overlay but no render path drew the overlay.
    // Post-fix, the runner calls crate::app::draw_help_overlay when
    // help_overlay is true.
    let mut runner = WatchRunner::new_static(fake_single());
    runner.toggle_help_for_test(); // help_overlay = true
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    runner.draw_frame(&mut term).unwrap();
    let buf = term.backend().buffer().clone();
    let rendered = format!("{buf:?}");
    // The overlay block title is "Help (any key closes)" — taken from
    // crate::app::draw_help_overlay.
    assert!(
        rendered.contains("Help"),
        "help_overlay = true must render the overlay; got: {rendered}"
    );
}

#[test]
fn watch_single_view_round_trips_render_through_all_4_views() {
    // Full end-to-end regression: press 1/2/3/4 via dispatch_event_for_test
    // and verify each subsequent draw renders the expected view title.
    // This guards against the "view state updates but render dispatch
    // doesn't follow" class of bug that originally motivated F1.7.1.
    use agentprof_tui::app::event::Event;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut runner = WatchRunner::new_static(fake_single());
    let cases: &[(char, &str)] = &[
        ('1', "Flamegraph"),
        ('2', "RoiView"),
        ('3', "Aggregate"),
        ('4', "Models"),
    ];
    for (key, expected_title_fragment) in cases {
        runner.dispatch_event_for_test(Event::Key(KeyEvent::new(
            KeyCode::Char(*key),
            KeyModifiers::empty(),
        )));
        let mut term = Terminal::new(TestBackend::new(120, 24)).unwrap();
        runner.draw_frame(&mut term).unwrap();
        let buf = term.backend().buffer().clone();
        let rendered = format!("{buf:?}");
        // For view = Models, the fake_single fixture has no metrics so
        // the empty-state placeholder appears instead of a "Models"
        // title. Accept either match.
        let view_visible = rendered.contains(expected_title_fragment)
            || (*key == '4' && rendered.contains("no model usage"));
        assert!(
            view_visible,
            "after pressing '{key}', render must show {expected_title_fragment:?} \
             (or empty-state for Models); got: {rendered}"
        );
    }
}

// ──────────────────────────────────────────────────────────────────────
// F2.3 — pending banner in Single mode
// ──────────────────────────────────────────────────────────────────────

/// Build a `WatchData::Single` with one pending `ask_user` call ~60s old.
fn fake_single_with_pending_askuser() -> WatchData {
    use agentprof_core::adapter::AgentKind;
    use agentprof_core::analyzer::AnalysisReport;
    use agentprof_core::episode::tool::{ToolCall, ToolCallStatus, ToolEpisode};
    use agentprof_core::episode::turn::Span;
    use agentprof_core::model::{SessionMeta, ToolSource};

    let meta = SessionMeta::new("s-abc".into(), AgentKind::Copilot, Utc::now(), false);
    let report = AnalysisReport::new(meta.clone());

    // Started 60s ago → > 30s threshold → pending.
    let started = Utc::now() - chrono::Duration::seconds(60);
    let mut call = ToolCall::new(Span::new(started, started));
    call.status = ToolCallStatus::OpenAtEndOfSession;
    let mut ep = ToolEpisode::new("ask_user".into(), ToolSource::Builtin);
    ep.calls.push(call);
    let mut episodes = Episodes::new();
    episodes.tools.insert("ask_user".into(), ep);

    WatchData::Single {
        report,
        episodes,
        meta,
    }
}

#[test]
fn watch_runner_pending_banner_renders_when_calls_pending() {
    let runner = WatchRunner::new_static(fake_single_with_pending_askuser());
    let mut term = Terminal::new(TestBackend::new(120, 24)).unwrap();
    runner.draw_frame(&mut term).unwrap();
    let buf = term.backend().buffer().clone();
    let rendered = format!("{buf:?}");
    assert!(
        rendered.contains("ask_user"),
        "pending banner must mention ask_user; got: {rendered}"
    );
    assert!(
        rendered.contains("pending"),
        "pending banner must include the literal 'pending'; got: {rendered}"
    );
    assert!(
        rendered.contains("your input needed"),
        "user-blocking pending must include 'your input needed' hint; got: {rendered}"
    );
}

#[test]
fn watch_runner_pending_banner_suppressed_by_reload_error() {
    // Spec §3.4: error precedence over pending. Both signals active
    // → only the error renders.
    let mut runner = WatchRunner::new_static(fake_single_with_pending_askuser());
    runner.set_last_error_for_test("disk full");
    let mut term = Terminal::new(TestBackend::new(120, 24)).unwrap();
    runner.draw_frame(&mut term).unwrap();
    let buf = term.backend().buffer().clone();
    let rendered = format!("{buf:?}");
    assert!(
        rendered.contains("reload error"),
        "error banner must take precedence; got: {rendered}"
    );
    assert!(
        !rendered.contains("your input needed"),
        "pending banner must be suppressed when error fires; got: {rendered}"
    );
}

#[test]
fn watch_runner_no_pending_no_banner() {
    // Regression guard: empty episodes → no banner → no body shrink.
    let runner = WatchRunner::new_static(fake_single());
    let mut term = Terminal::new(TestBackend::new(120, 24)).unwrap();
    runner.draw_frame(&mut term).unwrap();
    let buf = term.backend().buffer().clone();
    let rendered = format!("{buf:?}");
    assert!(
        !rendered.contains("pending for"),
        "no pending → no banner; got: {rendered}"
    );
}
