//! Integration tests for `agentprof_core::export`.
//!
//! Tests that need to load Copilot fixtures live in
//! `agentprof-adapters/tests/export_on_fixtures.rs` to avoid a
//! dev-dependency cycle (adapters depend on core, not vice versa).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use agentprof_core::export::speedscope::{
    Event, EventType, Frame, Profile, ProfileType, Shared, SpeedscopeProfile,
};
use agentprof_core::export::ExportWarning;
use chrono::{TimeZone as _, Utc};

#[test]
fn export_warning_span_adjusted_displays_useful_message() {
    let w = ExportWarning::SpanAdjustedForSpeedscope {
        tool_name: "bash".to_string(),
        original_start: Utc.with_ymd_and_hms(2026, 5, 31, 13, 0, 2).unwrap(),
        adjusted_start: Utc.with_ymd_and_hms(2026, 5, 31, 13, 0, 3).unwrap(),
    };
    let msg = format!("{w}");
    assert!(msg.contains("bash"), "tool name in message: {msg}");
    assert!(msg.contains("speedscope"), "format hint in message: {msg}");
}

#[test]
fn speedscope_profile_serializes_to_expected_shape() {
    let p = SpeedscopeProfile::new(
        SpeedscopeProfile::SCHEMA_URL.to_string(),
        "agentprof test".to_string(),
        "agentprof v0.0.0".to_string(),
        Shared::new(vec![
            Frame::new("session".to_string()),
            Frame::new("bash".to_string()),
        ]),
        vec![Profile::new(
            ProfileType::Evented,
            "wall-clock".to_string(),
            "milliseconds".to_string(),
            0,
            1000,
            vec![
                Event::new(EventType::Open, 0, 0),
                Event::new(EventType::Open, 100, 1),
                Event::new(EventType::Close, 500, 1),
                Event::new(EventType::Close, 1000, 0),
            ],
        )],
    );
    let json = serde_json::to_string(&p).expect("serialize");
    assert!(json.contains("\"$schema\""), "json: {json}");
    assert!(json.contains("\"type\":\"evented\""), "json: {json}");
    assert!(json.contains("\"type\":\"O\""), "json: {json}");
    assert!(json.contains("\"type\":\"C\""), "json: {json}");
    let back: SpeedscopeProfile = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(p, back);
}
