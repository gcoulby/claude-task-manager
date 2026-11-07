//! Connection set-up, forward-only migrations and pre-migration backups.

pub mod queries;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};

use crate::error::{AppError, AppResult};

/// Numbered migrations, applied in order. Never edit or remove one that has
/// shipped: add a new one instead.
const MIGRATIONS: &[(i64, &str, &str)] = &[
    (1, "init", include_str!("migrations/0001_init.sql")),
    (
        2,
        "task_activity",
        include_str!("migrations/0002_task_activity.sql"),
    ),
    (
        3,
        "replaced_sources",
        include_str!("migrations/0003_replaced_sources.sql"),
    ),
    (
        4,
        "token_usage",
        include_str!("migrations/0004_token_usage.sql"),
    ),
];

pub const LATEST_VERSION: i64 = MIGRATIONS[MIGRATIONS.len() - 1].0;

/// How many pre-migration backups to keep.
const BACKUPS_TO_KEEP: usize = 5;

pub const DB_FILE_NAME: &str = "archive.sqlite3";

/// Opens the archive, backing it up and migrating it if needed.
pub fn open(path: &Path) -> AppResult<Connection> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let conn = Connection::open(path)?;
    configure(&conn)?;
    migrate(&conn, path)?;
    Ok(conn)
}

/// A second connection for a background thread. WAL mode lets it write while
/// the UI connection reads.
pub fn open_existing(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> AppResult<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Ingest and UI reads overlap; wait briefly rather than fail on a lock.
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

pub fn current_version(conn: &Connection) -> AppResult<i64> {
    let has_table: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
            [],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !has_table {
        return Ok(0);
    }
    let version: Option<i64> =
        conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
            r.get(0)
        })?;
    Ok(version.unwrap_or(0))
}

fn migrate(conn: &Connection, path: &Path) -> AppResult<()> {
    let current = current_version(conn)?;
    if current > LATEST_VERSION {
        return Err(AppError::DatabaseTooNew {
            found: current,
            supported: LATEST_VERSION,
        });
    }
    if current == LATEST_VERSION {
        return Ok(());
    }
    // A fresh database has nothing worth backing up.
    if current > 0 {
        backup(conn, path, current)?;
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             name       TEXT NOT NULL,
             applied_at TEXT NOT NULL
         )",
    )?;
    for (version, name, sql) in MIGRATIONS.iter().filter(|(v, _, _)| *v > current) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, applied_at)
             VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            (version, name),
        )?;
        tx.commit()?;
    }
    Ok(())
}

pub fn backup_dir(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .map(|p| p.join("backups"))
        .unwrap_or_else(|| PathBuf::from("backups"))
}

/// Copies the database to `backups/` before a migration and prunes old copies.
fn backup(conn: &Connection, db_path: &Path, version: i64) -> AppResult<PathBuf> {
    let dir = backup_dir(db_path);
    fs::create_dir_all(&dir)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let target = dir.join(format!("archive-v{version}-{stamp}.sqlite3"));
    // VACUUM INTO writes a consistent, compacted copy even with WAL in use.
    conn.execute("VACUUM INTO ?1", [target.to_string_lossy()])?;
    prune_backups(&dir)?;
    Ok(target)
}

fn prune_backups(dir: &Path) -> AppResult<()> {
    let mut backups: Vec<(SystemTime, PathBuf)> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("archive-v") && name.ends_with(".sqlite3")
        })
        .filter_map(|e| {
            let modified = e.metadata().and_then(|m| m.modified()).ok()?;
            Some((modified, e.path()))
        })
        .collect();
    backups.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    for (_, path) in backups.into_iter().skip(BACKUPS_TO_KEEP) {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
pub fn open_in_memory() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    configure(&conn).unwrap();
    migrate(&conn, Path::new(":memory:")).unwrap();
    conn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_migrates_to_latest_without_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE_NAME);
        let conn = open(&path).unwrap();
        assert_eq!(current_version(&conn).unwrap(), LATEST_VERSION);
        assert!(!backup_dir(&path).exists());
    }

    #[test]
    fn reopening_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE_NAME);
        drop(open(&path).unwrap());
        let conn = open(&path).unwrap();
        assert_eq!(current_version(&conn).unwrap(), LATEST_VERSION);
        assert!(!backup_dir(&path).exists());
    }

    #[test]
    fn refuses_database_from_newer_version_and_leaves_it_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE_NAME);
        let conn = open(&path).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations VALUES (?1, 'future', 'x')",
            [LATEST_VERSION + 1],
        )
        .unwrap();
        drop(conn);

        let err = open(&path).unwrap_err();
        assert!(
            matches!(err, AppError::DatabaseTooNew { found, .. } if found == LATEST_VERSION + 1)
        );
        let conn = Connection::open(&path).unwrap();
        assert_eq!(current_version(&conn).unwrap(), LATEST_VERSION + 1);
    }

    #[test]
    fn backs_up_before_migrating_an_existing_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE_NAME);
        let conn = open(&path).unwrap();
        conn.execute("INSERT INTO settings VALUES ('k', 'v')", [])
            .unwrap();

        let target = backup(&conn, &path, 1).unwrap();
        let copy = Connection::open(&target).unwrap();
        let value: String = copy
            .query_row("SELECT value FROM settings WHERE key = 'k'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(value, "v");
    }

    #[test]
    fn upgrading_an_older_archive_backs_up_then_migrates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_FILE_NAME);
        {
            // Build a version 1 archive by hand, as an older release would have.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, name TEXT NOT NULL,
                                                 applied_at TEXT NOT NULL);",
            )
            .unwrap();
            conn.execute_batch(MIGRATIONS[0].2).unwrap();
            conn.execute("INSERT INTO schema_migrations VALUES (1, 'init', 'x')", [])
                .unwrap();
            conn.execute("INSERT INTO settings VALUES ('kept', 'yes')", [])
                .unwrap();
        }

        let conn = open(&path).unwrap();
        assert_eq!(current_version(&conn).unwrap(), LATEST_VERSION);
        let kept: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'kept'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(kept, "yes");

        let backups: Vec<_> = fs::read_dir(backup_dir(&path)).unwrap().collect();
        assert_eq!(backups.len(), 1);
        let name = backups[0].as_ref().unwrap().file_name();
        assert!(name.to_string_lossy().starts_with("archive-v1-"));
    }

    #[test]
    fn keeps_only_the_newest_backups() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..(BACKUPS_TO_KEEP + 3) {
            let p = dir.path().join(format!("archive-v1-{i}.sqlite3"));
            fs::write(&p, b"x").unwrap();
            let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000 + i as u64);
            fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        fs::write(dir.path().join("unrelated.txt"), b"x").unwrap();

        prune_backups(dir.path()).unwrap();
        let mut left: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left.len(), BACKUPS_TO_KEEP + 1);
        assert!(left.contains(&"unrelated.txt".to_string()));
        assert!(!left.contains(&"archive-v1-0.sqlite3".to_string()));
        assert!(left.contains(&format!("archive-v1-{}.sqlite3", BACKUPS_TO_KEEP + 2)));
    }

    #[test]
    fn fts_tracks_inserted_entries() {
        let conn = open_in_memory();
        conn.execute(
            "INSERT INTO sources (path, session_id, project_dir) VALUES ('p', 's', 'd')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entries (source_id, byte_offset, session_id, kind, summary, raw_json)
             VALUES (1, 0, 's', 'prompt', 'add a dark mode toggle', '{}')",
            [],
        )
        .unwrap();
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries_fts WHERE entries_fts MATCH 'toggle'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);
    }
}
