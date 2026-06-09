//! `agentprof db export <SESSION_ID>` — dump one session as JSON / JSONL.
//!
//! Reads the stored `analysis_report_json` blob for the requested
//! session id from the `SQLite` cache (no adapter re-parse) and writes
//! it to `--output` (default stdout) in one of two formats:
//!
//! - `json` — the full report pretty-printed as a single JSON object.
//! - `jsonl` — one line per top-level field of the report, useful for
//!   `jq`-style line-oriented pipelines.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use serde_json::{json, Map, Value};

use agentprof_cli::config::resolve_storage_config;
use agentprof_storage::admin::export_session_json;
use agentprof_storage::config::PartialStorageConfig;
use agentprof_storage::Db;

use crate::cmd::exit::ExitKind;

/// Arguments for `agentprof db export`.
#[derive(Args, Debug, Clone)]
#[non_exhaustive]
pub struct ExportArgs {
    /// Session id to dump.
    pub session_id: String,

    /// Output format.
    #[arg(long, value_enum, default_value_t = ExportFormat::Json)]
    pub format: ExportFormat,

    /// Output file. If absent, writes to stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// Output format for [`ExportArgs::format`].
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
#[non_exhaustive]
pub enum ExportFormat {
    /// Single pretty-printed JSON object.
    Json,
    /// Newline-delimited JSON: one `{"<key>": <value>}` object per
    /// top-level field of the report.
    Jsonl,
}

/// Run `agentprof db export`.
///
/// # Errors
///
/// - [`ExitKind::UserError`] for bad storage config / unknown session id.
/// - [`ExitKind::DataError`] if the stored JSON blob fails to re-parse.
/// - [`ExitKind::OutputError`] if `--output` cannot be written.
#[allow(clippy::needless_pass_by_value)]
pub fn run(args: ExportArgs, storage_path: Option<PathBuf>) -> Result<()> {
    let cfg = resolve_storage_config(PartialStorageConfig::default(), storage_path)
        .map_err(|e| ExitKind::UserError.into_anyhow(format!("storage config: {e}")))?;
    let db = Db::open_and_migrate(&cfg.path).map_err(|e| {
        ExitKind::DataError.into_anyhow(format!("open {}: {e}", cfg.path.display()))
    })?;
    let raw = export_session_json(&db, &args.session_id).map_err(|e| {
        ExitKind::UserError.into_anyhow(format!("session {} not found in DB: {e}", args.session_id))
    })?;

    let body = match args.format {
        ExportFormat::Json => {
            let v: Value = serde_json::from_str(&raw).map_err(|e| {
                ExitKind::DataError.into_anyhow(format!("re-parse stored report: {e}"))
            })?;
            let mut out = serde_json::to_string_pretty(&v)
                .map_err(|e| ExitKind::DataError.into_anyhow(format!("render json: {e}")))?;
            out.push('\n');
            out
        }
        ExportFormat::Jsonl => {
            let v: Value = serde_json::from_str(&raw).map_err(|e| {
                ExitKind::DataError.into_anyhow(format!("re-parse stored report: {e}"))
            })?;
            jsonl_from_value(&v)
        }
    };

    if let Some(path) = args.output.as_deref() {
        fs::write(path, &body).map_err(|e| {
            ExitKind::OutputError.into_anyhow(format!("write {}: {e}", path.display()))
        })?;
    } else {
        print!("{body}");
    }
    Ok(())
}

/// Render an `AnalysisReport` JSON blob as newline-delimited JSON:
/// one `{"<key>": <value>}` object per top-level field. Non-object
/// roots are emitted as a single `{"value": ...}` line.
fn jsonl_from_value(v: &Value) -> String {
    let mut out = String::new();
    match v {
        Value::Object(map) => {
            for (k, vv) in map {
                let mut wrap = Map::new();
                wrap.insert(k.clone(), vv.clone());
                let line = Value::Object(wrap).to_string();
                out.push_str(&line);
                out.push('\n');
            }
        }
        other => {
            out.push_str(&json!({ "value": other }).to_string());
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonl_emits_one_line_per_key() {
        let v: Value = serde_json::from_str(r#"{"a": 1, "b": [2,3], "c": null}"#).unwrap();
        let s = jsonl_from_value(&v);
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 3);
        for line in lines {
            let parsed: Value = serde_json::from_str(line).unwrap();
            assert_eq!(parsed.as_object().unwrap().len(), 1);
        }
    }

    #[test]
    fn jsonl_non_object_falls_back_to_value_wrapper() {
        let v: Value = serde_json::from_str("42").unwrap();
        let s = jsonl_from_value(&v);
        let parsed: Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(parsed["value"], 42);
    }
}
