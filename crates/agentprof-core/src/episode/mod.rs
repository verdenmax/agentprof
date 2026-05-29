//! # Episode aggregation
//!
//! Derived types built on top of `RawSession<E>` from any adapter. Episodes
//! are the **shared analysis substrate** across agents — `agentprof-core`
//! defines them once; per-agent adapters never customize them.
//!
//! ## Module layout
//!
//! | Module | Type | Purpose |
//! |---|---|---|
//! | [`call_ref`] | [`CallRef`] | Name-qualified back-reference (`{name, index}`) used by `Turn` and `SkillInvocation` |
//! | [`turn`] | [`Turn`], [`TurnStatus`], [`Span`], [`AbortInfo`] | Per-assistant-turn aggregation |
//! | [`tool`] | [`ToolEpisode`], [`ToolCall`], [`ToolCallStatus`] | Tool-name-keyed call history |
//! | [`hook`] | [`HookEpisode`], [`HookCall`] | Hook-name-keyed call history |
//! | [`skill`] | [`SkillEpisode`], [`SkillInvocation`] | Skill-name-keyed invocation history + triggered-tool window |
//! | [`mode_segment`] | [`ModeSegment`], [`Mode`] | Time-ranged "ask / auto / expert" segments |
//! | [`episodes`] | [`Episodes`] | Container holding all of the above + `warnings` |
//! | [`warning`] | [`DeriveWarning`] | 4-variant data-quality enum |
//! | [`mod@derive`] | [`derive_episodes`] | The pure aggregation function (Task 10) |
//!
//! ## Stability
//!
//! All public types are `#[non_exhaustive]`. Construct them via the provided
//! `pub const fn new(...)` constructors (mirroring `SessionMeta::new` /
//! `RawSession::new` from M1.2).
//!
//! See `docs/internals/adr-0004-episode-derivation.md` for the algorithm
//! design rationale.

pub mod call_ref;
pub mod derive;
pub mod episodes;
pub mod hook;
pub mod mode_segment;
pub mod skill;
pub mod tool;
pub mod turn;
pub mod warning;

pub use call_ref::CallRef;
pub use derive::{derive_episodes, ORPHAN_TOOL_SENTINEL};
pub use episodes::Episodes;
pub use hook::{HookCall, HookEpisode};
pub use mode_segment::{Mode, ModeSegment};
pub use skill::{SkillEpisode, SkillInvocation};
pub use tool::{ToolCall, ToolCallStatus, ToolEpisode};
pub use turn::{AbortInfo, Span, Turn, TurnStatus};
pub use warning::DeriveWarning;
