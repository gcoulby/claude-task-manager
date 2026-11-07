//! Status inference. There are no hooks, so status is a heuristic over what
//! the transcript shows. Every status carries the reason it was chosen, and
//! where the transcript cannot tell two states apart the reason says so.
//!
//! Inference happens in two stages:
//! 1. At ingest, `infer_task` turns the facts collected while walking a
//!    task's entries into running, waiting, done or failed.
//! 2. At read time, `with_staleness` turns running or waiting into stale if
//!    the transcript has been quiet for too long, since that depends on now.

use serde::Serialize;

use super::parser::EntryKind;

/// Bump when the rules below change, so stored statuses are recomputed.
pub const DERIVE_VERSION: i64 = 1;

pub const DEFAULT_STALE_MINUTES: i64 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Running,
    Waiting,
    Done,
    Failed,
    Stale,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Running => "running",
            Status::Waiting => "waiting",
            Status::Done => "done",
            Status::Failed => "failed",
            Status::Stale => "stale",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "running" => Status::Running,
            "waiting" => Status::Waiting,
            "done" => Status::Done,
            "failed" => Status::Failed,
            "stale" => Status::Stale,
            _ => return None,
        })
    }

    pub fn is_finished(self) -> bool {
        matches!(self, Status::Done | Status::Failed)
    }

    pub fn is_active(self) -> bool {
        matches!(self, Status::Running | Status::Waiting)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inferred {
    pub status: Status,
    pub reason: String,
}

impl Inferred {
    fn new(status: Status, reason: impl Into<String>) -> Self {
        Inferred {
            status,
            reason: reason.into(),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.status.is_finished()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskFacts {
    /// Tool calls with no result yet, oldest first: (tool_use_id, tool name).
    pub open_tool_uses: Vec<(String, String)>,
    /// Stop reason of the latest assistant entry that had one.
    pub last_stop_reason: Option<String>,
    /// Latest entry kind, ignoring injected context and system notes.
    pub last_kind: Option<EntryKind>,
    /// The entry kind that started the task.
    pub trigger: Option<EntryKind>,
    /// Any model output at all.
    pub has_assistant: bool,
    /// The user pressed stop after the last assistant output.
    pub interrupted: bool,
    /// Summary of an API error with no successful assistant output after it.
    pub api_error: Option<String>,
    /// A later prompt started, so this task cannot still be running.
    pub superseded: bool,
}

impl TaskFacts {
    pub fn observe(
        &mut self,
        kind: EntryKind,
        tool_use_id: Option<&str>,
        tool_name: Option<&str>,
        stop_reason: Option<&str>,
        summary: &str,
    ) {
        if self.trigger.is_none() {
            self.trigger = Some(kind);
        }
        match kind {
            EntryKind::ToolUse => {
                if let Some(id) = tool_use_id {
                    self.open_tool_uses
                        .push((id.to_string(), tool_name.unwrap_or("a tool").to_string()));
                }
            }
            EntryKind::ToolResult => {
                if let Some(id) = tool_use_id {
                    self.open_tool_uses.retain(|(open, _)| open != id);
                }
            }
            EntryKind::Interrupt => self.interrupted = true,
            EntryKind::ApiError => {
                self.api_error = Some(if summary.is_empty() {
                    "API error".to_string()
                } else {
                    summary.to_string()
                })
            }
            _ => {}
        }
        if matches!(
            kind,
            EntryKind::Text | EntryKind::Thinking | EntryKind::ToolUse
        ) {
            self.has_assistant = true;
            self.api_error = None;
            self.interrupted = false;
            if let Some(r) = stop_reason {
                self.last_stop_reason = Some(r.to_string());
            }
        }
        if !matches!(
            kind,
            EntryKind::Meta | EntryKind::System | EntryKind::Compact
        ) {
            self.last_kind = Some(kind);
        }
    }
}

/// Tools whose pending call means the agent is blocked on the user, not
/// working.
fn waiting_reason(tool: &str) -> Option<&'static str> {
    match tool {
        "AskUserQuestion" => Some("Asked you a question and is waiting for an answer."),
        "ExitPlanMode" => Some("Waiting for you to approve a plan."),
        _ => None,
    }
}

pub fn infer_task(f: &TaskFacts) -> Inferred {
    if let Some(err) = &f.api_error {
        return Inferred::new(Status::Failed, format!("Stopped on an error: {err}"));
    }
    if f.interrupted {
        return Inferred::new(Status::Done, "Stopped by you.");
    }

    if let Some((_, tool)) = f.open_tool_uses.last() {
        if f.superseded {
            return Inferred::new(
                Status::Done,
                format!("Ended when the next prompt started. The {tool} call never got a result."),
            );
        }
        if let Some(reason) = waiting_reason(tool) {
            return Inferred::new(Status::Waiting, reason);
        }
        return Inferred::new(
            Status::Running,
            format!(
                "Running {tool}. The transcript cannot show whether it is still working or \
                 waiting for your permission."
            ),
        );
    }

    let ended_turn = f.last_stop_reason.as_deref() == Some("end_turn")
        && matches!(f.last_kind, Some(EntryKind::Text | EntryKind::Thinking));
    if ended_turn {
        return Inferred::new(Status::Done, "Finished its turn.");
    }
    if f.superseded {
        return Inferred::new(Status::Done, "Ended when the next prompt started.");
    }
    if f.trigger == Some(EntryKind::Command) && !f.has_assistant {
        return Inferred::new(Status::Done, "Ran a local command.");
    }
    match f.last_kind {
        Some(EntryKind::ToolResult) => Inferred::new(
            Status::Running,
            "Got a tool result and is working on the next step.",
        ),
        Some(k) if k.starts_task() => {
            Inferred::new(Status::Running, "Prompt sent. No response yet.")
        }
        _ => Inferred::new(Status::Running, "Writing a response."),
    }
}

/// Background subagents report back to the main agent with a
/// `<task-notification>` naming their id and a status. When the subagent's
/// own transcript looks unfinished, that report is the better evidence.
pub fn from_notification(own: &Inferred, reported: Option<&str>) -> Option<Inferred> {
    if !own.status.is_active() {
        return None;
    }
    Some(match reported? {
        "completed" => Inferred::new(Status::Done, "Reported back to the main agent as finished."),
        "failed" => Inferred::new(Status::Failed, "Reported back to the main agent as failed."),
        other => Inferred::new(
            Status::Done,
            format!("Reported back to the main agent as {other}."),
        ),
    })
}

/// Pulls (task id, status) out of a `<task-notification>` message.
pub fn parse_notification(text: &str) -> Option<(String, String)> {
    let tag = |name: &str| {
        let open = format!("<{name}>");
        let close = format!("</{name}>");
        let start = text.find(&open)? + open.len();
        let end = text[start..].find(&close)? + start;
        Some(text[start..end].trim().to_string())
    };
    Some((tag("task-id")?, tag("status")?))
}

/// The session takes the most pressing status among its tasks: something
/// waiting on you, then something running, then the latest task's outcome.
pub fn infer_session(latest: Option<&Inferred>, others: &[(&str, &Inferred)]) -> Inferred {
    if let Some((title, t)) = others.iter().find(|(_, t)| t.status == Status::Waiting) {
        return Inferred::new(Status::Waiting, format!("{title}: {}", t.reason));
    }
    if let Some(l) = latest
        && l.status.is_active()
    {
        return l.clone();
    }
    if let Some((title, t)) = others.iter().find(|(_, t)| t.status == Status::Running) {
        return Inferred::new(Status::Running, format!("{title}: {}", t.reason));
    }
    match latest {
        Some(l) => l.clone(),
        None => Inferred::new(Status::Done, "No prompts in this session."),
    }
}

/// Read-time check: running or waiting with no writes to the transcript for
/// `stale_after_ms` becomes stale. `activity_ms` is the file's last change.
pub fn with_staleness(
    status: Status,
    reason: &str,
    activity_ms: i64,
    now_ms: i64,
    stale_after_ms: i64,
) -> (Status, String) {
    let quiet_ms = now_ms.saturating_sub(activity_ms);
    if !status.is_active() || activity_ms <= 0 || quiet_ms < stale_after_ms {
        return (status, reason.to_string());
    }
    let before = match status {
        Status::Waiting => "it was waiting",
        _ => "it was running",
    };
    (
        Status::Stale,
        format!(
            "No activity for {}. Before that {before}: {reason}",
            describe_duration(quiet_ms)
        ),
    )
}

fn describe_duration(ms: i64) -> String {
    let minutes = ms / 60_000;
    if minutes < 60 {
        return format!("{minutes} min");
    }
    let hours = minutes / 60;
    if hours < 48 {
        return format!("{hours} h");
    }
    format!("{} days", hours / 24)
}

#[cfg(test)]
mod tests {
    use super::*;
    use EntryKind::*;

    /// (kind, tool_use_id, tool name, stop_reason)
    type Step<'a> = (EntryKind, Option<&'a str>, Option<&'a str>, Option<&'a str>);

    /// Feeds a sequence of steps into facts.
    fn facts(steps: &[Step]) -> TaskFacts {
        let mut f = TaskFacts::default();
        for (kind, id, tool, stop) in steps {
            f.observe(*kind, *id, *tool, *stop, "API error 529: Overloaded");
        }
        f
    }

    fn status(steps: &[Step]) -> Inferred {
        infer_task(&facts(steps))
    }

    const PROMPT: Step = (Prompt, None, None, None);

    #[test]
    fn finished_turn_is_done() {
        let s = status(&[
            PROMPT,
            (Thinking, None, None, None),
            (Text, None, None, Some("end_turn")),
        ]);
        assert_eq!(s.status, Status::Done);
        assert_eq!(s.reason, "Finished its turn.");
    }

    #[test]
    fn pending_tool_is_running_and_admits_uncertainty() {
        let s = status(&[
            PROMPT,
            (ToolUse, Some("t1"), Some("Bash"), Some("tool_use")),
        ]);
        assert_eq!(s.status, Status::Running);
        assert!(s.reason.contains("Bash"));
        assert!(s.reason.contains("cannot show whether"));
    }

    #[test]
    fn pending_question_or_plan_is_waiting() {
        for tool in ["AskUserQuestion", "ExitPlanMode"] {
            let s = status(&[PROMPT, (ToolUse, Some("t1"), Some(tool), Some("tool_use"))]);
            assert_eq!(s.status, Status::Waiting, "{tool}");
        }
    }

    #[test]
    fn answered_tool_then_more_work_is_running() {
        let s = status(&[
            PROMPT,
            (ToolUse, Some("t1"), Some("Bash"), Some("tool_use")),
            (ToolResult, Some("t1"), None, None),
        ]);
        assert_eq!(s.status, Status::Running);
        assert!(s.reason.contains("tool result"));
    }

    #[test]
    fn end_turn_followed_by_more_work_is_not_done() {
        // Observed in real files: end_turn, then more assistant output.
        let s = status(&[
            PROMPT,
            (Text, None, None, Some("end_turn")),
            (ToolUse, Some("t2"), Some("Read"), Some("tool_use")),
        ]);
        assert_eq!(s.status, Status::Running);
    }

    #[test]
    fn api_error_fails_unless_recovered() {
        let failed = status(&[PROMPT, (ApiError, None, None, None)]);
        assert_eq!(failed.status, Status::Failed);
        assert!(failed.reason.contains("529"));

        let recovered = status(&[
            PROMPT,
            (ApiError, None, None, None),
            (Text, None, None, Some("end_turn")),
        ]);
        assert_eq!(recovered.status, Status::Done);
    }

    #[test]
    fn interruption_is_done_by_user() {
        let s = status(&[
            PROMPT,
            (ToolUse, Some("t1"), Some("Bash"), Some("tool_use")),
            (ToolResult, Some("t1"), None, None),
            (Interrupt, None, None, None),
        ]);
        assert_eq!(
            (s.status, s.reason.as_str()),
            (Status::Done, "Stopped by you.")
        );
    }

    #[test]
    fn superseded_task_is_done_even_with_open_tool() {
        let mut f = facts(&[
            PROMPT,
            (ToolUse, Some("t1"), Some("Bash"), Some("tool_use")),
        ]);
        f.superseded = true;
        let s = infer_task(&f);
        assert_eq!(s.status, Status::Done);
        assert!(s.reason.contains("never got a result"));
    }

    #[test]
    fn prompt_without_response_is_running() {
        let s = status(&[PROMPT]);
        assert_eq!(s.status, Status::Running);
        assert_eq!(s.reason, "Prompt sent. No response yet.");
    }

    #[test]
    fn local_command_without_response_is_done() {
        let s = status(&[(Command, None, None, None), (Meta, None, None, None)]);
        assert_eq!(s.status, Status::Done);
    }

    #[test]
    fn system_notes_after_end_turn_do_not_reopen_the_task() {
        let s = status(&[
            PROMPT,
            (Text, None, None, Some("end_turn")),
            (System, None, None, None),
            (Meta, None, None, None),
        ]);
        assert_eq!(s.status, Status::Done);
    }

    #[test]
    fn background_subagent_takes_its_reported_outcome_only_if_unfinished() {
        let running = Inferred::new(Status::Running, "x");
        let done = Inferred::new(Status::Done, "Stopped by you.");
        assert_eq!(
            from_notification(&running, Some("completed")).map(|s| s.status),
            Some(Status::Done)
        );
        assert_eq!(
            from_notification(&running, Some("failed")).map(|s| s.status),
            Some(Status::Failed)
        );
        assert_eq!(from_notification(&running, None), None);
        assert_eq!(from_notification(&done, Some("failed")), None);
    }

    #[test]
    fn parses_notification_tags() {
        let text = "<task-notification>\n<task-id>a1b2</task-id>\n<status>completed</status>\n<summary>x</summary>";
        assert_eq!(
            parse_notification(text),
            Some(("a1b2".to_string(), "completed".to_string()))
        );
        assert_eq!(parse_notification("<task-notification>"), None);
    }

    #[test]
    fn session_prefers_waiting_then_running_then_latest() {
        let done = Inferred::new(Status::Done, "Finished its turn.");
        let running = Inferred::new(Status::Running, "busy");
        let waiting = Inferred::new(Status::Waiting, "question");

        assert_eq!(infer_session(Some(&done), &[]).status, Status::Done);
        let s = infer_session(Some(&done), &[("Review", &running)]);
        assert_eq!(
            (s.status, s.reason.as_str()),
            (Status::Running, "Review: busy")
        );
        assert_eq!(
            infer_session(Some(&running), &[("Ask", &waiting)]).status,
            Status::Waiting
        );
        assert_eq!(infer_session(None, &[]).status, Status::Done);
    }

    #[test]
    fn staleness_applies_only_to_active_quiet_tasks() {
        let min = 60_000;
        let (s, r) = with_staleness(Status::Running, "Running Bash.", 1, 20 * min, 15 * min);
        assert_eq!(s, Status::Stale);
        assert!(r.starts_with("No activity for 19 min. Before that it was running: Running Bash."));

        assert_eq!(
            with_staleness(Status::Running, "r", 10 * min, 20 * min, 15 * min).0,
            Status::Running
        );
        assert_eq!(
            with_staleness(Status::Done, "r", 1, 100 * min, 15 * min).0,
            Status::Done
        );
        assert_eq!(
            with_staleness(Status::Waiting, "r", 1, 100 * min, 15 * min).0,
            Status::Stale
        );
        // Unknown activity time: do not guess.
        assert_eq!(
            with_staleness(Status::Running, "r", 0, 100 * min, 15 * min).0,
            Status::Running
        );
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(describe_duration(5 * 60_000), "5 min");
        assert_eq!(describe_duration(3 * 3_600_000), "3 h");
        assert_eq!(describe_duration(72 * 3_600_000), "3 days");
    }

    #[test]
    fn status_names_round_trip() {
        for s in [
            Status::Running,
            Status::Waiting,
            Status::Done,
            Status::Failed,
            Status::Stale,
        ] {
            assert_eq!(Status::parse(s.as_str()), Some(s));
        }
    }
}
