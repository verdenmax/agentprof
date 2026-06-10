//! OTLP receiver subsystem (M2.2). Gated by the `otlp` cargo feature.
//!
//! # Modules
//!
//! - [`config`] — [`config::OtlpServerConfig`] + [`config::PartialOtlpServerConfig`]
//! - [`error`] — [`error::OtlpServerError`] / [`error::MapperError`] / [`error::RouterError`]
//!
//! Additional modules (`server_grpc` / `server_http` / `auth` / `tls` / `mapper` /
//! `session_router` / `pipeline` / `typed`) land in subsequent M2.2 tasks.

pub mod config;
pub mod error;

pub use config::{OtlpServerConfig, PartialOtlpServerConfig};
pub use error::{MapperError, OtlpServerError, RouterError};
