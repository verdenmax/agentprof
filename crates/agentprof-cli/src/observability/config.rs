//! Resolved tracing configuration: precedence is flag > env > default.
//!
//! Construct via [`LogConfig::resolve_from_env_and_flags`] from parsed
//! CLI args; pass the resulting config into [`super::init_tracing`].
//! Resolution NEVER fails and never returns a `Result` — invalid inputs
//! soft-fall to the default (warn-level stderr, no full paths). See
//! ADR-0010 D-3 and D-13.

use std::path::PathBuf;

/// Where tracing events are written.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LogWriter {
    /// Default writer. Used unless `--log-file <PATH>` or
    /// `AGENTPROF_LOG_FILE=<PATH>` is set, OR the special value `-` is
    /// passed (which explicitly forces stderr even in TUI mode).
    Stderr,
    /// Write to `path`. Parent directory is created on `init_tracing`
    /// if missing; soft-falls to `Stderr` if creation/open fails.
    File(PathBuf),
}

/// Frozen tracing configuration for the lifetime of one CLI invocation.
///
/// Construct via [`LogConfig::resolve_from_env_and_flags`] from the parsed
/// CLI args.
///
/// # Examples
///
/// ```text
/// // bin-crate: not name-resolvable from doctests; see the
/// // `config::tests` module for executable coverage.
/// let cfg = LogConfig::resolve_from_env_and_flags(Some("debug".into()), None);
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct LogConfig {
    /// Raw env-filter string (e.g. `"warn"`, `"debug"`,
    /// `"warn,agentprof_core=trace"`). Validated lazily by
    /// `init_tracing`; invalid values soft-fall to `"warn"`.
    pub level_filter: String,

    /// Destination writer (stderr or file).
    pub writer: LogWriter,

    /// If `true`, emission helpers should NOT redact session paths
    /// (i.e. `hash_path()` is bypassed). Set via env
    /// `AGENTPROF_LOG_FULL_PATHS=1`.
    pub full_paths: bool,

    /// If `true`, `enter_tui_log_guard` should NOT auto-switch to a
    /// file writer (user explicitly passed `--log-file -`).
    pub force_stderr: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level_filter: "warn".to_string(),
            writer: LogWriter::Stderr,
            full_paths: false,
            force_stderr: false,
        }
    }
}

impl LogConfig {
    /// Merge inputs into a `LogConfig` with `flag > env > default` precedence.
    ///
    /// Special cases:
    /// - `flag_log_file == Some("-")` → forces `LogWriter::Stderr` AND
    ///   sets `force_stderr = true` (so the TUI guard knows not to
    ///   redirect).
    /// - `env_full_paths != Some("1")` → `full_paths = false`.
    ///
    /// Resolution never panics and never returns an error.
    ///
    /// # Examples
    ///
    /// ```text
    /// // bin-crate: not name-resolvable from doctests; see unit
    /// // tests for executable coverage.
    /// LogConfig::from_inputs(Some("debug".into()), None, None, None, None)
    /// ```
    #[must_use]
    pub fn from_inputs(
        flag_log_level: Option<String>,
        flag_log_file: Option<PathBuf>,
        env_log_level: Option<String>,
        env_log_file: Option<PathBuf>,
        env_full_paths: Option<String>,
    ) -> Self {
        // Level precedence: flag > env > default ("warn").
        let level_filter = flag_log_level
            .or(env_log_level)
            .unwrap_or_else(|| "warn".to_string());

        // File precedence: flag > env > None.
        let file = flag_log_file.or(env_log_file);
        let (writer, force_stderr) = match file {
            Some(p) if p.as_os_str() == "-" => (LogWriter::Stderr, true),
            Some(p) => (LogWriter::File(p), false),
            None => (LogWriter::Stderr, false),
        };

        let full_paths = env_full_paths.is_some_and(|s| s == "1");

        Self {
            level_filter,
            writer,
            full_paths,
            force_stderr,
        }
    }

    /// Resolve from CLI args + the standard env vars.
    ///
    /// Level precedence (lower wins):
    /// 1. `flag_log_level` (CLI `--log-level`)
    /// 2. `AGENTPROF_LOG_LEVEL` env
    /// 3. `AGENTPROF_LOG` env (backwards-compatible alias)
    /// 4. Default `"warn"`
    ///
    /// File precedence (lower wins):
    /// 1. `flag_log_file` (CLI `--log-file`)
    /// 2. `AGENTPROF_LOG_FILE` env
    /// 3. None (= stderr by default; TUI may auto-redirect via
    ///    [`super::enter_tui_log_guard`])
    ///
    /// Plus: `AGENTPROF_LOG_FULL_PATHS=1` → opt out of path hashing.
    ///
    /// # Examples
    ///
    /// ```text
    /// // bin-crate: not name-resolvable from doctests; see unit
    /// // tests (config::tests::*) for executable coverage.
    /// let cfg = LogConfig::resolve_from_env_and_flags(Some("debug".into()), None);
    /// ```
    #[must_use]
    pub fn resolve_from_env_and_flags(
        flag_log_level: Option<String>,
        flag_log_file: Option<PathBuf>,
    ) -> Self {
        Self::resolve_from_env_and_flags_with_lookup(flag_log_level, flag_log_file, |var| {
            std::env::var(var).ok()
        })
    }

    /// Internal env-lookup-injected variant used by unit tests to avoid
    /// racing `std::env::set_var` across parallel test threads
    /// (rubber-duck Important #4).
    pub(crate) fn resolve_from_env_and_flags_with_lookup<F>(
        flag_log_level: Option<String>,
        flag_log_file: Option<PathBuf>,
        lookup: F,
    ) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let env_log_level = lookup("AGENTPROF_LOG_LEVEL").or_else(|| lookup("AGENTPROF_LOG"));
        let env_log_file = lookup("AGENTPROF_LOG_FILE").map(PathBuf::from);
        let env_full_paths = lookup("AGENTPROF_LOG_FULL_PATHS");
        Self::from_inputs(
            flag_log_level,
            flag_log_file,
            env_log_level,
            env_log_file,
            env_full_paths,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_warn_stderr_no_full_paths() {
        let cfg = LogConfig::from_inputs(None, None, None, None, None);
        assert_eq!(cfg.level_filter, "warn");
        assert!(matches!(cfg.writer, LogWriter::Stderr));
        assert!(!cfg.full_paths);
        assert!(!cfg.force_stderr);
    }

    #[test]
    fn flag_log_level_wins_over_env() {
        let cfg =
            LogConfig::from_inputs(Some("debug".into()), None, Some("warn".into()), None, None);
        assert_eq!(cfg.level_filter, "debug");
    }

    #[test]
    fn env_log_level_used_when_no_flag() {
        let cfg = LogConfig::from_inputs(None, None, Some("info".into()), None, None);
        assert_eq!(cfg.level_filter, "info");
    }

    #[test]
    fn flag_log_file_wins_over_env() {
        let cfg = LogConfig::from_inputs(
            None,
            Some(PathBuf::from("/tmp/flag.log")),
            None,
            Some(PathBuf::from("/tmp/env.log")),
            None,
        );
        match &cfg.writer {
            LogWriter::File(p) => assert_eq!(p, &PathBuf::from("/tmp/flag.log")),
            other => panic!("expected File, got {other:?}"),
        }
        assert!(!cfg.force_stderr);
    }

    #[test]
    fn env_log_file_used_when_no_flag() {
        let cfg =
            LogConfig::from_inputs(None, None, None, Some(PathBuf::from("/tmp/env.log")), None);
        match &cfg.writer {
            LogWriter::File(p) => assert_eq!(p, &PathBuf::from("/tmp/env.log")),
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[test]
    fn dash_log_file_forces_stderr() {
        let cfg = LogConfig::from_inputs(None, Some(PathBuf::from("-")), None, None, None);
        assert!(matches!(cfg.writer, LogWriter::Stderr));
        assert!(cfg.force_stderr);
    }

    #[test]
    fn full_paths_env_var_enables_flag() {
        let cfg = LogConfig::from_inputs(None, None, None, None, Some("1".into()));
        assert!(cfg.full_paths);

        let cfg_zero = LogConfig::from_inputs(None, None, None, None, Some("0".into()));
        assert!(!cfg_zero.full_paths);

        let cfg_empty = LogConfig::from_inputs(None, None, None, None, Some(String::new()));
        assert!(!cfg_empty.full_paths);
    }

    #[test]
    fn invalid_level_does_not_panic() {
        let cfg = LogConfig::from_inputs(Some("not-a-real-level".into()), None, None, None, None);
        // Resolution doesn't validate — only init_tracing does, soft-falling on
        // invalid input. Verify just that resolution accepted any string.
        assert_eq!(cfg.level_filter, "not-a-real-level");
    }

    #[test]
    fn agentprof_log_back_compat_used_when_no_other_inputs() {
        // Rubber-duck Important #4 regression test. Uses the closure-form
        // entry point to avoid env-var races across parallel test threads.
        let env = |var: &str| -> Option<String> {
            match var {
                "AGENTPROF_LOG" => Some("info".into()),
                _ => None,
            }
        };
        let cfg = LogConfig::resolve_from_env_and_flags_with_lookup(None, None, env);
        assert_eq!(
            cfg.level_filter, "info",
            "AGENTPROF_LOG must serve as a backwards-compatible fallback \
             when AGENTPROF_LOG_LEVEL is unset and no --log-level is given"
        );
    }

    #[test]
    fn agentprof_log_level_wins_over_agentprof_log_back_compat() {
        let env = |var: &str| -> Option<String> {
            match var {
                "AGENTPROF_LOG_LEVEL" => Some("debug".into()),
                "AGENTPROF_LOG" => Some("info".into()),
                _ => None,
            }
        };
        let cfg = LogConfig::resolve_from_env_and_flags_with_lookup(None, None, env);
        assert_eq!(cfg.level_filter, "debug");
    }
}
