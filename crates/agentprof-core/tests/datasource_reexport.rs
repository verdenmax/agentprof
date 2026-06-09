//! Locks the crate-root re-exports of the `datasource` module so that
//! downstream callers can write `use agentprof_core::SessionDataSource`
//! instead of the longer `use agentprof_core::datasource::SessionDataSource`.
//!
//! `SessionRef` construction by field literal is **not** exercised here:
//! the struct is `#[non_exhaustive]`, which forbids cross-crate struct
//! literals. That assertion lives in
//! `crates/agentprof-core/src/datasource.rs` `#[cfg(test)] mod tests`.

use std::time::Duration;

use agentprof_core::{DataSourceError, SessionDataSource, SessionRef};

struct Stub;

impl SessionDataSource for Stub {
    fn name(&self) -> &'static str {
        "stub"
    }

    fn discover(&self, _since: Duration) -> Result<Vec<SessionRef>, DataSourceError> {
        Ok(Vec::new())
    }

    fn load_session(
        &self,
        id: &str,
    ) -> Result<agentprof_core::analyzer::AnalysisReport, DataSourceError> {
        Err(DataSourceError::NotFound { id: id.to_owned() })
    }
}

fn accepts_datasource<T: SessionDataSource>(_: T) {}

#[test]
fn trait_is_reexported_at_crate_root() {
    accepts_datasource(Stub);
}

#[test]
fn error_variant_is_reexported_at_crate_root() {
    let e = DataSourceError::NotFound { id: "x".into() };
    assert_eq!(e.to_string(), "session not found: x");
}

#[test]
fn stub_load_session_returns_not_found() {
    let s = Stub;
    match s.load_session("missing") {
        Err(DataSourceError::NotFound { id }) => assert_eq!(id, "missing"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}
