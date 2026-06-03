//! Tool source classification.

use serde::{Deserialize, Serialize};

/// Where a tool came from. Inferred from the tool name's prefix.
///
/// # Examples
///
/// ```
/// use agentprof_core::model::ToolSource;
///
/// assert_eq!(ToolSource::infer("bash"), ToolSource::Builtin);
/// assert_eq!(
///     ToolSource::infer("mcp__github__search_issues"),
///     ToolSource::Mcp { server: "github".to_string() }
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolSource {
    /// Built-in agent tool (bash, view, `str_replace_editor`, etc.).
    Builtin,
    /// Tool exposed by an MCP server.
    Mcp {
        /// MCP server name (segment between `mcp__` and next `__`).
        server: String,
    },
    /// Tool exposed by a project / plugin skill.
    Skill {
        /// Skill name.
        name: String,
    },
}

impl std::fmt::Display for ToolSource {
    /// Human-readable rendering matching the markdown / HTML report style.
    ///
    /// Variants render as:
    ///
    /// - [`ToolSource::Builtin`] → `"builtin"`
    /// - [`ToolSource::Mcp`] → `"mcp:<server>"`
    /// - [`ToolSource::Skill`] → `"skill:<name>"`
    ///
    /// This format is what end-user reports show; it intentionally avoids
    /// the `Debug` syntax (`Skill { name: "foo" }`) and is stable across
    /// future refactors of the underlying enum (snapshots use this
    /// `Display` impl rather than `Debug`).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::model::ToolSource;
    /// assert_eq!(ToolSource::Builtin.to_string(), "builtin");
    /// assert_eq!(
    ///     ToolSource::Skill { name: "synthetic".into() }.to_string(),
    ///     "skill:synthetic",
    /// );
    /// assert_eq!(
    ///     ToolSource::Mcp { server: "github".into() }.to_string(),
    ///     "mcp:github",
    /// );
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin => f.write_str("builtin"),
            Self::Mcp { server } => write!(f, "mcp:{server}"),
            Self::Skill { name } => write!(f, "skill:{name}"),
        }
    }
}

impl ToolSource {
    /// Infer the source of a tool from its registered name.
    ///
    /// Names with the `mcp__<server>__<tool>` prefix classify as
    /// [`ToolSource::Mcp`]; names with `skill__<name>__<tool>` classify
    /// as [`ToolSource::Skill`]; everything else is [`ToolSource::Builtin`].
    ///
    /// # Silent-degrade on malformed prefixes
    ///
    /// Inputs that start with `mcp__` or `skill__` but do not contain a
    /// matching second `__` separator (e.g. `"mcp__"`, `"mcp__github"`,
    /// `"skill__brainstorming"`) classify as [`ToolSource::Builtin`]
    /// rather than raising. The parser layer is responsible for emitting
    /// a [`crate::error::ParseWarning::UnknownToolSourcePrefix`] when it
    /// observes such a name, since `infer` does not have access to the
    /// warning accumulator.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_core::model::ToolSource;
    /// assert_eq!(ToolSource::infer("bash"), ToolSource::Builtin);
    /// assert_eq!(ToolSource::infer("mcp__"), ToolSource::Builtin);            // malformed
    /// assert_eq!(ToolSource::infer("mcp__github"), ToolSource::Builtin);      // malformed
    /// ```
    #[must_use]
    pub fn infer(name: &str) -> Self {
        if let Some(rest) = name.strip_prefix("mcp__") {
            if let Some((server, _tool)) = rest.split_once("__") {
                return Self::Mcp {
                    server: server.to_owned(),
                };
            }
        }
        if let Some(rest) = name.strip_prefix("skill__") {
            if let Some((skill, _)) = rest.split_once("__") {
                return Self::Skill {
                    name: skill.to_owned(),
                };
            }
        }
        Self::Builtin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_inference() {
        assert_eq!(ToolSource::infer("bash"), ToolSource::Builtin);
        assert_eq!(ToolSource::infer("view"), ToolSource::Builtin);
        assert_eq!(ToolSource::infer("str_replace_editor"), ToolSource::Builtin);
    }

    #[test]
    fn mcp_inference() {
        assert_eq!(
            ToolSource::infer("mcp__github__search_issues"),
            ToolSource::Mcp {
                server: "github".to_string()
            }
        );
    }

    #[test]
    fn skill_inference() {
        assert_eq!(
            ToolSource::infer("skill__brainstorming__present"),
            ToolSource::Skill {
                name: "brainstorming".to_string()
            }
        );
    }

    #[test]
    fn unknown_prefix_falls_back_to_builtin() {
        assert_eq!(ToolSource::infer("weird__name"), ToolSource::Builtin);
    }

    #[test]
    fn malformed_mcp_prefix_falls_back_to_builtin() {
        // `mcp__` with nothing after → Builtin
        assert_eq!(ToolSource::infer("mcp__"), ToolSource::Builtin);
        // `mcp__server` with no second `__` → Builtin
        assert_eq!(ToolSource::infer("mcp__github"), ToolSource::Builtin);
    }

    #[test]
    fn malformed_skill_prefix_falls_back_to_builtin() {
        // `skill__` with nothing after → Builtin
        assert_eq!(ToolSource::infer("skill__"), ToolSource::Builtin);
        // `skill__name` with no second `__` → Builtin
        assert_eq!(
            ToolSource::infer("skill__brainstorming"),
            ToolSource::Builtin
        );
    }

    #[test]
    fn empty_name_falls_back_to_builtin() {
        assert_eq!(ToolSource::infer(""), ToolSource::Builtin);
    }

    #[test]
    fn display_renders_human_readable() {
        assert_eq!(ToolSource::Builtin.to_string(), "builtin");
        assert_eq!(
            ToolSource::Mcp {
                server: "github".into()
            }
            .to_string(),
            "mcp:github"
        );
        assert_eq!(
            ToolSource::Skill {
                name: "test-skill".into()
            }
            .to_string(),
            "skill:test-skill"
        );
    }

    #[test]
    fn display_avoids_debug_syntax() {
        let rendered = ToolSource::Skill {
            name: "synthetic".into(),
        }
        .to_string();
        assert!(!rendered.contains("Skill"));
        assert!(!rendered.contains("name:"));
        assert!(rendered.contains("synthetic"));
    }
}
