//! Application runtime stub.
//!
//! Full `AppRunner` (state machine + event loop) lands in T6. T1 only
//! ships [`terminal`] (panic-safe terminal lifecycle) so downstream tasks
//! have a stable entry/leave contract to build on.

pub mod terminal;
