//! Build script for `agentprof-storage`.
//!
//! When the `otlp` feature is enabled, this script compiles the OpenTelemetry
//! collector service `.proto` definitions into Rust server stubs using
//! `tonic-build`. Client stubs are **not** generated here because agentprof
//! acts as an OTLP *receiver* (server); test code that needs OTLP client
//! types reuses the generated stubs from the `opentelemetry-otlp` crate.
//!
//! When the `otlp` feature is disabled, this script is a no-op.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(feature = "otlp")]
    compile_otlp_protos();
}

#[cfg(feature = "otlp")]
fn compile_otlp_protos() {
    use std::path::PathBuf;

    // Ensure tonic_build / prost_build can locate a `protoc` binary even on
    // hosts that don't have one installed system-wide (notably the
    // GitHub-hosted CI runners). User-set `PROTOC` always wins.
    set_vendored_protoc();

    let proto_root = locate_otel_proto_root();
    println!("cargo:rerun-if-changed={}", proto_root.display());

    let proto_paths: Vec<PathBuf> = [
        "opentelemetry/proto/collector/logs/v1/logs_service.proto",
        "opentelemetry/proto/collector/metrics/v1/metrics_service.proto",
        "opentelemetry/proto/collector/trace/v1/trace_service.proto",
    ]
    .iter()
    .map(|p| proto_root.join(p))
    .collect();

    for p in &proto_paths {
        assert!(
            p.exists(),
            "expected OTLP proto file not found: {}",
            p.display()
        );
    }

    let include_dirs = [proto_root];

    let mut prost_cfg = prost_build::Config::new();
    // Suppress message-level doc comments: upstream OTLP `.proto` files contain
    // fenced code samples (e.g. `"/http/user_agent": "..."`) that rustdoc
    // mis-parses as Rust doctests. Service-level comments are filtered
    // separately by tonic-build's own `.disable_comments(".")`.
    prost_cfg.disable_comments(["."]);

    if let Err(e) = tonic_build::configure()
        .build_client(false)
        .build_server(true)
        .disable_comments(".")
        .compile_protos_with_config(prost_cfg, &proto_paths, &include_dirs)
    {
        panic!("failed to compile OTLP collector .proto files with tonic_build: {e}");
    }
}

/// Point `prost_build` at a vendored `protoc` binary if no `PROTOC` env var
/// is already set.
///
/// `protoc-bin-vendored` ships precompiled `protoc` binaries for the common
/// CI targets (`linux` / `macos` / `windows` × `x86_64` / `aarch64`). On
/// platforms it doesn't cover (e.g. linux musl, freebsd), `protoc_bin_path()`
/// returns `Err` and we silently fall through to whatever `prost_build` finds
/// on `PATH` — the user can also force a specific binary via the `PROTOC` env
/// var, which we always honor.
#[cfg(feature = "otlp")]
fn set_vendored_protoc() {
    if std::env::var_os("PROTOC").is_some() {
        // Honor caller's choice (e.g. `nix-shell -p protobuf` exports).
        return;
    }
    match protoc_bin_vendored::protoc_bin_path() {
        Ok(path) => {
            // SAFETY: setting an env var from build.rs is sound — Cargo
            // spawns each build script in a fresh process, so we're not
            // racing other threads.
            std::env::set_var("PROTOC", &path);
            println!("cargo:rerun-if-env-changed=PROTOC");
        }
        Err(err) => {
            // Not fatal — fall back to system protoc. The subsequent
            // `tonic_build` call will produce a clear error if neither
            // source works.
            println!(
                "cargo:warning=protoc-bin-vendored unavailable for this target ({err}); \
                 falling back to system protoc on PATH"
            );
        }
    }
}

/// Locate the `opentelemetry-proto` crate's vendored `.proto` root.
///
/// The `opentelemetry-proto` crate (a transitive dep of `opentelemetry-otlp`)
/// vendors the upstream protobuf definitions inside its own `src/proto/
/// opentelemetry-proto/` directory. Cargo unpacks the crate source under
/// `$CARGO_HOME/registry/src/<index-hash>/opentelemetry-proto-<ver>/`, so we
/// walk every registry index and return the highest-versioned match whose
/// `src/proto/opentelemetry-proto` subdir actually exists.
///
/// Fallback order:
/// 1. `OTEL_PROTO_ROOT` env var (explicit override; must point at the dir
///    containing `opentelemetry/proto/collector/...`).
/// 2. `<workspace_root>/vendor/opentelemetry-proto/proto` (if a future
///    decision vendors protos into the repo).
/// 3. Cargo registry scan (default path for normal builds).
#[cfg(feature = "otlp")]
fn locate_otel_proto_root() -> std::path::PathBuf {
    use std::path::PathBuf;

    if let Ok(env_root) = std::env::var("OTEL_PROTO_ROOT") {
        let p = PathBuf::from(env_root);
        if p.join("opentelemetry/proto/collector/logs/v1/logs_service.proto")
            .exists()
        {
            return p;
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor_candidate = manifest_dir
        .join("..")
        .join("..")
        .join("vendor")
        .join("opentelemetry-proto")
        .join("proto");
    if vendor_candidate
        .join("opentelemetry/proto/collector/logs/v1/logs_service.proto")
        .exists()
    {
        return vendor_candidate;
    }

    let cargo_home = std::env::var("CARGO_HOME").map_or_else(
        |_| {
            let home = std::env::var("HOME")
                .unwrap_or_else(|_| panic!("HOME env var must be set to locate cargo registry"));
            PathBuf::from(home).join(".cargo")
        },
        PathBuf::from,
    );
    let registry_src = cargo_home.join("registry").join("src");

    let mut best: Option<(String, PathBuf)> = None;
    if let Ok(indexes) = std::fs::read_dir(&registry_src) {
        for index_entry in indexes.flatten() {
            let index_path = index_entry.path();
            let Ok(crates) = std::fs::read_dir(&index_path) else {
                continue;
            };
            for crate_entry in crates.flatten() {
                let name = crate_entry.file_name().to_string_lossy().into_owned();
                let Some(version) = name.strip_prefix("opentelemetry-proto-") else {
                    continue;
                };
                let candidate = crate_entry
                    .path()
                    .join("src")
                    .join("proto")
                    .join("opentelemetry-proto");
                if !candidate
                    .join("opentelemetry/proto/collector/logs/v1/logs_service.proto")
                    .exists()
                {
                    continue;
                }
                let replace = match best.as_ref() {
                    Some((v, _)) => version > v.as_str(),
                    None => true,
                };
                if replace {
                    best = Some((version.to_string(), candidate));
                }
            }
        }
    }

    if let Some((_, path)) = best {
        return path;
    }

    panic!(
        "could not find opentelemetry-proto proto dir; searched OTEL_PROTO_ROOT env, \
         {}, and cargo registry under {}. Either run `cargo fetch` to populate \
         the registry, set OTEL_PROTO_ROOT to the directory containing \
         `opentelemetry/proto/collector/...`, or vendor the protos under \
         `vendor/opentelemetry-proto/proto/`.",
        vendor_candidate.display(),
        registry_src.display(),
    );
}
