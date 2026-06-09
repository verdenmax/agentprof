//! Verify the trait surface includes `load_episodes` after M2.1.1.
//!
//! Compile-only test: if the trait doesn't have the method, this won't compile.

use agentprof_core::analyzer::AnalysisReport;
use agentprof_core::episode::Episodes;
use agentprof_core::{DataSourceError, SessionDataSource, SessionRef};
use std::time::Duration;

#[allow(dead_code)]
fn accepts_with_load_episodes<T: SessionDataSource>(src: &T) {
    let _refs: Result<Vec<SessionRef>, DataSourceError> = src.discover(Duration::from_secs(1));
    let _report: Result<AnalysisReport, DataSourceError> = src.load_session("x");
    let _episodes: Result<Episodes, DataSourceError> = src.load_episodes("x");
}

#[test]
fn trait_compiles_with_load_episodes() {
    // If the file compiles at all, the trait has the method. No runtime assertion needed.
}
