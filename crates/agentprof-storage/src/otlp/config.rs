//! OTLP server configuration. See [ADR-0021].
//!
//! The receiver layer reads a TOML-friendly [`PartialOtlpServerConfig`]
//! (all fields optional) and resolves it through
//! [`OtlpServerConfig::from_partial`] into a fully-typed
//! [`OtlpServerConfig`] with default fall-backs. A CLI flag layer may then
//! overwrite individual fields before calling [`OtlpServerConfig::validate`]
//! to enforce cross-field invariants.
//!
//! [ADR-0021]: ../../../docs/internals/adr-0021-otlp-receiver-architecture.md

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use crate::otlp::error::OtlpServerError;

const DEFAULT_GRPC_PORT: u16 = 4317;
const DEFAULT_HTTP_PORT: u16 = 4318;
const DEFAULT_IDLE_SECS: u64 = 300;
const DEFAULT_GRACE_SECS: u64 = 10;

/// Resolved OTLP receiver configuration.
///
/// Built from a [`PartialOtlpServerConfig`] (TOML-deserialized) via
/// [`OtlpServerConfig::from_partial`]. CLI flag layer can post-process
/// the resolved config by overwriting specific fields.
///
/// # Examples
///
/// ```
/// use agentprof_storage::otlp::config::OtlpServerConfig;
/// let cfg = OtlpServerConfig::default();
/// assert_eq!(cfg.listen_grpc.unwrap().to_string(), "127.0.0.1:4317");
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct OtlpServerConfig {
    /// gRPC listener address. `None` disables the gRPC listener.
    pub listen_grpc: Option<SocketAddr>,
    /// HTTP/protobuf listener address. `None` disables the HTTP listener.
    pub listen_http: Option<SocketAddr>,
    /// Bearer token required on every request (`Authorization: Bearer <T>`).
    /// `None` disables bearer auth.
    pub listen_token: Option<String>,
    /// TLS server certificate path (PEM). When `Some`, **both** listeners
    /// serve over TLS.
    pub tls_cert: Option<PathBuf>,
    /// TLS server key path (PEM). Required when `tls_cert` is `Some`.
    pub tls_key: Option<PathBuf>,
    /// Client CA path (PEM). When `Some`, requires + verifies client certs
    /// (mutual TLS). Implies TLS (so `tls_cert` + `tls_key` must also be set).
    pub tls_client_ca: Option<PathBuf>,
    /// Per-session idle flush threshold. Default 5 min.
    pub session_idle_timeout: Duration,
    /// Maximum graceful shutdown wait after SIGINT / SIGTERM. Default 10s.
    pub shutdown_grace: Duration,
}

impl Default for OtlpServerConfig {
    fn default() -> Self {
        let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
        Self {
            listen_grpc: Some(SocketAddr::new(loopback, DEFAULT_GRPC_PORT)),
            listen_http: Some(SocketAddr::new(loopback, DEFAULT_HTTP_PORT)),
            listen_token: None,
            tls_cert: None,
            tls_key: None,
            tls_client_ca: None,
            session_idle_timeout: Duration::from_secs(DEFAULT_IDLE_SECS),
            shutdown_grace: Duration::from_secs(DEFAULT_GRACE_SECS),
        }
    }
}

impl OtlpServerConfig {
    /// Build a resolved config from a TOML-deserialized partial.
    ///
    /// Empty-string listener addresses disable the corresponding listener.
    /// Duration fields accept a tiny humantime-style syntax (`<N>s`, `<N>m`,
    /// `<N>h`) — no external `humantime` crate dependency.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::otlp::config::{OtlpServerConfig, PartialOtlpServerConfig};
    /// let cfg = OtlpServerConfig::from_partial(PartialOtlpServerConfig::default()).unwrap();
    /// assert!(cfg.listen_grpc.is_some());
    /// ```
    ///
    /// # Errors
    ///
    /// - [`OtlpServerError::Config`] if a `SocketAddr` string is malformed.
    /// - [`OtlpServerError::Config`] if a duration string is unparseable
    ///   (empty, missing unit, unknown unit, or non-integer number).
    pub fn from_partial(p: PartialOtlpServerConfig) -> Result<Self, OtlpServerError> {
        let mut cfg = Self::default();

        if let Some(s) = p.listen_grpc {
            cfg.listen_grpc = parse_optional_addr(&s, "listen_grpc")?;
        }
        if let Some(s) = p.listen_http {
            cfg.listen_http = parse_optional_addr(&s, "listen_http")?;
        }
        cfg.listen_token = p.listen_token.filter(|t| !t.is_empty());
        cfg.tls_cert = p.tls_cert;
        cfg.tls_key = p.tls_key;
        cfg.tls_client_ca = p.tls_client_ca;
        if let Some(s) = p.session_idle_timeout {
            cfg.session_idle_timeout = parse_duration(&s, "session_idle_timeout")?;
        }
        if let Some(s) = p.shutdown_grace {
            cfg.shutdown_grace = parse_duration(&s, "shutdown_grace")?;
        }

        Ok(cfg)
    }

    /// Validate cross-field invariants.
    ///
    /// Specifically:
    /// - at least one of `listen_grpc` / `listen_http` must be enabled;
    /// - `tls_cert` and `tls_key` must be set as a pair;
    /// - `tls_client_ca` (mTLS) implies server TLS is also configured.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::otlp::config::OtlpServerConfig;
    /// OtlpServerConfig::default().validate().unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// [`OtlpServerError::Config`] when any invariant above is violated.
    pub fn validate(&self) -> Result<(), OtlpServerError> {
        if self.listen_grpc.is_none() && self.listen_http.is_none() {
            return Err(OtlpServerError::Config(
                "both --listen-grpc and --listen-http disabled; nothing to serve".into(),
            ));
        }
        match (self.tls_cert.is_some(), self.tls_key.is_some()) {
            (true, false) | (false, true) => {
                return Err(OtlpServerError::Config(
                    "--tls-cert and --tls-key must both be set or both unset".into(),
                ));
            }
            _ => {}
        }
        if self.tls_client_ca.is_some() && self.tls_cert.is_none() {
            return Err(OtlpServerError::Config(
                "--tls-client-ca requires --tls-cert + --tls-key (mTLS implies server TLS)".into(),
            ));
        }
        Ok(())
    }
}

/// User-supplied partial config (TOML-friendly, all fields optional).
///
/// Resolved into [`OtlpServerConfig`] via [`OtlpServerConfig::from_partial`].
/// Listener addresses are strings (so an empty string can mean "disable
/// this listener"); durations are strings parsed with a minimal
/// humantime-style grammar.
///
/// # Examples
///
/// ```
/// use agentprof_storage::otlp::config::{OtlpServerConfig, PartialOtlpServerConfig};
/// let partial = PartialOtlpServerConfig {
///     listen_grpc: Some(String::new()), // disable gRPC
///     ..Default::default()
/// };
/// let cfg = OtlpServerConfig::from_partial(partial).unwrap();
/// assert!(cfg.listen_grpc.is_none());
/// ```
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PartialOtlpServerConfig {
    /// gRPC listener address as a string (`"host:port"`); empty string disables.
    pub listen_grpc: Option<String>,
    /// HTTP listener address.
    pub listen_http: Option<String>,
    /// Bearer token.
    pub listen_token: Option<String>,
    /// TLS cert path.
    pub tls_cert: Option<PathBuf>,
    /// TLS key path.
    pub tls_key: Option<PathBuf>,
    /// mTLS client CA path.
    pub tls_client_ca: Option<PathBuf>,
    /// Idle timeout as humantime string (e.g. `"5m"`).
    pub session_idle_timeout: Option<String>,
    /// Shutdown grace as humantime string.
    pub shutdown_grace: Option<String>,
}

fn parse_optional_addr(
    s: &str,
    field: &'static str,
) -> Result<Option<SocketAddr>, OtlpServerError> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(None);
    }
    SocketAddr::from_str(t)
        .map(Some)
        .map_err(|e| OtlpServerError::Config(format!("invalid {field}: {t:?}: {e}")))
}

fn parse_duration(s: &str, field: &'static str) -> Result<Duration, OtlpServerError> {
    // Minimal humantime-style parser to avoid pulling the humantime crate.
    // Supports: <N>s, <N>m, <N>h. N must be a non-negative integer.
    let t = s.trim();
    if t.is_empty() {
        return Err(OtlpServerError::Config(format!("empty {field} duration")));
    }
    let split_at = t.find(|c: char| !c.is_ascii_digit()).ok_or_else(|| {
        OtlpServerError::Config(format!("{field}: missing unit on {t:?}; use Ns/Nm/Nh"))
    })?;
    let (num_str, unit) = t.split_at(split_at);
    let n: u64 = num_str
        .parse()
        .map_err(|e| OtlpServerError::Config(format!("{field}: invalid number in {t:?}: {e}")))?;
    let secs = match unit {
        "s" => n,
        "m" => n.saturating_mul(60),
        "h" => n.saturating_mul(3600),
        other => {
            return Err(OtlpServerError::Config(format!(
                "{field}: unknown unit {other:?} in {t:?}; use s/m/h"
            )));
        }
    };
    Ok(Duration::from_secs(secs))
}
