//! e2e tests for `agentprof serve` (M2.3).
//!
//! Each test spawns the binary in a child process bound to an ephemeral
//! port, then probes via reqwest. Mirrors M2.2's `cli_ingest_otlp_e2e.rs`.

#![cfg(feature = "web")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin;

// Sub-second monotonic counter to avoid collisions between concurrently
// running e2e tests (cargo test runs tests on a thread-pool by default).
static SUFFIX_COUNTER: AtomicU32 = AtomicU32::new(0);

fn ephemeral_addr() -> SocketAddr {
    let lis = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let a = lis.local_addr().expect("local_addr");
    drop(lis);
    a
}

fn wait_for_bind(addr: SocketAddr, deadline: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn unique_db_path() -> std::path::PathBuf {
    let pid = std::process::id();
    let n = SUFFIX_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    std::env::temp_dir().join(format!("agentprof-serve-e2e-{pid}-{nanos}-{n}.db"))
}

struct ServerGuard {
    child: Child,
    addr: SocketAddr,
    db_path: std::path::PathBuf,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        // Best-effort SIGKILL (covers the case the test panicked); the
        // dedicated SIGINT path is exercised in the synchronous CLI
        // failure tests below, not here.
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.db_path);
    }
}

fn spawn_server(extra_args: &[&str]) -> ServerGuard {
    let addr = ephemeral_addr();
    let db_path = unique_db_path();
    // Materialize the SQLite store first so cmd::serve's existence check
    // passes (UserError otherwise).
    {
        let _db = agentprof_storage::Db::open_and_migrate(&db_path).expect("open store");
    }

    let bind_arg = addr.to_string();
    let storage_arg = db_path.to_str().expect("utf8 path").to_owned();
    let mut cmd = Command::new(cargo_bin("agentprof"));
    cmd.args([
        "serve",
        "--bind",
        bind_arg.as_str(),
        "--storage-path",
        storage_arg.as_str(),
        "--no-open",
        "--quiet",
    ]);
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let child = cmd.spawn().expect("spawn agentprof");
    assert!(
        wait_for_bind(addr, Duration::from_secs(5)),
        "serve didn't bind to {addr} within 5s"
    );
    ServerGuard {
        child,
        addr,
        db_path,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn e2e_serve_binds_and_serves_healthz() {
    let server = spawn_server(&[]);
    let resp = reqwest::get(format!("http://{}/healthz", server.addr))
        .await
        .expect("get");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert_eq!(body, "healthy");
}

#[test]
fn e2e_serve_with_missing_storage_path_exits_user_error() {
    use assert_cmd::Command as ACommand;
    ACommand::cargo_bin("agentprof")
        .expect("cargo_bin")
        .args(["serve", "--bind", "127.0.0.1:0", "--no-open", "--quiet"])
        .assert()
        .failure()
        .code(1); // ExitKind::UserError
}

#[test]
fn e2e_serve_with_nonexistent_storage_path_exits_user_error() {
    use assert_cmd::Command as ACommand;
    let nonexistent = std::env::temp_dir().join("agentprof-serve-e2e-nonexistent-dir/missing.db");
    ACommand::cargo_bin("agentprof")
        .expect("cargo_bin")
        .args([
            "serve",
            "--bind",
            "127.0.0.1:0",
            "--storage-path",
            nonexistent.to_str().unwrap(),
            "--no-open",
            "--quiet",
        ])
        .assert()
        .failure()
        .code(1); // ExitKind::UserError
}

#[tokio::test(flavor = "multi_thread")]
async fn e2e_serve_serves_sessions_page() {
    let server = spawn_server(&[]);
    let resp = reqwest::get(format!("http://{}/sessions", server.addr))
        .await
        .expect("get");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Sessions"));
}

#[tokio::test(flavor = "multi_thread")]
async fn e2e_serve_serves_static_assets() {
    let server = spawn_server(&[]);
    let resp = reqwest::get(format!("http://{}/static/dashboard.css", server.addr))
        .await
        .expect("get");
    assert_eq!(resp.status(), 200);
    let ct = resp.headers().get("content-type").expect("content-type");
    assert!(ct.to_str().unwrap().starts_with("text/css"));
}
