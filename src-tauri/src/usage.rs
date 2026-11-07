//! Plan usage from Claude Code's own `/usage` command.
//!
//! The limits for your plan (the current session and week) are not in the
//! transcripts. Claude Code reports them through `/usage`, which runs locally
//! without calling a model, costs nothing, and with
//! `--no-session-persistence` writes no transcript. The output is display
//! text, not a contract, so parsing is tolerant and the raw text is always
//! kept as a fallback.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::db::queries;
use crate::state::AppState;

pub const EVENT_PLAN_USAGE: &str = "usage://plan";
pub const DEFAULT_INTERVAL_MINUTES: i64 = 5;
const RUN_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meter {
    /// For example "Current session" or "Current week (all models)".
    pub label: String,
    pub percent: u8,
    /// As Claude Code words it, for example "Nov 7 at 5pm (Europe/London)".
    pub resets: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanUsage {
    pub fetched_at_ms: i64,
    pub meters: Vec<Meter>,
    /// The full `/usage` text, shown when the meters cannot be read.
    pub raw: String,
    pub error: Option<String>,
    /// The `claude` binary used, so Settings can show which one was found.
    pub binary: Option<String>,
}

pub enum UsageMsg {
    /// Check now, for example from the refresh button or after a settings change.
    Refresh,
}

static METER: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r"(?m)^\s*(?P<label>[^:\n]{1,80}?):\s*(?P<pct>\d{1,3})%\s*used(?:\s*·\s*resets\s+(?P<resets>[^\n]+?))?\s*$",
    )
    .ok()
});

pub fn parse_meters(text: &str) -> Vec<Meter> {
    let Some(re) = METER.as_ref() else {
        return Vec::new();
    };
    re.captures_iter(text)
        .filter_map(|c| {
            Some(Meter {
                label: c.name("label")?.as_str().trim().to_string(),
                percent: c.name("pct")?.as_str().parse::<u16>().ok()?.min(100) as u8,
                resets: c.name("resets").map(|m| m.as_str().trim().to_string()),
            })
        })
        .collect()
}

/// Where Claude Code is usually installed. A desktop app does not inherit
/// the shell's PATH, so the common locations are checked directly.
fn candidates(home: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for n in names {
                out.push(dir.join(n));
            }
        }
    }
    if let Some(home) = home {
        for n in names {
            out.push(home.join(".local").join("bin").join(n));
            out.push(home.join(".claude").join("local").join(n));
        }
        if cfg!(windows) {
            out.push(
                home.join("AppData")
                    .join("Roaming")
                    .join("npm")
                    .join("claude.cmd"),
            );
        }
    }
    if !cfg!(windows) {
        out.push(PathBuf::from("/opt/homebrew/bin/claude"));
        out.push(PathBuf::from("/usr/local/bin/claude"));
    }
    // Last resort: the copy bundled with the newest VS Code extension.
    if let Some(home) = home
        && let Ok(dirs) = std::fs::read_dir(home.join(".vscode").join("extensions"))
    {
        let mut ext: Vec<PathBuf> = dirs
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("anthropic.claude-code-"))
            })
            .collect();
        ext.sort();
        if let Some(newest) = ext.last() {
            let bin = if cfg!(windows) {
                "claude.exe"
            } else {
                "claude"
            };
            out.push(newest.join("resources").join("native-binary").join(bin));
        }
    }
    out
}

pub fn find_claude(custom: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = custom.map(str::trim).filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p)).filter(|p| p.is_file());
    }
    candidates(dirs::home_dir().as_deref())
        .into_iter()
        .find(|p| p.is_file())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Runs `claude -p /usage` once and reads its result.
pub fn fetch(binary: &Path, workdir: &Path) -> PlanUsage {
    let mut usage = PlanUsage {
        fetched_at_ms: now_ms(),
        binary: Some(binary.to_string_lossy().into_owned()),
        ..PlanUsage::default()
    };
    match run(binary, workdir) {
        Ok(stdout) => {
            let text = serde_json::from_str::<Value>(&stdout)
                .ok()
                .and_then(|v| {
                    if v.get("is_error").and_then(Value::as_bool) == Some(true) {
                        usage.error = Some("Claude Code reported an error for /usage.".into());
                    }
                    v.get("result").and_then(Value::as_str).map(String::from)
                })
                .unwrap_or(stdout);
            usage.meters = parse_meters(&text);
            if usage.meters.is_empty() && usage.error.is_none() {
                usage.error =
                    Some("Could not read the usage figures; showing the raw text.".into());
            }
            usage.raw = text;
        }
        Err(e) => usage.error = Some(e),
    }
    usage
}

fn run(binary: &Path, workdir: &Path) -> Result<String, String> {
    std::fs::create_dir_all(workdir).map_err(|e| e.to_string())?;
    let is_cmd = binary
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd"));
    let mut cmd = if is_cmd {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(binary);
        c
    } else {
        Command::new(binary)
    };
    cmd.args([
        "-p",
        "/usage",
        "--no-session-persistence",
        "--output-format",
        "json",
    ])
    .current_dir(workdir)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up every few minutes.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start Claude Code: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > RUN_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Claude Code took too long to report usage.".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("Claude Code exited with {}.", output.status));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

struct Config {
    enabled: bool,
    interval: Duration,
    custom_binary: Option<String>,
}

fn read_config(state: &AppState) -> Config {
    let get = |k: &str| state.with_db(|c| queries::get_setting(c, k)).ok().flatten();
    let minutes = get("plan_usage_minutes")
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|m| *m > 0)
        .unwrap_or(DEFAULT_INTERVAL_MINUTES);
    Config {
        enabled: get("plan_usage_enabled").is_none_or(|v| v != "false"),
        interval: Duration::from_secs(minutes as u64 * 60),
        custom_binary: get("claude_path"),
    }
}

/// Checks usage now and then on the configured interval, until the app
/// closes. A Refresh message checks straight away and re-reads settings.
pub fn spawn(app: AppHandle, rx: Receiver<UsageMsg>) {
    let spawned = std::thread::Builder::new()
        .name("plan-usage".into())
        .spawn(move || {
            let workdir = match app.path().app_data_dir() {
                Ok(d) => d.join("usage-probe"),
                Err(_) => return,
            };
            loop {
                let state = app.state::<AppState>();
                let config = read_config(&state);
                if config.enabled {
                    let usage = match find_claude(config.custom_binary.as_deref()) {
                        Some(bin) => fetch(&bin, &workdir),
                        None => PlanUsage {
                            fetched_at_ms: now_ms(),
                            error: Some(
                                "Claude Code was not found. Set its path in Settings.".into(),
                            ),
                            ..PlanUsage::default()
                        },
                    };
                    if let Ok(mut slot) = state.plan_usage.lock() {
                        *slot = Some(usage.clone());
                    }
                    let _ = app.emit(EVENT_PLAN_USAGE, usage);
                }
                match rx.recv_timeout(config.interval) {
                    Ok(UsageMsg::Refresh) | Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        });
    if let Err(e) = spawned {
        eprintln!("could not start the usage thread: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "You are currently using your subscription to power your Claude Code usage\n\n\
        Current session: 23% used · resets Nov 7 at 5pm (Europe/London)\n\
        Current week (all models): 61% used · resets Nov 9 at 4pm (Europe/London)\n\n\
        What's contributing to your usage?\nLast 24h · 335 requests · 3 sessions\n  71%  Start fresh when you switch tasks · run /clear\n";

    #[test]
    fn reads_session_and_week_meters() {
        let meters = parse_meters(SAMPLE);
        assert_eq!(
            meters,
            vec![
                Meter {
                    label: "Current session".into(),
                    percent: 23,
                    resets: Some("Nov 7 at 5pm (Europe/London)".into()),
                },
                Meter {
                    label: "Current week (all models)".into(),
                    percent: 61,
                    resets: Some("Nov 9 at 4pm (Europe/London)".into()),
                },
            ]
        );
    }

    #[test]
    fn tolerates_missing_reset_and_unknown_text() {
        assert_eq!(
            parse_meters("Current week (Opus): 5% used"),
            vec![Meter {
                label: "Current week (Opus)".into(),
                percent: 5,
                resets: None
            }]
        );
        assert!(parse_meters("Something new and different").is_empty());
        assert_eq!(parse_meters("X: 250% used")[0].percent, 100);
    }

    #[test]
    fn custom_path_must_exist() {
        assert_eq!(find_claude(Some("/definitely/not/here/claude")), None);
    }

    #[test]
    fn checks_common_install_locations() {
        let home = Path::new("/home/u");
        let c = candidates(Some(home));
        if !cfg!(windows) {
            assert!(c.contains(&home.join(".local/bin/claude")));
            assert!(c.contains(&PathBuf::from("/opt/homebrew/bin/claude")));
        }
    }
}
