//! Server-side TLS + mTLS configuration for the OTLP receiver (M2.2 T4.2).
//!
//! Both the gRPC and the HTTP listeners share the same operator-supplied
//! PEM cert+key pair, exposed through
//! [`crate::otlp::config::OtlpServerConfig::tls_cert`] /
//! [`crate::otlp::config::OtlpServerConfig::tls_key`]. When
//! [`crate::otlp::config::OtlpServerConfig::tls_client_ca`] is also
//! present we additionally require + verify a client cert (mutual TLS)
//! via [`rustls::server::WebPkiClientVerifier`].
//!
//! This module deliberately uses the PEM API surface from
//! [`rustls::pki_types::pem::PemObject`] rather than the (unmaintained)
//! `rustls-pemfile` crate; see deny.toml RUSTSEC-2025-0134 for the
//! tracking rationale.
//!
//! # Threading
//!
//! The returned [`rustls::ServerConfig`] is `Send + Sync`; the listener
//! crates clone it (wrapped in `Arc`) into the per-connection acceptor.

use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};

use crate::otlp::error::OtlpServerError;

/// Read a PEM file into memory and surface a typed I/O error on failure.
///
/// Both the gRPC TLS bootstrap (which feeds raw PEM bytes to tonic's
/// `Identity` / `Certificate` helpers) and [`load_server_tls_config`]
/// rely on this helper so paths in error messages come out consistent.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use agentprof_storage::otlp::tls::read_pem_file;
/// let bytes = read_pem_file(Path::new("cert.pem")).unwrap();
/// assert!(!bytes.is_empty());
/// ```
///
/// # Errors
///
/// [`OtlpServerError::Io`] with the offending path on read failure.
pub fn read_pem_file(path: &Path) -> Result<Vec<u8>, OtlpServerError> {
    std::fs::read(path).map_err(|source| OtlpServerError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Build a rustls [`ServerConfig`] from operator-supplied PEM files.
///
/// `cert_pem` and `key_pem` are required. `client_ca` is optional; when
/// supplied the returned config requires + verifies a client certificate
/// signed by a CA in that file (mutual TLS). When absent, client
/// authentication is disabled.
///
/// The key file is parsed via [`PrivateKeyDer::from_pem_slice`], which
/// transparently accepts PKCS#8, PKCS#1 (RSA), and SEC1 (ECDSA) formats.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use agentprof_storage::otlp::tls::load_server_tls_config;
/// let cfg = load_server_tls_config(
///     Path::new("server.crt"),
///     Path::new("server.key"),
///     None,
/// ).unwrap();
/// assert!(cfg.max_early_data_size == 0);
/// ```
///
/// # Errors
///
/// - [`OtlpServerError::Io`] if any input file cannot be read.
/// - [`OtlpServerError::TlsConfig`] if a PEM file contains no usable
///   sections, the key cannot be parsed, the cert+key pair is invalid,
///   or the client-CA root store fails to build.
pub fn load_server_tls_config(
    cert_pem: &Path,
    key_pem: &Path,
    client_ca: Option<&Path>,
) -> Result<ServerConfig, OtlpServerError> {
    let cert_bytes = read_pem_file(cert_pem)?;
    let key_bytes = read_pem_file(key_pem)?;

    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&cert_bytes)
        .collect::<Result<_, _>>()
        .map_err(|e| OtlpServerError::TlsConfig {
            message: format!(
                "failed to parse server certificate {}: {e}",
                cert_pem.display()
            ),
        })?;
    if certs.is_empty() {
        return Err(OtlpServerError::TlsConfig {
            message: format!(
                "no CERTIFICATE sections found in {}; try regenerating the PEM file",
                cert_pem.display()
            ),
        });
    }

    let key =
        PrivateKeyDer::from_pem_slice(&key_bytes).map_err(|e| OtlpServerError::TlsConfig {
            message: format!(
                "failed to parse server private key {} (expected PKCS#8, PKCS#1, or SEC1 PEM): {e}",
                key_pem.display()
            ),
        })?;

    let builder = ServerConfig::builder();

    let cfg = match client_ca {
        Some(ca_path) => {
            let ca_bytes = read_pem_file(ca_path)?;
            let mut roots = RootCertStore::empty();
            let mut added = 0usize;
            for cert in CertificateDer::pem_slice_iter(&ca_bytes) {
                let cert = cert.map_err(|e| OtlpServerError::TlsConfig {
                    message: format!(
                        "failed to parse client CA cert in {}: {e}",
                        ca_path.display()
                    ),
                })?;
                roots.add(cert).map_err(|e| OtlpServerError::TlsConfig {
                    message: format!(
                        "failed to add client CA cert from {} to root store: {e}",
                        ca_path.display()
                    ),
                })?;
                added += 1;
            }
            if added == 0 {
                return Err(OtlpServerError::TlsConfig {
                    message: format!(
                        "no CERTIFICATE sections found in client CA bundle {}",
                        ca_path.display()
                    ),
                });
            }
            let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
                .build()
                .map_err(|e| OtlpServerError::TlsConfig {
                    message: format!("failed to build client cert verifier: {e}"),
                })?;
            builder
                .with_client_cert_verifier(verifier)
                .with_single_cert(certs, key)
                .map_err(|e| OtlpServerError::TlsConfig {
                    message: format!("with_single_cert (mTLS): {e}"),
                })?
        }
        None => builder
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| OtlpServerError::TlsConfig {
                message: format!("with_single_cert: {e}"),
            })?,
    };

    Ok(cfg)
}
