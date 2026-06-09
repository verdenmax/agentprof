//! Integration tests for [`agentprof_storage::Db`] open + migrate (M2.1 T2.3).

#![allow(clippy::expect_used, clippy::used_underscore_binding)]

use agentprof_storage::Db;

#[test]
fn open_creates_file_and_tables() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let path = tmp.path().join("nested").join("agentprof.sqlite3");

    let db = Db::open_and_migrate(&path).expect("open_and_migrate");

    assert!(
        path.exists(),
        "database file should exist at {}",
        path.display()
    );

    let tables = db.table_names_for_test();
    for expected in ["sessions", "tools_loaded", "turn_buckets"] {
        assert!(
            tables.iter().any(|t| t == expected),
            "expected table `{expected}` in {tables:?}"
        );
    }
}

#[test]
fn open_in_memory_works() {
    let db = Db::open_in_memory().expect("open_in_memory");
    let tables = db.table_names_for_test();
    for expected in ["sessions", "tools_loaded", "turn_buckets"] {
        assert!(
            tables.iter().any(|t| t == expected),
            "expected table `{expected}` in {tables:?}"
        );
    }
}

#[test]
fn open_is_idempotent() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let path = tmp.path().join("agentprof.sqlite3");

    let _db1 = Db::open_and_migrate(&path).expect("first open");
    drop(_db1);
    let db2 = Db::open_and_migrate(&path).expect("re-open same path");
    assert!(db2.table_names_for_test().iter().any(|t| t == "sessions"));
}

#[test]
fn open_creates_episodes_column_from_migration_002() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let path = tmp.path().join("test.sqlite");
    let db = Db::open_and_migrate(&path).expect("open ok");

    let conn = db.conn_for_test();
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'episodes_json'",
            [],
            |r| r.get::<_, i64>(0).map(|n| n > 0),
        )
        .expect("query column");
    assert!(
        exists,
        "expected sessions.episodes_json column after migration 002"
    );
}
