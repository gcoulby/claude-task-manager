//! Finds transcript files and reads the bytes added since the last scan.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use super::derive;
use super::parser::{Parsed, parse_line};
use crate::db::queries::{self, AgentMeta, NewSource};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: PathBuf,
    pub session_id: String,
    /// Set for `subagents/agent-<id>.jsonl`.
    pub agent_id: Option<String>,
    /// The encoded project folder name, for display when no cwd is known.
    pub project_dir: String,
}

impl SourceFile {
    fn meta_path(&self) -> Option<PathBuf> {
        let agent = self.agent_id.as_ref()?;
        Some(self.path.with_file_name(format!("agent-{agent}.meta.json")))
    }
}

/// `~/.claude/projects`, found through the OS rather than a fixed path.
pub fn default_projects_root() -> AppResult<PathBuf> {
    let home = dirs::home_dir().ok_or(AppError::NoHomeDir)?;
    Ok(home.join(".claude").join("projects"))
}

/// Lists main transcripts (`<project>/<session>.jsonl`) and subagent runs
/// (`<project>/<session>/subagents/agent-<id>.jsonl`). A missing root is not
/// an error: Claude Code may simply not have run yet.
pub fn discover(root: &Path) -> AppResult<Vec<SourceFile>> {
    let mut found = Vec::new();
    let projects = match fs::read_dir(root) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(found),
        Err(e) => return Err(e.into()),
    };
    for project in projects.filter_map(Result::ok) {
        let project_path = project.path();
        if !project_path.is_dir() {
            continue;
        }
        let project_dir = project.file_name().to_string_lossy().into_owned();
        let Ok(items) = fs::read_dir(&project_path) else {
            continue;
        };
        for item in items.filter_map(Result::ok) {
            let path = item.path();
            if path.is_file() && has_ext(&path, "jsonl") {
                if let Some(stem) = file_stem(&path) {
                    found.push(SourceFile {
                        path,
                        session_id: stem,
                        agent_id: None,
                        project_dir: project_dir.clone(),
                    });
                }
            } else if path.is_dir() {
                let session_id = item.file_name().to_string_lossy().into_owned();
                discover_subagents(
                    &path.join("subagents"),
                    &session_id,
                    &project_dir,
                    &mut found,
                );
            }
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

fn discover_subagents(dir: &Path, session_id: &str, project_dir: &str, out: &mut Vec<SourceFile>) {
    let Ok(items) = fs::read_dir(dir) else {
        return;
    };
    for item in items.filter_map(Result::ok) {
        let path = item.path();
        if !path.is_file() || !has_ext(&path, "jsonl") {
            continue;
        }
        let Some(agent_id) =
            file_stem(&path).and_then(|s| s.strip_prefix("agent-").map(String::from))
        else {
            continue;
        };
        out.push(SourceFile {
            path,
            session_id: session_id.to_string(),
            agent_id: Some(agent_id),
            project_dir: project_dir.to_string(),
        });
    }
}

fn has_ext(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e == ext)
}

fn file_stem(path: &Path) -> Option<String> {
    path.file_stem().map(|s| s.to_string_lossy().into_owned())
}

/// Read-only, and on Windows explicitly sharing read, write and delete, so
/// Claude Code can keep appending (or clean up) while we hold the file open.
fn open_shared(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        const FILE_SHARE_WRITE: u32 = 0x2;
        const FILE_SHARE_DELETE: u32 = 0x4;
        opts.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
    }
    opts.open(path)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FileOutcome {
    pub new_entries: usize,
    pub malformed: usize,
    /// False when the file was unchanged and not read at all.
    pub read: bool,
}

/// Reads complete lines added since the stored offset. A trailing line with
/// no newline is still being written, so it is left for the next pass.
pub fn ingest_file(conn: &mut Connection, src: &SourceFile) -> AppResult<FileOutcome> {
    let path_str = src.path.to_string_lossy();
    let metadata = fs::metadata(&src.path)?;
    let size = metadata.len();
    let mtime_ms = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let tx = conn.transaction()?;
    let mut row = match queries::get_source(&tx, &path_str)? {
        Some(r) => r,
        None => queries::insert_source(
            &tx,
            &NewSource {
                path: &path_str,
                session_id: &src.session_id,
                agent_id: src.agent_id.as_deref(),
                project_dir: &src.project_dir,
            },
        )?,
    };
    if row.size == size && row.mtime_ms == mtime_ms {
        return Ok(FileOutcome::default());
    }
    if size < row.byte_offset {
        queries::reset_source(&tx, row.id)?;
        row.byte_offset = 0;
    }
    if let Some(meta_path) = src.meta_path() {
        queries::set_agent_meta(&tx, row.id, &read_agent_meta(&meta_path))?;
    }

    let mut file = open_shared(&src.path)?;
    file.seek(SeekFrom::Start(row.byte_offset))?;
    let mut reader = BufReader::with_capacity(1 << 20, file);
    let mut outcome = FileOutcome {
        read: true,
        ..FileOutcome::default()
    };
    let mut pos = row.byte_offset;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 || buf.last() != Some(&b'\n') {
            break;
        }
        let line = trim_line_end(&buf);
        if !line.is_empty() {
            match parse_line(line) {
                Parsed::Entry(entry) => {
                    let added = queries::insert_entry(
                        &tx,
                        row.id,
                        pos,
                        &src.session_id,
                        src.agent_id.as_deref(),
                        &entry,
                    )?;
                    if added {
                        outcome.new_entries += 1;
                    }
                }
                Parsed::Malformed => outcome.malformed += 1,
                Parsed::Skip => {}
            }
        }
        pos += n as u64;
    }
    queries::set_source_progress(&tx, row.id, pos, size, mtime_ms)?;
    tx.commit()?;
    Ok(outcome)
}

fn trim_line_end(buf: &[u8]) -> &[u8] {
    let mut end = buf.len();
    while end > 0 && matches!(buf[end - 1], b'\n' | b'\r') {
        end -= 1;
    }
    &buf[..end]
}

fn read_agent_meta(path: &Path) -> AgentMeta {
    let Ok(text) = fs::read_to_string(path) else {
        return AgentMeta::default();
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return AgentMeta::default();
    };
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
    AgentMeta {
        parent_tool_use_id: s("toolUseId"),
        agent_type: s("agentType"),
        description: s("description"),
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub files_done: usize,
    pub files_total: usize,
    pub entries_added: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub files: usize,
    pub files_read: usize,
    pub entries_added: usize,
    pub malformed_lines: usize,
    pub sessions_rebuilt: usize,
    /// Files that could not be read, with the reason. Never transcript text.
    pub errors: Vec<String>,
}

/// Ingests every transcript under `root`, then rebuilds the sessions that
/// changed. One unreadable file does not stop the rest.
pub fn scan(
    conn: &mut Connection,
    root: &Path,
    mut on_progress: impl FnMut(&ScanProgress),
) -> AppResult<ScanSummary> {
    let files = discover(root)?;
    let mut summary = ScanSummary {
        files: files.len(),
        ..ScanSummary::default()
    };
    let mut progress = ScanProgress {
        files_total: files.len(),
        ..ScanProgress::default()
    };
    let mut touched = BTreeSet::new();
    for src in &files {
        match ingest_file(conn, src) {
            Ok(o) => {
                if o.read {
                    summary.files_read += 1;
                    touched.insert(src.session_id.clone());
                }
                summary.entries_added += o.new_entries;
                summary.malformed_lines += o.malformed;
                progress.entries_added += o.new_entries;
            }
            Err(e) => summary
                .errors
                .push(format!("{}: {e}", src.path.to_string_lossy())),
        }
        progress.files_done += 1;
        on_progress(&progress);
    }
    for session_id in queries::refresh_missing(conn, |p| Path::new(p).exists())? {
        touched.insert(session_id);
    }
    for session_id in &touched {
        derive::rebuild_session(conn, session_id)?;
    }
    summary.sessions_rebuilt = touched.len();
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::io::Write;

    fn fixtures() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/projects")
    }

    /// Copies the fixtures so tests can append to and delete files freely.
    fn fixture_copy() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        copy_dir(&fixtures(), dir.path());
        dir
    }

    fn copy_dir(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for e in fs::read_dir(from).unwrap() {
            let e = e.unwrap();
            let target = to.join(e.file_name());
            if e.path().is_dir() {
                copy_dir(&e.path(), &target);
            } else {
                fs::copy(e.path(), target).unwrap();
            }
        }
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn discovers_main_and_subagent_files_only() {
        let files = discover(&fixtures()).unwrap();
        let mains = files.iter().filter(|f| f.agent_id.is_none()).count();
        let subs: Vec<_> = files.iter().filter(|f| f.agent_id.is_some()).collect();
        assert_eq!(mains, 4);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].agent_id.as_deref(), Some("a1b2c3d4e5f6a7b8c"));
        assert_eq!(subs[0].session_id, "00000000-0000-4000-8000-000000000001");
    }

    #[test]
    fn missing_root_is_empty_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(discover(&dir.path().join("nope")).unwrap().is_empty());
    }

    #[test]
    fn ingests_fixtures_and_stops_before_partial_line() {
        let dir = fixture_copy();
        let mut conn = db::open_in_memory();
        let summary = scan(&mut conn, dir.path(), |_| {}).unwrap();
        assert_eq!(summary.files, 5);
        assert!(summary.errors.is_empty(), "{:?}", summary.errors);
        assert_eq!(summary.malformed_lines, 0);
        assert_eq!(summary.sessions_rebuilt, 4);

        // Session 003 ends with a partial line, which must not be stored or
        // counted, and the offset must stop before it.
        let path = dir
            .path()
            .join("-Users-alex-code-my-app/00000000-0000-4000-8000-000000000003.jsonl");
        let text = fs::read_to_string(&path).unwrap();
        let complete = text.rfind('\n').unwrap() as i64 + 1;
        let offset: i64 = conn
            .query_row(
                "SELECT byte_offset FROM sources WHERE path = ?1",
                [path.to_string_lossy()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(offset, complete);
        assert_eq!(
            count(
                &conn,
                "SELECT COUNT(*) FROM entries WHERE session_id LIKE '%003'"
            ),
            2
        );
        // Bookkeeping and unknown types are not stored.
        assert_eq!(
            count(
                &conn,
                "SELECT COUNT(*) FROM entries WHERE raw_json LIKE '%session-owner%' OR raw_json LIKE '%some-future-type%'"
            ),
            0
        );
    }

    #[test]
    fn resumes_from_offset_and_completes_partial_line() {
        let dir = fixture_copy();
        let mut conn = db::open_in_memory();
        scan(&mut conn, dir.path(), |_| {}).unwrap();
        let before = count(&conn, "SELECT COUNT(*) FROM entries");

        // Unchanged files are not re-read.
        let again = scan(&mut conn, dir.path(), |_| {}).unwrap();
        assert_eq!((again.files_read, again.entries_added), (0, 0));

        // Finish the partial line in session 003 and add one more.
        let path = dir
            .path()
            .join("-Users-alex-code-my-app/00000000-0000-4000-8000-000000000003.jsonl");
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(br#"age":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_fixture_0301","content":"done"}]},"uuid":"late-1","timestamp":"2026-11-01T10:31:00.000Z"}"#).unwrap();
        f.write_all(b"\n").unwrap();
        f.write_all(br#"{"type":"assistant","uuid":"late-2","message":{"id":"m9","content":[{"type":"text","text":"Installed."}],"stop_reason":"end_turn"}}"#).unwrap();
        f.write_all(b"\n").unwrap();
        drop(f);

        let third = scan(&mut conn, dir.path(), |_| {}).unwrap();
        assert_eq!((third.files_read, third.entries_added), (1, 2));
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM entries"), before + 2);
        assert_eq!(
            count(
                &conn,
                "SELECT COUNT(*) FROM entries WHERE uuid = 'late-1' AND kind = 'tool_result'"
            ),
            1
        );
    }

    #[test]
    fn shrunk_file_is_reread_from_the_start() {
        let dir = fixture_copy();
        let mut conn = db::open_in_memory();
        scan(&mut conn, dir.path(), |_| {}).unwrap();
        let path = dir
            .path()
            .join("-Users-alex-code-demo-app/00000000-0000-4000-8000-000000000002.jsonl");
        let first_line = fs::read_to_string(&path)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string();
        fs::write(&path, format!("{first_line}\n")).unwrap();

        scan(&mut conn, dir.path(), |_| {}).unwrap();
        assert_eq!(
            count(
                &conn,
                "SELECT COUNT(*) FROM entries WHERE session_id LIKE '%002'"
            ),
            1
        );
    }

    #[test]
    fn deleted_source_is_kept_and_flagged() {
        let dir = fixture_copy();
        let mut conn = db::open_in_memory();
        scan(&mut conn, dir.path(), |_| {}).unwrap();
        let before = count(
            &conn,
            "SELECT COUNT(*) FROM entries WHERE session_id LIKE '%004'",
        );
        fs::remove_file(
            dir.path()
                .join("-Users-alex-code-my-app/00000000-0000-4000-8000-000000000004.jsonl"),
        )
        .unwrap();

        let s = scan(&mut conn, dir.path(), |_| {}).unwrap();
        assert_eq!(s.sessions_rebuilt, 1);
        assert_eq!(
            count(
                &conn,
                "SELECT COUNT(*) FROM entries WHERE session_id LIKE '%004'"
            ),
            before
        );
        assert_eq!(
            count(
                &conn,
                "SELECT source_missing FROM sessions WHERE id LIKE '%004'"
            ),
            1
        );
    }

    #[test]
    fn reports_progress_per_file() {
        let dir = fixture_copy();
        let mut conn = db::open_in_memory();
        let mut seen = Vec::new();
        scan(&mut conn, dir.path(), |p| seen.push(p.files_done)).unwrap();
        assert_eq!(seen, vec![1, 2, 3, 4, 5]);
    }
}
