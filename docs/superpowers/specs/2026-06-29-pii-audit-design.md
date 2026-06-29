# PII Audit Tool + CI Guard (L-11/L-12)

| Field | Value |
|---|---|
| Date | 2026-06-29 |
| Status | Approved — entering writing-plans |
| Author | L-11/L-12 closure |
| Touches ADRs | none (single obvious approach; §5.5 SKIP) |
| Target | v0.4.0 (additive xtask + CI job) |

## 1. Problem
L-11/L-12: fixture/report de-identification is manual `sed` (privacy.md §3/§5); nothing stops a real `/home/<user>/` path being committed. Add one scanner + a CI gate.

## 2. Scope
### In
- `cargo xtask audit-pii <path>` — recursively scan a file/dir for high-confidence real-path PII; nonzero exit on hit.
- New CI job `pii-guard` runs it over `crates/` (fixtures + src).
- Tests; privacy.md §5 → shipped; ROADMAP L-11/L-12 → fixed; CHANGELOG; xtask README.
### Out
- raw-UUID/branch scanning (synthetic fixtures legitimately contain fake ones → false positives).
- auto-redaction (audit only; manual sed recipe stays in privacy.md §3).

## 3. Design
- **`xtask/src/pii.rs`**: `AuditPiiCmd { path: PathBuf }` + `run`. Walk path (dir recursive / single file); skip `target/`, `.git/`, binary (non-UTF8) files. Per UTF-8 text line, match 3 patterns: `/home/<seg>/`, `/Users/<seg>/`, `C:\Users\` where `<seg>` ∉ {`USER`,`<user>`,`<username>`} (allowlist placeholders). Print `path:line: <matched>`. Exit `2` if ≥1 hit, `0` clean. Patterns via `regex` (workspace dep).
- **main.rs**: add `AuditPii(pii::AuditPiiCmd)` variant + dispatch.
- **CI** `.github/workflows/ci.yml`: job `pii-guard` → `cargo xtask audit-pii crates`.

## 4. Tests (xtask/tests)
- dir with `/home/alice/x` → exit 2 + path:line. clean dir → 0. `/home/USER/` placeholder → 0. `/Users/bob/` + `C:\Users\bob` → 2. binary file skipped.

## 5. Self-review
No placeholders; scope one plan; patterns pinned (3 + allowlist); audit-only (no redaction). regex is workspace dep (verify in plan).
