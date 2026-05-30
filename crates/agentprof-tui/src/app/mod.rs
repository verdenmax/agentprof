//! Event loop, view switching, terminal lifecycle.
//!
//! See [`terminal`] for the panic-safe `enter`/`leave` pair, [`event`] for
//! the input event abstraction, and [`state`] for the pure-logic state
//! machine. The `AppRunner` wiring lives in T6.

pub mod event;
pub mod state;
pub mod terminal;
