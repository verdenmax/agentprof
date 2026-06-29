//! Integration tests for `AnalysisReport::redact`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use agentprof_core::adapter::AgentKind;
use agentprof_core::analyzer::redact::PrivacyLevel;
use agentprof_core::analyzer::{AnalysisReport, ModelUsage, ToolRankRow, TurnSummaryRow};
use agentprof_core::episode::{AbortInfo, TurnStatus};
use agentprof_core::model::{SessionMeta, ToolSource};
use chrono::{Duration, TimeZone, Utc};
use std::collections::BTreeMap;

fn sample() -> AnalysisReport {
    let mut meta = SessionMeta::new(
        "11111111-1111-1111-1111-111111111111".into(),
        AgentKind::Copilot,
        Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap(),
        false,
    );
    meta.cwd = Some("/home/alice/projects/secret".into());
    meta.branch = Some("feat/secret".into());
    meta.repository = Some("alice/secret-repo".into());
    meta.agent_version = Some("1.0.54".into());
    let mut r = AnalysisReport::new(meta);
    let row = TurnSummaryRow::new(
        "22222222-2222-2222-2222-222222222222".into(),
        Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap(),
        None,
        TurnStatus::Open,
        Some("claude-opus-4.7-1m-internal".into()),
        None,
        None,
        0,
        0,
        0,
    );
    r.turn_summary.push(row);
    r
}

#[test]
fn redact_strips_high_tier_and_keeps_map_empty() {
    let (out, map) = sample().redact(PrivacyLevel::Redact);
    assert_eq!(out.meta.cwd.as_deref(), Some("<redacted>"));
    assert_eq!(out.meta.branch.as_deref(), Some("<redacted>"));
    assert_eq!(out.meta.repository.as_deref(), Some("<redacted>"));
    assert_eq!(out.meta.id, "<uuid-0>");
    assert_eq!(out.turn_summary[0].turn_id, "<uuid-1>");
    assert_eq!(out.turn_summary[0].model.as_deref(), Some("claude-opus"));
    assert_eq!(out.meta.agent_version.as_deref(), Some("1.0.54")); // kept at redact
    assert!(map.is_empty(), "redact level → empty map");
}

#[test]
fn anonymize_strips_version_and_fills_map() {
    let (out, map) = sample().redact(PrivacyLevel::Anonymize);
    assert_eq!(out.meta.agent_version.as_deref(), Some("<redacted>"));
    assert_eq!(out.meta.started_at, chrono::DateTime::<Utc>::UNIX_EPOCH);
    assert_eq!(
        map.uuids.get("<uuid-0>").map(String::as_str),
        Some("11111111-1111-1111-1111-111111111111")
    );
    assert_eq!(
        map.models.get("claude-opus").map(String::as_str),
        Some("claude-opus-4.7-1m-internal")
    );
}

#[test]
fn anonymize_zeros_turn_started_at() {
    let (out, _) = sample().redact(PrivacyLevel::Anonymize);
    for row in &out.turn_summary {
        assert_eq!(row.started_at, chrono::DateTime::<Utc>::UNIX_EPOCH);
    }
}

#[test]
fn redact_keeps_turn_started_at() {
    let before = sample();
    let (out, _) = before.redact(PrivacyLevel::Redact);
    // Redact (not anonymize) must NOT zero turn timestamps (consistent w/ meta.started_at)
    assert_eq!(
        out.turn_summary[0].started_at,
        before.turn_summary[0].started_at
    );
}

// C1: TurnStatus::Aborted embeds an `at` wall-clock instant (🟡 MEDIUM). It
// survives into json + analyze-html, so anonymize must zero it to UNIX_EPOCH
// alongside `started_at`. Redact (timestamps preserved) must leave it intact.
fn aborted_sample() -> (AnalysisReport, chrono::DateTime<Utc>) {
    let abort_at = Utc.with_ymd_and_hms(2026, 5, 26, 16, 0, 5).unwrap();
    let mut r = sample();
    r.turn_summary.push(TurnSummaryRow::new(
        "66666666-6666-6666-6666-666666666666".into(),
        Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap(),
        None,
        TurnStatus::Aborted(AbortInfo::new("user_cancel".into(), abort_at)),
        None,
        None,
        None,
        0,
        0,
        0,
    ));
    (r, abort_at)
}

#[test]
fn anonymize_zeros_abort_timestamp() {
    let (r, _) = aborted_sample();
    let (out, _) = r.redact(PrivacyLevel::Anonymize);
    match &out.turn_summary[1].status {
        TurnStatus::Aborted(info) => {
            assert_eq!(
                info.at,
                chrono::DateTime::<Utc>::UNIX_EPOCH,
                "abort `at` must be zeroed at anonymize"
            );
            assert_eq!(info.reason, "user_cancel", "reason must be preserved");
        }
        other => panic!("expected Aborted status, got {other:?}"),
    }
}

#[test]
fn redact_keeps_abort_timestamp() {
    let (r, abort_at) = aborted_sample();
    let (out, _) = r.redact(PrivacyLevel::Redact);
    match &out.turn_summary[1].status {
        TurnStatus::Aborted(info) => {
            assert_eq!(info.at, abort_at, "abort `at` must be preserved at redact");
            assert_eq!(info.reason, "user_cancel");
        }
        other => panic!("expected Aborted status, got {other:?}"),
    }
}

#[test]
fn none_is_identity() {
    let (out, map) = sample().redact(PrivacyLevel::None);
    assert_eq!(out, sample());
    assert!(map.is_empty());
}

#[test]
fn anonymize_hashes_mcp_tool_and_records_server() {
    let mut r = sample();
    r.tool_rank.push(ToolRankRow::new(
        "mcp__github__search_issues".into(),
        ToolSource::Mcp {
            server: "github".into(),
        },
        1,
        1,
        0,
        0,
        0,
        Duration::zero(),
        Duration::zero(),
        Duration::zero(),
        Duration::zero(),
    ));
    r.loaded_mcp_tools
        .insert("mcp__github__search_issues".into());

    let (out, map) = r.redact(PrivacyLevel::Anonymize);

    let name = &out.tool_rank[0].name;
    assert!(name.starts_with("mcp__"), "got {name}");
    assert!(name.ends_with("__search_issues"), "got {name}");
    assert_ne!(name, "mcp__github__search_issues"); // server segment changed
    assert!(
        map.mcp_servers.values().any(|v| v == "github"),
        "mcp_servers should map a hash back to github: {:?}",
        map.mcp_servers
    );
}

// I-1: the parallel `source.server` must be scrubbed too, else the raw
// MCP server name leaks in the serialized report despite the hashed name.
#[test]
fn anonymize_scrubs_raw_server_from_tool_source() {
    let mut r = sample();
    r.tool_rank.push(ToolRankRow::new(
        "mcp__github__search_issues".into(),
        ToolSource::Mcp {
            server: "github".into(),
        },
        1,
        1,
        0,
        0,
        0,
        Duration::zero(),
        Duration::zero(),
        Duration::zero(),
        Duration::zero(),
    ));

    let (out, _map) = r.redact(PrivacyLevel::Anonymize);

    // source.server is hashed, not the raw "github".
    match &out.tool_rank[0].source {
        ToolSource::Mcp { server } => {
            assert_ne!(server, "github", "source.server must be hashed");
            // same hash already embedded in the name segment.
            assert!(
                out.tool_rank[0].name.contains(server),
                "name {} should embed hashed server {server}",
                out.tool_rank[0].name
            );
        }
        other => panic!("expected Mcp source, got {other:?}"),
    }

    // The serialized report must NOT leak the raw server name anywhere.
    let json = serde_json::to_string(&out).unwrap();
    assert!(
        !json.contains("github"),
        "serialized anonymized report still leaks raw server: {json}"
    );
}

#[test]
fn cross_site_uuid_stability() {
    let shared = "33333333-3333-3333-3333-333333333333";
    let mut meta = SessionMeta::new(
        shared.into(),
        AgentKind::Copilot,
        Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap(),
        false,
    );
    meta.cwd = Some("/home/alice/x".into());
    let mut r = AnalysisReport::new(meta);
    r.turn_summary.push(TurnSummaryRow::new(
        shared.into(),
        Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap(),
        None,
        TurnStatus::Open,
        None,
        None,
        None,
        0,
        0,
        0,
    ));

    let (out, _map) = r.redact(PrivacyLevel::Redact);
    assert_eq!(
        out.meta.id, out.turn_summary[0].turn_id,
        "same source UUID must map to the same placeholder"
    );
}

#[test]
fn model_metrics_merge_on_family_collision() {
    let mut r = sample();
    let mut a = ModelUsage::new();
    a.input_tokens = 100;
    a.output_tokens = 10;
    a.cache_read_tokens = 5;
    a.cache_write_tokens = 1;
    let mut b = ModelUsage::new();
    b.input_tokens = 200;
    b.output_tokens = 20;
    b.cache_read_tokens = 7;
    b.cache_write_tokens = 2;
    let mut mm = BTreeMap::new();
    mm.insert("gpt-5".to_string(), a);
    mm.insert("gpt-5-mini".to_string(), b);
    r.model_metrics = Some(mm);

    let (out, _map) = r.redact(PrivacyLevel::Redact);
    let merged = out.model_metrics.expect("model_metrics present");
    assert_eq!(merged.len(), 1, "both collapse to one gpt-5 family");
    let g = merged.get("gpt-5").expect("gpt-5 family key");
    assert_eq!(g.input_tokens, 300);
    assert_eq!(g.output_tokens, 30);
    assert_eq!(g.cache_read_tokens, 12);
    assert_eq!(g.cache_write_tokens, 3);
}

#[test]
fn redaction_clears_diagnostics() {
    use agentprof_core::adapter::EventKind;
    use agentprof_core::episode::DeriveWarning;
    use agentprof_core::error::ParseWarning;

    let mut r = sample();
    r.warnings.push(DeriveWarning::PayloadNameMissing {
        kind: EventKind::ToolExecStart,
        event_id: "44444444-4444-4444-4444-444444444444".into(),
    });
    r.parse_warnings.push(ParseWarning::UnclosedTurn {
        turn_id: "55555555-5555-5555-5555-555555555555".into(),
    });

    let (redacted, _m) = r.clone().redact(PrivacyLevel::Redact);
    assert!(redacted.warnings.is_empty(), "warnings must be cleared");
    assert!(
        redacted.parse_warnings.is_empty(),
        "parse_warnings must be cleared"
    );

    let (anon, _m) = r.redact(PrivacyLevel::Anonymize);
    assert!(anon.warnings.is_empty());
    assert!(anon.parse_warnings.is_empty());
}

// --- L-1 T3: AggregateReport::redact ---------------------------------------

use agentprof_core::analyzer::aggregate::bucket::{
    DayBucket, McpServerBucket, ModelBucket, ToolBucket,
};
use agentprof_core::analyzer::aggregate::{AggregateKey, AggregateReport};
use chrono::NaiveDate;

const fn report<B>(by: AggregateKey, buckets: Vec<B>) -> AggregateReport<B> {
    AggregateReport::new(by, None, 0, 0, Duration::zero(), buckets)
}
fn model_bucket(model: &str) -> ModelBucket {
    ModelBucket::new(model.into(), 0, 0, 0, Duration::zero())
}

fn mcp_bucket(server: &str) -> McpServerBucket {
    McpServerBucket::new(server.into(), 0, 0, 0, Duration::zero(), 0)
}

fn tool_bucket(name: &str, source: ToolSource) -> ToolBucket {
    ToolBucket::new(
        name.into(),
        source,
        0,
        0,
        0,
        Duration::zero(),
        Duration::zero(),
        Duration::zero(),
        0,
    )
}

fn day_bucket(date: &str) -> DayBucket {
    DayBucket::new(
        NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
        0,
        Duration::zero(),
        Duration::zero(),
        0,
        0.0,
        false,
    )
}

#[test]
fn aggregate_model_bucket_redacts_to_family() {
    let report: AggregateReport<ModelBucket> = report(
        AggregateKey::Model,
        vec![model_bucket("claude-opus-4.7-1m-internal")],
    );
    let (out, _map) = report.redact(PrivacyLevel::Redact);
    assert_eq!(out.buckets[0].model, "claude-opus");
}

#[test]
fn aggregate_mcp_server_hashed_only_at_anonymize() {
    let report: AggregateReport<McpServerBucket> =
        report(AggregateKey::McpServer, vec![mcp_bucket("github")]);
    let (redacted, m1) = report.redact(PrivacyLevel::Redact);
    assert_eq!(redacted.buckets[0].server, "github"); // redact: unchanged
    assert!(m1.is_empty());
    let (anon, m2) = report.redact(PrivacyLevel::Anonymize);
    assert_ne!(anon.buckets[0].server, "github");
    assert_eq!(
        m2.mcp_servers.get(&anon.buckets[0].server),
        Some(&"github".to_string())
    );
}

// M-1: ToolBucket is the only aggregate bucket carrying a `source` field,
// so its `redact_key` must scrub the raw MCP server from both `name` and
// `source.server` at Anonymize (and leave everything alone at Redact).
#[test]
fn aggregate_tool_bucket_scrubs_source_at_anonymize() {
    let report: AggregateReport<ToolBucket> = report(
        AggregateKey::Tool,
        vec![tool_bucket(
            "mcp__github__search_issues",
            ToolSource::Mcp {
                server: "github".into(),
            },
        )],
    );

    // Redact: untouched (MCP names are 🟡 MEDIUM).
    let (redacted, m1) = report.redact(PrivacyLevel::Redact);
    assert_eq!(redacted.buckets[0].name, "mcp__github__search_issues");
    assert_eq!(
        redacted.buckets[0].source,
        ToolSource::Mcp {
            server: "github".into()
        }
    );
    assert!(m1.is_empty());

    // Anonymize: name hashed, source.server hashed, map inverts the hash.
    let (anon, m2) = report.redact(PrivacyLevel::Anonymize);
    let name = &anon.buckets[0].name;
    assert!(name.starts_with("mcp__"), "got {name}");
    assert!(name.ends_with("__search_issues"), "got {name}");
    assert_ne!(name, "mcp__github__search_issues"); // server segment changed

    let server = match &anon.buckets[0].source {
        ToolSource::Mcp { server } => server.clone(),
        other => panic!("expected Mcp source, got {other:?}"),
    };
    assert_ne!(server, "github", "source.server must be hashed");
    assert!(
        name.contains(&server),
        "name {name} should embed hashed server {server}"
    );

    let json = serde_json::to_string(&anon).unwrap();
    assert!(
        !json.contains("github"),
        "serialized anonymized aggregate still leaks raw server: {json}"
    );
    assert_eq!(m2.mcp_servers.get(&server), Some(&"github".to_string()));
}

#[test]
fn aggregate_day_bucket_never_redacted() {
    let report: AggregateReport<DayBucket> =
        report(AggregateKey::Day, vec![day_bucket("2026-05-26")]);
    let (out, _) = report.redact(PrivacyLevel::Anonymize);
    assert_eq!(
        out.buckets[0].date,
        NaiveDate::parse_from_str("2026-05-26", "%Y-%m-%d").unwrap()
    );
}

// I1: two distinct ids sharing a family must merge into ONE bucket after
// redact (model → family), summing ALL 7 non-key fields — else they render as
// two identical-keyed rows with split counts.
#[test]
fn aggregate_same_family_models_merge_summing_all_fields() {
    let a = ModelBucket::new("claude-sonnet-4.5".into(), 1, 2, 3, Duration::seconds(4))
        .with_cache_metrics(5, 6, 7);
    let b = ModelBucket::new(
        "claude-sonnet-4.6".into(),
        10,
        20,
        30,
        Duration::seconds(40),
    )
    .with_cache_metrics(50, 60, 70);
    let report: AggregateReport<ModelBucket> = report(AggregateKey::Model, vec![a, b]);

    let (out, _map) = report.redact(PrivacyLevel::Redact);
    assert_eq!(out.buckets.len(), 1, "same-family buckets must consolidate");
    let m = &out.buckets[0];
    assert_eq!(m.model, "claude-sonnet");
    assert_eq!(m.session_count, 11);
    assert_eq!(m.turn_count, 22);
    assert_eq!(m.total_output_tokens, 33);
    assert_eq!(m.total_input_tokens, 55);
    assert_eq!(m.total_cache_read, 66);
    assert_eq!(m.total_cache_creation, 77);
    assert_eq!(m.total_duration, Duration::seconds(44));
}

#[test]
fn shared_ctx_gives_stable_uuids_across_reports() {
    use agentprof_core::analyzer::redact::RedactionContext;
    let mut ctx = RedactionContext::default();
    let r1 = ctx.redact_uuid("sess-1");
    let r2 = ctx.redact_uuid("sess-1");
    let r3 = ctx.redact_uuid("turn-9");
    assert_eq!(r1, "<uuid-0>");
    assert_eq!(r2, "<uuid-0>");
    assert_eq!(r3, "<uuid-1>");
    assert!(!ctx.into_map().uuids.is_empty());
}

#[test]
fn episodes_redact_with_anonymize_rekeys_and_syncs_callrefs() {
    use agentprof_core::analyzer::redact::RedactionContext;
    use agentprof_core::episode::{CallRef, DeriveWarning, Episodes, ToolEpisode, Turn};
    use agentprof_core::model::ToolSource;

    use agentprof_core::episode::ToolCall;

    let t0 = Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap();
    let mut e = Episodes::new();
    let mut turn = Turn::new("t-1".into(), t0);
    turn.model = Some("claude-sonnet-4.6".into());
    turn.tool_calls
        .push(CallRef::new("mcp__github__search".into(), 0));
    e.turns.push(turn);
    let mut ep = ToolEpisode::new(
        "mcp__github__search".into(),
        ToolSource::Mcp {
            server: "github".into(),
        },
    );
    let mut call = ToolCall::new(agentprof_core::episode::Span::new(t0, t0));
    call.turn_id = Some("t-1".into());
    ep.calls.push(call);
    e.tools.insert("mcp__github__search".into(), ep);
    e.warnings.push(DeriveWarning::AbortWithoutOpenElement {
        reason: "user_cancel".into(),
        at: t0,
    });

    let mut ctx = RedactionContext::default();
    let out = e.redact_with(PrivacyLevel::Anonymize, &mut ctx);

    assert_eq!(out.turns[0].id, "<uuid-0>");
    assert_eq!(out.turns[0].model.as_deref(), Some("claude-sonnet"));
    assert_eq!(out.turns[0].started_at, chrono::DateTime::<Utc>::UNIX_EPOCH);
    assert!(out.warnings.is_empty(), "warnings cleared");

    let key = out.tools.keys().next().unwrap();
    assert!(key.starts_with("mcp__") && key.ends_with("__search"));
    assert_ne!(key, "mcp__github__search", "server segment hashed");
    assert_eq!(
        &out.turns[0].tool_calls[0].name, key,
        "CallRef.name must equal rekeyed map key"
    );

    // Fix 1: ToolEpisode.source's Mcp{server} must be hashed, not raw.
    match &out.tools[key].source {
        ToolSource::Mcp { server } => {
            assert_ne!(server, "github", "tool.source.server must be hashed");
        }
        other => panic!("expected Mcp source, got {other:?}"),
    }
    let json = serde_json::to_string(&out).unwrap();
    assert!(
        !json.contains("\"github\""),
        "serialized episodes still leak raw server: {json}"
    );

    // Fix 2: call-level turn_id rewritten to match the redacted turn id.
    assert_eq!(
        out.tools[key].calls[0].turn_id.as_deref(),
        Some("<uuid-0>"),
        "ToolCall.turn_id must match redacted turn id"
    );
    assert_eq!(out.tools[key].calls[0].turn_id.as_deref(), Some("<uuid-0>"));
}

// Fix 2: turn_id rewrite happens at Redact too (no anonymize required).
#[test]
fn episodes_redact_rewrites_call_turn_id() {
    use agentprof_core::analyzer::redact::RedactionContext;
    use agentprof_core::episode::{Episodes, Span, ToolCall, ToolEpisode, Turn};
    use agentprof_core::model::ToolSource;

    let t0 = Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap();
    let mut e = Episodes::new();
    e.turns.push(Turn::new("t-1".into(), t0));
    let mut ep = ToolEpisode::new("bash".into(), ToolSource::Builtin);
    let mut call = ToolCall::new(Span::new(t0, t0));
    call.turn_id = Some("t-1".into());
    ep.calls.push(call);
    e.tools.insert("bash".into(), ep);

    let mut ctx = RedactionContext::default();
    let out = e.redact_with(PrivacyLevel::Redact, &mut ctx);

    assert_eq!(out.turns[0].id, "<uuid-0>");
    assert_eq!(
        out.tools["bash"].calls[0].turn_id.as_deref(),
        Some("<uuid-0>"),
        "ToolCall.turn_id must match redacted turn even at Redact"
    );
}

// #2: absolute call timestamps leak working hours + break flamegraph offsets if
// kept while turn.started_at is zeroed. Anonymize must zero ToolCall/HookCall
// spans + SkillInvocation.at to epoch, mirroring turn.started_at. Redact keeps.
fn span_sample() -> agentprof_core::episode::Episodes {
    use agentprof_core::episode::{
        Episodes, HookCall, HookEpisode, SkillEpisode, SkillInvocation, Span, ToolCall,
        ToolEpisode, Turn,
    };
    use agentprof_core::model::ToolSource;
    let t0 = Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap();
    let t1 = Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 5).unwrap();
    let mut e = Episodes::new();
    e.turns.push(Turn::new("t-1".into(), t0));
    let mut tool = ToolEpisode::new("bash".into(), ToolSource::Builtin);
    tool.calls.push(ToolCall::new(Span::new(t0, t1)));
    e.tools.insert("bash".into(), tool);
    let mut hook = HookEpisode::new("pre".into());
    hook.calls.push(HookCall::new(Span::new(t0, t1)));
    e.hooks.insert("pre".into(), hook);
    let mut skill = SkillEpisode::new("plan".into());
    skill.invocations.push(SkillInvocation::new(t0));
    e.skills.insert("plan".into(), skill);
    e
}

#[test]
fn anonymize_zeros_call_spans_and_skill_at() {
    use agentprof_core::analyzer::redact::RedactionContext;
    let epoch = chrono::DateTime::<Utc>::UNIX_EPOCH;
    let mut ctx = RedactionContext::default();
    let out = span_sample().redact_with(PrivacyLevel::Anonymize, &mut ctx);
    let tk = out.tools.keys().next().unwrap().clone();
    assert_eq!(out.tools[&tk].calls[0].span.started_at, epoch);
    assert_eq!(out.tools[&tk].calls[0].span.ended_at, epoch);
    let hk = out.hooks.keys().next().unwrap().clone();
    assert_eq!(out.hooks[&hk].calls[0].span.started_at, epoch);
    assert_eq!(out.hooks[&hk].calls[0].span.ended_at, epoch);
    let sk = out.skills.keys().next().unwrap().clone();
    assert_eq!(out.skills[&sk].invocations[0].at, epoch);
}

#[test]
fn redact_keeps_call_spans_and_skill_at() {
    use agentprof_core::analyzer::redact::RedactionContext;
    let t0 = Utc.with_ymd_and_hms(2026, 5, 26, 2, 43, 0).unwrap();
    let mut ctx = RedactionContext::default();
    let out = span_sample().redact_with(PrivacyLevel::Redact, &mut ctx);
    assert_eq!(out.tools["bash"].calls[0].span.started_at, t0);
    assert_eq!(out.hooks["pre"].calls[0].span.started_at, t0);
    assert_eq!(out.skills["plan"].invocations[0].at, t0);
}
