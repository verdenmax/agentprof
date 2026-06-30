//! `Db` — owns a single [`rusqlite::Connection`] with migrations applied.
//!
//! This module is the entry point for all `SQLite` persistence in
//! `agentprof-storage`. Opening a [`Db`] applies the standard pragmas
//! (`journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`) and runs
//! every embedded migration in `crates/agentprof-storage/migrations/` to
//! the latest version. The operation is idempotent: re-opening an existing
//! database performs a no-op migration step.
//!
//! The DDL is normative in [`docs/architecture.md`] §9.
//!
//! [`docs/architecture.md`]: https://github.com/verdenmax/agentprof/blob/main/docs/architecture.md#9-sqlite-schema

use std::fs;
use std::path::Path;

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

use crate::error::SqliteError;

/// Embedded SQL migrations, ordered. Numeric prefix gives the apply order;
/// the textual name is for debugging only.
const MIGRATIONS: &[(&str, &str)] = &[
    ("001_initial", include_str!("../migrations/001_initial.sql")),
    (
        "002_episodes_column",
        include_str!("../migrations/002_episodes_column.sql"),
    ),
];

/// `SQLite` database handle for agentprof persistence.
///
/// Constructed via [`Db::open_and_migrate`] (file-backed) or
/// [`Db::open_in_memory`] (ephemeral, primarily for tests). Both
/// constructors apply pragmas and run every embedded migration; callers do
/// not need to manage schema versioning themselves.
///
/// # Examples
///
/// ```
/// use agentprof_storage::Db;
///
/// let db = Db::open_in_memory().expect("open in-memory db");
/// assert!(db.table_names_for_test().iter().any(|t| t == "sessions"));
/// ```
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (or create) the database at `path` and migrate it to HEAD.
    ///
    /// Missing parent directories are created (`mkdir -p`). The standard
    /// pragmas are applied before migrations run. Calling this on an
    /// already-migrated database is a no-op.
    ///
    /// # Errors
    ///
    /// - [`SqliteError::Io`] if the parent directory cannot be created.
    /// - [`SqliteError::Rusqlite`] if the file cannot be opened or pragmas fail.
    /// - [`SqliteError::Migration`] if any migration fails to apply.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::Db;
    /// let tmp = tempfile::tempdir().unwrap();
    /// let path = tmp.path().join("agentprof.sqlite3");
    /// let _db = Db::open_and_migrate(&path).unwrap();
    /// ```
    pub fn open_and_migrate(path: &Path) -> Result<Self, SqliteError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| SqliteError::Io {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
        }

        let mut conn = Connection::open(path).map_err(|e| SqliteError::Rusqlite {
            context: format!("opening database at {}", path.display()),
            source: e,
        })?;
        Self::apply_pragmas(&conn)?;
        Self::apply_migrations(&mut conn)?;
        Ok(Self { conn })
    }

    /// Open an in-memory database and migrate it to HEAD.
    ///
    /// The database is destroyed when the returned [`Db`] is dropped.
    /// Intended for tests and ephemeral computations.
    ///
    /// # Errors
    ///
    /// Same conditions as [`Db::open_and_migrate`], minus the filesystem
    /// I/O failure mode.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::Db;
    /// let db = Db::open_in_memory().unwrap();
    /// assert!(!db.table_names_for_test().is_empty());
    /// ```
    pub fn open_in_memory() -> Result<Self, SqliteError> {
        let mut conn = Connection::open_in_memory().map_err(|e| SqliteError::Rusqlite {
            context: "opening in-memory database".to_string(),
            source: e,
        })?;
        Self::apply_pragmas(&conn)?;
        Self::apply_migrations(&mut conn)?;
        Ok(Self { conn })
    }

    /// Test-only helper: list user-table names in this database.
    ///
    /// Returns an empty `Vec` on any query failure rather than panicking,
    /// so it satisfies the workspace `clippy::unwrap_used` /
    /// `clippy::expect_used` denies. Used by integration tests to verify
    /// that migrations ran.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::Db;
    /// let db = Db::open_in_memory().unwrap();
    /// let names = db.table_names_for_test();
    /// assert!(names.iter().any(|t| t == "tools_loaded"));
    /// ```
    #[doc(hidden)]
    pub fn table_names_for_test(&self) -> Vec<String> {
        let Ok(mut stmt) = self.conn.prepare(
            "SELECT name FROM sqlite_master \
             WHERE type='table' AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
            return Vec::new();
        };
        rows.filter_map(Result::ok).collect()
    }

    /// Borrow the underlying connection (read-only access for sibling modules).
    ///
    /// **In-crate** callers (admin / query / datasource) use this. For
    /// raw SQL access from **integration tests** (outside the crate
    /// boundary, where `pub(crate)` is not reachable) see
    /// [`Self::conn_for_test`].
    #[allow(dead_code)]
    pub(crate) const fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Test-only helper: borrow the underlying [`Connection`] for raw SQL
    /// `SELECT`s in **integration tests** under
    /// `crates/agentprof-storage/tests/` (e.g. `COUNT(*)` assertions in
    /// `tests/upsert_smoke.rs`).
    ///
    /// Production / in-crate code must NOT call this — use
    /// [`Self::conn`] (`pub(crate)`) instead. The M2.1 audit (P2-2)
    /// caught \`admin\` / \`query\` reaching for this helper despite
    /// being in the same crate; they were migrated to \`conn()\`.
    ///
    /// Hidden from public rustdoc.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_storage::Db;
    /// let db = Db::open_in_memory().unwrap();
    /// let n: i64 = db
    ///     .conn_for_test()
    ///     .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
    ///     .unwrap();
    /// assert_eq!(n, 0);
    /// ```
    #[doc(hidden)]
    pub const fn conn_for_test(&self) -> &Connection {
        &self.conn
    }

    /// Mutably borrow the underlying connection (sibling modules issuing
    /// transactions).
    #[allow(dead_code)]
    pub(crate) const fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    fn apply_pragmas(conn: &Connection) -> Result<(), SqliteError> {
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| SqliteError::Rusqlite {
                context: "setting journal_mode=WAL".to_string(),
                source: e,
            })?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| SqliteError::Rusqlite {
                context: "setting synchronous=NORMAL".to_string(),
                source: e,
            })?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| SqliteError::Rusqlite {
                context: "setting foreign_keys=ON".to_string(),
                source: e,
            })?;
        Ok(())
    }

    fn apply_migrations(conn: &mut Connection) -> Result<(), SqliteError> {
        let ms: Vec<M<'static>> = MIGRATIONS.iter().map(|(_, sql)| M::up(sql)).collect();
        let migrations = Migrations::new(ms);
        migrations.to_latest(conn)?;
        Ok(())
    }
}
