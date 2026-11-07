//! Ingests a real projects folder into a throwaway database and prints counts
//! only, never transcript content. For checking the parser against real files:
//!
//!     cargo run --release --example scan -- [projects-dir] [db-path]

use std::path::PathBuf;
use std::time::Instant;

use claude_task_manager_lib::{db, ingest::scanner};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = match args.next() {
        Some(p) => PathBuf::from(p),
        None => scanner::default_projects_root()?,
    };
    let db_path = match args.next() {
        Some(p) => PathBuf::from(p),
        None => std::env::temp_dir().join("ctm-scan-check.sqlite3"),
    };
    let _ = std::fs::remove_file(&db_path);
    let mut conn = db::open(&db_path)?;

    let started = Instant::now();
    let summary = scanner::scan(&mut conn, &root, |_| {})?;
    println!("first scan: {:?} in {:.2?}", summary, started.elapsed());

    let started = Instant::now();
    let again = scanner::scan(&mut conn, &root, |_| {})?;
    println!(
        "second scan read {} files in {:.2?}",
        again.files_read,
        started.elapsed()
    );

    let count = |sql: &str| -> rusqlite::Result<i64> { conn.query_row(sql, [], |r| r.get(0)) };
    println!("sessions {}", count("SELECT COUNT(*) FROM sessions")?);
    println!("tasks {}", count("SELECT COUNT(*) FROM tasks")?);
    println!(
        "subagent tasks {}",
        count("SELECT COUNT(*) FROM tasks WHERE trigger = 'subagent'")?
    );
    println!(
        "subagent tasks without parent {}",
        count("SELECT COUNT(*) FROM tasks WHERE trigger = 'subagent' AND parent_task_id IS NULL")?
    );
    println!(
        "entries without task (outside preamble) {}",
        count(
            "SELECT COUNT(*) FROM entries e WHERE task_id IS NULL AND EXISTS (
               SELECT 1 FROM entries p WHERE p.source_id = e.source_id
                 AND p.byte_offset < e.byte_offset AND p.kind IN ('prompt','notification','command'))"
        )?
    );
    let mut stmt = conn.prepare(
        "SELECT kind, COUNT(*), SUM(LENGTH(raw_json)) FROM entries GROUP BY kind ORDER BY 2 DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (kind, n, bytes) = row?;
        println!("  {kind:14} {n:6} entries {:8.1} MB", bytes as f64 / 1e6);
    }
    let mut stmt = conn.prepare("SELECT trigger, COUNT(*) FROM tasks GROUP BY trigger")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (t, n) = row?;
        println!("  trigger {t:12} {n}");
    }
    println!(
        "database size {:.1} MB (before WAL checkpoint) at {}",
        std::fs::metadata(&db_path)?.len() as f64 / 1e6,
        db_path.display()
    );
    Ok(())
}
