//! Status inference. Collects facts about a task while entries are walked in
//! file order; the rules that turn facts into a status come in build step 5.

use super::parser::EntryKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Unknown,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inferred {
    pub status: Status,
    pub reason: String,
}

impl Inferred {
    pub fn is_finished(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskFacts {
    /// Tool calls with no result yet, oldest first: (tool_use_id, tool name).
    pub open_tool_uses: Vec<(String, String)>,
    /// Stop reason of the latest assistant entry that had one.
    pub last_stop_reason: Option<String>,
    pub last_kind: Option<EntryKind>,
    /// The user pressed stop after the last assistant output.
    pub interrupted: bool,
    /// An API error with no successful assistant output after it.
    pub api_error: bool,
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
    ) {
        match kind {
            EntryKind::ToolUse => {
                if let Some(id) = tool_use_id {
                    self.open_tool_uses
                        .push((id.to_string(), tool_name.unwrap_or("tool").to_string()));
                }
            }
            EntryKind::ToolResult => {
                if let Some(id) = tool_use_id {
                    self.open_tool_uses.retain(|(open, _)| open != id);
                }
            }
            EntryKind::Interrupt => self.interrupted = true,
            EntryKind::ApiError => self.api_error = true,
            _ => {}
        }
        if matches!(
            kind,
            EntryKind::Text | EntryKind::Thinking | EntryKind::ToolUse
        ) {
            self.api_error = false;
            self.interrupted = false;
            if let Some(r) = stop_reason {
                self.last_stop_reason = Some(r.to_string());
            }
        }
        if !matches!(kind, EntryKind::Meta | EntryKind::System) {
            self.last_kind = Some(kind);
        }
    }
}

pub fn infer_task(_facts: &TaskFacts) -> Inferred {
    placeholder()
}

pub fn infer_session(_latest: Option<&TaskFacts>) -> Inferred {
    placeholder()
}

fn placeholder() -> Inferred {
    Inferred {
        status: Status::Unknown,
        reason: "Status inference is not built yet.".into(),
    }
}
