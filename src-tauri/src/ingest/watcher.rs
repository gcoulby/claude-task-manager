//! Debounced file watching on the projects folder. Events are forwarded to
//! the ingest thread as batches of changed paths.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};

/// Claude Code appends a line at a time while working. Batching for this
/// long turns a burst of appends into one read and one rebuild.
pub const DEBOUNCE: Duration = Duration::from_millis(250);

pub enum WatchEvent {
    Changed(Vec<PathBuf>),
    Error(String),
}

/// Watches `root` recursively until the returned value is dropped.
pub fn watch<F>(root: &Path, forward: F) -> notify::Result<Debouncer<RecommendedWatcher>>
where
    F: Fn(WatchEvent) + Send + 'static,
{
    let mut debouncer = new_debouncer(DEBOUNCE, move |res: DebounceEventResult| match res {
        Ok(events) => forward(WatchEvent::Changed(
            events.into_iter().map(|e| e.path).collect(),
        )),
        Err(e) => forward(WatchEvent::Error(e.to_string())),
    })?;
    debouncer.watcher().watch(root, RecursiveMode::Recursive)?;
    Ok(debouncer)
}

/// Adapts a channel sender into a watch callback.
pub fn to_channel<T: Send + 'static>(
    tx: Sender<T>,
    wrap: fn(WatchEvent) -> T,
) -> impl Fn(WatchEvent) + Send + 'static {
    move |event| {
        // The receiver only goes away when the app is closing.
        let _ = tx.send(wrap(event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::sync::mpsc;

    #[test]
    fn reports_appends_to_a_transcript() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let project = root.join("proj");
        fs::create_dir_all(&project).unwrap();
        let file = project.join("s1.jsonl");
        fs::write(&file, "{}\n").unwrap();

        let (tx, rx) = mpsc::channel();
        let _watcher = watch(&root, to_channel(tx, |e| e)).unwrap();
        // Give the OS watcher a moment to start before writing.
        std::thread::sleep(Duration::from_millis(300));
        let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
        f.write_all(b"{\"type\":\"user\"}\n").unwrap();
        drop(f);

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match rx.recv_timeout(remaining) {
                Ok(WatchEvent::Changed(paths))
                    if paths.iter().any(|p| p.ends_with("proj/s1.jsonl")) =>
                {
                    break;
                }
                Ok(_) => continue,
                Err(e) => panic!("no change event for the transcript: {e}"),
            }
        }
    }
}
