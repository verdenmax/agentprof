//! Self-describing cross-episode call reference.
//!
//! `CallRef { name, index }` replaces bare `usize` indices in `Turn` and
//! `SkillInvocation` so a back-reference can be dereferenced without
//! external context. The `name` field is the key in the relevant
//! `BTreeMap` (e.g. `Episodes.tools[name].calls[index]`).

use serde::{Deserialize, Serialize};

/// A name-qualified index into a per-name call vector.
///
/// # Examples
///
/// ```
/// use agentprof_core::episode::CallRef;
/// let r = CallRef::new("bash".into(), 3);
/// assert_eq!(r.name, "bash");
/// assert_eq!(r.index, 3);
/// ```
///
/// To dereference: `episodes.tools.get(&r.name).and_then(|e| e.calls.get(r.index))`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CallRef {
    /// Name key into the relevant per-name `BTreeMap` (tools/hooks/skills).
    pub name: String,
    /// Zero-based index into that map entry's `calls` (or `invocations`) vector.
    pub index: usize,
}

impl CallRef {
    /// Construct a `CallRef` from name and index.
    #[must_use]
    pub const fn new(name: String, index: usize) -> Self {
        Self { name, index }
    }
}
