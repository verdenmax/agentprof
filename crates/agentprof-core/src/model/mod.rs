//! Domain model types shared across adapters.
//!
//! - [`session::RawSession`] is the parser's output.
//! - [`meta::SessionMeta`] is metadata extracted from session lifecycle events.
//! - [`tool_source::ToolSource`] classifies tool names.

pub mod meta;
pub mod session;
pub mod tool_source;

pub use meta::SessionMeta;
pub use session::RawSession;
pub use tool_source::ToolSource;
