//! Domain model types shared across adapters.
//!
//! - [`session::RawSession`] is the parser's output.
//! - [`meta::SessionMeta`] is metadata extracted from session lifecycle events.
//! - [`tool_source::ToolSource`] classifies tool names.
//! - [`waste`] holds the MCP-server waste data model (M1.6.5).

pub mod meta;
pub mod session;
pub mod tool_source;
pub mod waste;

pub use meta::SessionMeta;
pub use session::RawSession;
pub use tool_source::ToolSource;
pub use waste::{
    AggregateWasteReport, LoadedSource, McpServerCrossWaste, McpServerWaste,
    McpToolUsageAcrossSessions, McpToolWaste, WasteDataSource, WasteReport,
};
