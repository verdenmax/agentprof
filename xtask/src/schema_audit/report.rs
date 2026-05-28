//! Format a [`Classification`] as markdown for human review.

use std::fmt::Write;
use std::path::Path;

use chrono::Local;

use crate::schema_audit::classifier::{BalanceSeverity, Classification};

/// Render a markdown report for the given `Classification` and scan `root`.
#[must_use]
pub fn render(c: &Classification, root: &Path) -> String {
    let mut out = String::new();
    let today = Local::now().format("%Y-%m-%d");
    let _ = writeln!(out, "# Copilot CLI Schema Audit — {today}");
    let _ = writeln!(out);

    let _ = writeln!(out, "## Session 覆盖");
    let _ = writeln!(out, "- 扫描 root: `{}`", root.display());
    let _ = writeln!(out, "- 抽样 sessions: {}", c.session_count);
    let _ = writeln!(out, "- 总事件数: {}", c.event_count);
    if !c.agent_version_counts.is_empty() {
        let _ = writeln!(out, "- agent_version 分布:");
        for (ver, count) in &c.agent_version_counts {
            let _ = writeln!(out, "  - `{ver}`: {count} session(s)");
        }
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## Unknown 事件分类（按 `type` 字段聚合）");
    if c.unknown_by_type.is_empty() {
        let _ = writeln!(out, "✅ 无 Unknown 事件。");
    } else {
        let _ = writeln!(
            out,
            "| `type` 字段值 | 出现次数 | 候选 Rust 变体名 | 样本 session |"
        );
        let _ = writeln!(out, "|---|---|---|---|");
        let mut total = 0usize;
        for (type_str, group) in &c.unknown_by_type {
            let candidate = candidate_variant_name(type_str);
            let sample = group.example_session.as_deref().unwrap_or("?");
            let _ = writeln!(
                out,
                "| `{type_str}` | {} | `{candidate}` | `{sample}` |",
                group.count
            );
            total += group.count;
        }
        let _ = writeln!(out, "| **合计 Unknown** | **{total}** | | |");
        let _ = writeln!(out);
        let _ = writeln!(out, "### Samples");
        for (type_str, group) in &c.unknown_by_type {
            let _ = writeln!(out, "**`{type_str}`** (n={})", group.count);
            for (i, s) in group.samples.iter().enumerate() {
                let _ = writeln!(out, "```json");
                let _ = writeln!(out, "// sample {}", i + 1);
                let _ = writeln!(out, "{s}");
                let _ = writeln!(out, "```");
            }
        }
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## ParseWarning 分布");
    if c.warning_counts.is_empty() {
        let _ = writeln!(out, "✅ 无 ParseWarning。");
    } else {
        let _ = writeln!(out, "| ParseWarning 变体 | 计数 | 样本位置 |");
        let _ = writeln!(out, "|---|---|---|");
        for (variant, group) in &c.warning_counts {
            let locs = group.example_locations.join("; ");
            let _ = writeln!(out, "| `{variant}` | {} | {locs} |", group.count);
        }
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## 事件类型平衡分析");
    let _ = writeln!(out, "| EventKind 对 | start | end | 差值 | 严重程度 |");
    let _ = writeln!(out, "|---|---|---|---|---|");
    for row in &c.balance {
        let icon = match row.severity {
            BalanceSeverity::Ok => "✅ OK",
            BalanceSeverity::Minor => "⚠️ Minor",
            BalanceSeverity::Severe => "🛑 Severe",
        };
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {:+} | {icon} |",
            row.label, row.start_count, row.end_count, row.delta
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## EventKind 分布（完整）");
    let _ = writeln!(out, "| EventKind | count |");
    let _ = writeln!(out, "|---|---|");
    for (k, n) in &c.event_kind_counts {
        let _ = writeln!(out, "| `{k}` | {n} |");
    }

    out
}

/// Convert a wire-format `type` like `tool.execution_cancelled` into a
/// candidate Rust variant name like `ToolExecutionCancelled`.
fn candidate_variant_name(type_str: &str) -> String {
    let mut out = String::new();
    let mut capitalize_next = true;
    for ch in type_str.chars() {
        if ch == '.' || ch == '_' || ch == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            out.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_variant_name_examples() {
        assert_eq!(
            candidate_variant_name("tool.execution_cancelled"),
            "ToolExecutionCancelled"
        );
        assert_eq!(
            candidate_variant_name("assistant.streaming-chunk"),
            "AssistantStreamingChunk"
        );
        assert_eq!(candidate_variant_name("simple"), "Simple");
        assert_eq!(candidate_variant_name(""), "");
    }
}
