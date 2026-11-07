//! Turns one transcript line into an entry worth keeping, or skips it.
//!
//! The format is not a public contract (see docs/transcript-format.md), so
//! everything here is tolerant: unknown types and missing fields lead to a
//! skip or a generic entry, never an error.

use serde_json::{Map, Value};

use super::redact::redact;

/// Bump when classification or summaries change, so stored entries are
/// re-parsed from their raw JSON.
pub const PARSER_VERSION: i64 = 2;

/// Longest summary kept for lists and search. The full text stays in raw_json.
pub const SUMMARY_MAX_CHARS: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A prompt from the user, which starts a task.
    Prompt,
    /// A background task finished and the agent was told, which starts a task.
    Notification,
    /// A local slash command such as /clear, which starts a task.
    Command,
    /// The user pressed stop.
    Interrupt,
    /// Injected context, slash command output, or a compaction summary.
    Meta,
    Text,
    Thinking,
    ToolUse,
    ToolResult,
    ApiError,
    Compact,
    System,
    AiTitle,
    CustomTitle,
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryKind::Prompt => "prompt",
            EntryKind::Notification => "notification",
            EntryKind::Command => "command",
            EntryKind::Interrupt => "interrupt",
            EntryKind::Meta => "meta",
            EntryKind::Text => "text",
            EntryKind::Thinking => "thinking",
            EntryKind::ToolUse => "tool_use",
            EntryKind::ToolResult => "tool_result",
            EntryKind::ApiError => "api_error",
            EntryKind::Compact => "compact",
            EntryKind::System => "system",
            EntryKind::AiTitle => "ai_title",
            EntryKind::CustomTitle => "custom_title",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "prompt" => EntryKind::Prompt,
            "notification" => EntryKind::Notification,
            "command" => EntryKind::Command,
            "interrupt" => EntryKind::Interrupt,
            "meta" => EntryKind::Meta,
            "text" => EntryKind::Text,
            "thinking" => EntryKind::Thinking,
            "tool_use" => EntryKind::ToolUse,
            "tool_result" => EntryKind::ToolResult,
            "api_error" => EntryKind::ApiError,
            "compact" => EntryKind::Compact,
            "system" => EntryKind::System,
            "ai_title" => EntryKind::AiTitle,
            "custom_title" => EntryKind::CustomTitle,
            _ => return None,
        })
    }

    /// Kinds that open a new task when they appear in a transcript.
    pub fn starts_task(self) -> bool {
        matches!(
            self,
            EntryKind::Prompt | EntryKind::Notification | EntryKind::Command
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedEntry {
    pub kind: EntryKind,
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub tool_name: Option<String>,
    pub tool_use_id: Option<String>,
    pub message_id: Option<String>,
    pub stop_reason: Option<String>,
    pub is_error: bool,
    pub cwd: Option<String>,
    pub ts: Option<String>,
    /// Redacted and truncated.
    pub summary: String,
    /// The original line with embedded binary data removed.
    pub raw_json: String,
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    Entry(Box<ParsedEntry>),
    /// A known bookkeeping type or an unknown type: nothing to keep.
    Skip,
    /// Not valid JSON. Counted, then ignored.
    Malformed,
}

pub fn parse_line(line: &[u8]) -> Parsed {
    let Ok(mut value) = serde_json::from_slice::<Value>(line) else {
        return Parsed::Malformed;
    };
    let Some(obj) = value.as_object() else {
        return Parsed::Malformed;
    };
    let Some(classified) = classify(obj) else {
        return Parsed::Skip;
    };

    strip_binary(&mut value);
    strip_file_snapshots(&mut value);
    let raw_json = value.to_string();
    let obj = match value.as_object() {
        Some(o) => o,
        None => return Parsed::Malformed,
    };

    let message = obj.get("message").and_then(Value::as_object);
    Parsed::Entry(Box::new(ParsedEntry {
        kind: classified.kind,
        uuid: str_field(obj, "uuid"),
        parent_uuid: str_field(obj, "parentUuid"),
        tool_name: classified.tool_name,
        tool_use_id: classified.tool_use_id,
        message_id: message.and_then(|m| str_field(m, "id")),
        stop_reason: message.and_then(|m| str_field(m, "stop_reason")),
        is_error: classified.is_error,
        cwd: str_field(obj, "cwd"),
        ts: str_field(obj, "timestamp"),
        summary: finish_summary(&classified.summary),
        raw_json,
    }))
}

/// The complete, redacted text of a stored line, for an expanded view.
/// Tool calls show their full input as formatted JSON.
pub fn full_text(raw_json: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(raw_json) else {
        return String::new();
    };
    let Some(obj) = value.as_object() else {
        return String::new();
    };
    let Some(c) = classify(obj) else {
        return String::new();
    };
    let text = if c.kind == EntryKind::ToolUse {
        obj.get("message")
            .and_then(|m| m.get("content"))
            .and_then(Value::as_array)
            .and_then(|blocks| {
                blocks
                    .iter()
                    .find(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
            })
            .and_then(|b| b.get("input"))
            .and_then(|input| serde_json::to_string_pretty(input).ok())
            .unwrap_or(c.summary)
    } else {
        c.summary
    };
    redact(text.trim()).into_owned()
}

struct Classified {
    kind: EntryKind,
    tool_name: Option<String>,
    tool_use_id: Option<String>,
    is_error: bool,
    summary: String,
}

impl Classified {
    fn new(kind: EntryKind, summary: String) -> Self {
        Classified {
            kind,
            tool_name: None,
            tool_use_id: None,
            is_error: false,
            summary,
        }
    }
}

fn classify(obj: &Map<String, Value>) -> Option<Classified> {
    match obj.get("type")?.as_str()? {
        "user" => classify_user(obj),
        "assistant" => classify_assistant(obj),
        "system" => Some(classify_system(obj)),
        "ai-title" => Some(Classified::new(
            EntryKind::AiTitle,
            str_field(obj, "aiTitle")?,
        )),
        "custom-title" => Some(Classified::new(
            EntryKind::CustomTitle,
            str_field(obj, "customTitle")?,
        )),
        _ => None,
    }
}

fn classify_user(obj: &Map<String, Value>) -> Option<Classified> {
    let content = obj.get("message")?.get("content")?;
    let blocks: Vec<&Value> = match content {
        Value::Array(items) => items.iter().collect(),
        _ => Vec::new(),
    };

    let results: Vec<&Value> = blocks
        .iter()
        .copied()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
        .collect();
    if let Some(first) = results.first() {
        let mut c = Classified::new(
            EntryKind::ToolResult,
            results
                .iter()
                .map(|b| tool_result_text(b))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        c.tool_use_id = first
            .get("tool_use_id")
            .and_then(Value::as_str)
            .map(String::from);
        c.is_error = results
            .iter()
            .any(|b| b.get("is_error").and_then(Value::as_bool) == Some(true));
        return Some(c);
    }

    let text = match content {
        Value::String(s) => s.clone(),
        _ => prompt_text(&blocks),
    };
    let trimmed = text.trim_start();
    let origin = obj
        .get("origin")
        .and_then(|o| o.get("kind"))
        .and_then(Value::as_str);

    let kind = if bool_field(obj, "isCompactSummary") || bool_field(obj, "isMeta") {
        EntryKind::Meta
    } else if trimmed.starts_with("[Request interrupted by user") {
        EntryKind::Interrupt
    } else if origin == Some("task-notification") || trimmed.starts_with("<task-notification>") {
        EntryKind::Notification
    } else if trimmed.starts_with("<command-name>") {
        EntryKind::Command
    } else if trimmed.starts_with("<local-command-") || trimmed.starts_with("<command-message>") {
        EntryKind::Meta
    } else {
        EntryKind::Prompt
    };

    let summary = match kind {
        EntryKind::Command => command_summary(trimmed),
        _ => text,
    };
    Some(Classified::new(kind, summary))
}

fn classify_assistant(obj: &Map<String, Value>) -> Option<Classified> {
    let message = obj.get("message")?;
    let blocks: Vec<&Value> = message
        .get("content")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();

    if bool_field(obj, "isApiErrorMessage") {
        let mut c = Classified::new(EntryKind::ApiError, content_text(&blocks));
        c.is_error = true;
        return Some(c);
    }

    // Observed: one block per entry. If several arrive, a tool call matters
    // most for task state, so it decides the kind.
    if let Some(tool) = blocks
        .iter()
        .find(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
    {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("tool")
            .to_string();
        let input = tool.get("input").cloned().unwrap_or(Value::Null);
        let mut c = Classified::new(EntryKind::ToolUse, tool_use_summary(&name, &input));
        c.tool_use_id = tool.get("id").and_then(Value::as_str).map(String::from);
        c.tool_name = Some(name);
        return Some(c);
    }

    let thinking_only = !blocks.is_empty()
        && blocks
            .iter()
            .all(|b| b.get("type").and_then(Value::as_str) == Some("thinking"));
    if thinking_only {
        let text = blocks
            .iter()
            .filter_map(|b| b.get("thinking").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        return Some(Classified::new(EntryKind::Thinking, text));
    }
    Some(Classified::new(EntryKind::Text, content_text(&blocks)))
}

fn classify_system(obj: &Map<String, Value>) -> Classified {
    let subtype = obj.get("subtype").and_then(Value::as_str).unwrap_or("");
    match subtype {
        "api_error" => {
            let err = obj.get("error");
            let status = err.and_then(|e| e.get("status")).and_then(Value::as_i64);
            let message = err
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            let summary = match status {
                Some(s) => format!("API error {s}: {message}"),
                None => format!("API error: {message}"),
            };
            let mut c = Classified::new(EntryKind::ApiError, summary);
            c.is_error = true;
            c
        }
        "compact_boundary" => Classified::new(EntryKind::Compact, "Conversation compacted".into()),
        _ => {
            let content = str_field(obj, "content").unwrap_or_default();
            let summary = if content.is_empty() {
                subtype.replace('_', " ")
            } else {
                content
            };
            let mut c = Classified::new(EntryKind::System, summary);
            c.is_error = obj.get("level").and_then(Value::as_str) == Some("error");
            c
        }
    }
}

/// A one-line description of a tool call, using whichever input field says
/// the most for that tool.
fn tool_use_summary(name: &str, input: &Value) -> String {
    let field = |k: &str| input.get(k).and_then(Value::as_str);
    let detail = match name {
        "Bash" => field("command"),
        "Read" | "Write" | "Edit" | "NotebookEdit" => field("file_path").or(field("notebook_path")),
        "Agent" | "Task" => field("description"),
        "Grep" | "Glob" => field("pattern"),
        "WebFetch" => field("url"),
        "WebSearch" => field("query"),
        "Skill" => field("skill"),
        _ => None,
    };
    match detail {
        Some(d) => format!("{name}: {d}"),
        None => {
            let compact = input.to_string();
            if compact == "{}" || compact == "null" {
                name.to_string()
            } else {
                format!("{name}: {}", truncate(&compact, 300))
            }
        }
    }
}

fn tool_result_text(block: &Value) -> String {
    match block.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => content_text(&items.iter().collect::<Vec<_>>()),
        _ => String::new(),
    }
}

/// IDE extensions send context (the open file, the selection) as separate
/// text blocks wrapped whole in one `<ide_…>` tag. They are not the user's
/// words, so they are left out of the summary; the raw JSON keeps them.
fn is_ide_context(block: &Value) -> bool {
    let Some(text) = block.get("text").and_then(Value::as_str) else {
        return false;
    };
    let text = text.trim();
    let Some(rest) = text.strip_prefix("<ide_") else {
        return false;
    };
    let Some(tag_end) = rest.find('>') else {
        return false;
    };
    let tag = &rest[..tag_end];
    tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && text.ends_with(&format!("</ide_{tag}>"))
}

fn prompt_text(blocks: &[&Value]) -> String {
    let own: Vec<&Value> = blocks
        .iter()
        .copied()
        .filter(|b| !is_ide_context(b))
        .collect();
    // A prompt that is nothing but context still needs a summary.
    if own.is_empty() {
        content_text(blocks)
    } else {
        content_text(&own)
    }
}

fn content_text(blocks: &[&Value]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b.get("type").and_then(Value::as_str) {
            Some("text") => b.get("text").and_then(Value::as_str).map(String::from),
            Some("image") => Some("[image]".into()),
            Some("document") => Some("[document]".into()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// "<command-name>/clear</command-name>…" becomes "/clear …args".
fn command_summary(text: &str) -> String {
    let between = |tag: &str| {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        let start = text.find(&open)? + open.len();
        let end = text[start..].find(&close)? + start;
        Some(text[start..end].trim().to_string())
    };
    let name = between("command-name").unwrap_or_else(|| "command".into());
    match between("command-args").filter(|a| !a.is_empty()) {
        Some(args) => format!("{name} {args}"),
        None => name,
    }
}

fn finish_summary(text: &str) -> String {
    truncate(&redact(text.trim()), SUMMARY_MAX_CHARS)
}

fn truncate(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => format!("{}…", &text[..idx]),
        None => text.to_string(),
    }
}

/// Replaces base64 image and document data with a size note. These blobs are
/// most of the bytes in a transcript and are never shown as text.
fn strip_binary(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Message blocks use {"type":"base64","data":…}; Read tool
            // results use {"base64":…}.
            let is_base64 = map.get("type").and_then(Value::as_str) == Some("base64");
            if is_base64 && let Some(Value::String(data)) = map.get_mut("data") {
                *data = format!("[{} bytes omitted]", data.len());
            }
            if let Some(Value::String(data)) = map.get_mut("base64") {
                *data = format!("[{} bytes omitted]", data.len());
            }
            for v in map.values_mut() {
                strip_binary(v);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(strip_binary),
        _ => {}
    }
}

/// Edit and Write results carry the whole file as it was before the change.
/// That is not part of the log, repeats on every edit and is the likeliest
/// place for sensitive file contents, so only its size is kept.
fn strip_file_snapshots(value: &mut Value) {
    if let Some(Value::String(original)) = value
        .get_mut("toolUseResult")
        .and_then(|r| r.get_mut("originalFile"))
    {
        *original = format!("[{} bytes omitted]", original.len());
    }
}

fn str_field(obj: &Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key).and_then(Value::as_str).map(String::from)
}

fn bool_field(obj: &Map<String, Value>, key: &str) -> bool {
    obj.get(key).and_then(Value::as_bool).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(v: Value) -> ParsedEntry {
        match parse_line(v.to_string().as_bytes()) {
            Parsed::Entry(e) => *e,
            other => panic!("expected entry, got {other:?}"),
        }
    }

    fn user(content: Value, extra: Value) -> Value {
        let mut v = json!({"type": "user", "uuid": "u1", "timestamp": "2026-11-01T10:00:00.000Z",
                           "message": {"role": "user", "content": content}});
        if let (Some(m), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            m.extend(e.clone());
        }
        v
    }

    #[test]
    fn malformed_and_partial_lines_are_reported_not_fatal() {
        assert_eq!(parse_line(b"{\"type\":\"user\",\"mess"), Parsed::Malformed);
        assert_eq!(parse_line(b"not json"), Parsed::Malformed);
        assert_eq!(parse_line(b"[1,2]"), Parsed::Malformed);
    }

    #[test]
    fn unknown_and_bookkeeping_types_are_skipped() {
        for t in [
            "some-future-type",
            "api-request-blob",
            "session-owner",
            "attachment",
            "last-prompt",
        ] {
            assert_eq!(
                parse_line(json!({"type": t}).to_string().as_bytes()),
                Parsed::Skip
            );
        }
        assert_eq!(parse_line(br#"{"no_type":true}"#), Parsed::Skip);
    }

    #[test]
    fn known_type_with_unexpected_shape_is_skipped() {
        assert_eq!(
            parse_line(br#"{"type":"user","message":"odd"}"#),
            Parsed::Skip
        );
        assert_eq!(parse_line(br#"{"type":"ai-title"}"#), Parsed::Skip);
    }

    #[test]
    fn classifies_user_entries() {
        let cases = [
            (
                user(
                    json!([{"type":"text","text":"Add a toggle"}]),
                    json!({"origin":{"kind":"human"}}),
                ),
                EntryKind::Prompt,
            ),
            (user(json!("plain prompt"), json!({})), EntryKind::Prompt),
            (
                user(
                    json!([{"type":"text","text":"[Request interrupted by user]"}]),
                    json!({}),
                ),
                EntryKind::Interrupt,
            ),
            (
                user(
                    json!("[Request interrupted by user for tool use]"),
                    json!({}),
                ),
                EntryKind::Interrupt,
            ),
            (
                user(
                    json!("<task-notification>\n</task-notification>"),
                    json!({"origin":{"kind":"task-notification"}}),
                ),
                EntryKind::Notification,
            ),
            (
                user(json!("<command-name>/clear</command-name>"), json!({})),
                EntryKind::Command,
            ),
            (
                user(
                    json!("<local-command-stdout></local-command-stdout>"),
                    json!({}),
                ),
                EntryKind::Meta,
            ),
            (
                user(json!("context"), json!({"isMeta": true})),
                EntryKind::Meta,
            ),
            (
                user(json!("summary"), json!({"isCompactSummary": true})),
                EntryKind::Meta,
            ),
            (
                user(
                    json!([{"type":"tool_result","tool_use_id":"t1","content":"ok"}]),
                    json!({}),
                ),
                EntryKind::ToolResult,
            ),
        ];
        for (v, kind) in cases {
            assert_eq!(entry(v.clone()).kind, kind, "{v}");
        }
    }

    #[test]
    fn tool_results_carry_id_error_and_text() {
        let e = entry(user(
            json!([{"type":"tool_result","tool_use_id":"t9","is_error":true,
                    "content":[{"type":"text","text":"1 test failed"},{"type":"image","source":{}}]}]),
            json!({}),
        ));
        assert_eq!(e.tool_use_id.as_deref(), Some("t9"));
        assert!(e.is_error);
        assert_eq!(e.summary, "1 test failed\n[image]");
    }

    #[test]
    fn assistant_blocks_map_to_kinds_with_message_fields() {
        let a = |block: Value, stop: Value| {
            json!({"type":"assistant","uuid":"a1","message":{"id":"msg_1","role":"assistant",
                   "content":[block],"stop_reason":stop}})
        };
        let t = entry(a(json!({"type":"thinking","thinking":"hmm"}), Value::Null));
        assert_eq!(t.kind, EntryKind::Thinking);
        assert_eq!(t.message_id.as_deref(), Some("msg_1"));
        assert_eq!(t.stop_reason, None);

        let x = entry(a(json!({"type":"text","text":"Done."}), json!("end_turn")));
        assert_eq!(x.kind, EntryKind::Text);
        assert_eq!(x.stop_reason.as_deref(), Some("end_turn"));

        let u = entry(a(
            json!({"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"pnpm test","description":"Run tests"}}),
            json!("tool_use"),
        ));
        assert_eq!(u.kind, EntryKind::ToolUse);
        assert_eq!(u.tool_name.as_deref(), Some("Bash"));
        assert_eq!(u.tool_use_id.as_deref(), Some("toolu_1"));
        assert_eq!(u.summary, "Bash: pnpm test");
    }

    #[test]
    fn api_errors_from_assistant_and_system() {
        let a = entry(json!({"type":"assistant","isApiErrorMessage":true,
            "message":{"id":"m","content":[{"type":"text","text":"API Error: overloaded"}],"stop_reason":"stop_sequence"}}));
        assert_eq!(a.kind, EntryKind::ApiError);
        assert!(a.is_error);

        let s = entry(
            json!({"type":"system","subtype":"api_error","level":"error",
            "error":{"status":529,"message":"Overloaded"}}),
        );
        assert_eq!(s.kind, EntryKind::ApiError);
        assert_eq!(s.summary, "API error 529: Overloaded");

        let c = entry(json!({"type":"system","subtype":"compact_boundary"}));
        assert_eq!(c.kind, EntryKind::Compact);
        let h = entry(json!({"type":"system","subtype":"stop_hook_summary","level":"suggestion"}));
        assert_eq!(
            (h.kind, h.summary.as_str()),
            (EntryKind::System, "stop hook summary")
        );
    }

    #[test]
    fn titles_are_kept() {
        assert_eq!(
            entry(json!({"type":"ai-title","aiTitle":"Dark mode"})).kind,
            EntryKind::AiTitle
        );
        let c = entry(json!({"type":"custom-title","customTitle":"Mine"}));
        assert_eq!(
            (c.kind, c.summary.as_str()),
            (EntryKind::CustomTitle, "Mine")
        );
    }

    #[test]
    fn command_summary_extracts_name_and_args() {
        let e = entry(user(
            json!(
                "<command-name>/model</command-name>\n<command-message>model</command-message>\n<command-args>opus</command-args>"
            ),
            json!({}),
        ));
        assert_eq!(e.summary, "/model opus");
    }

    #[test]
    fn summaries_are_redacted_and_truncated() {
        let secret = format!("sk-ant-api03-{}", "x1Y2".repeat(10));
        let e = entry(user(json!(format!("use {secret} please")), json!({})));
        assert!(!e.summary.contains(&secret));

        let long = "é".repeat(SUMMARY_MAX_CHARS + 50);
        let e = entry(user(json!(long), json!({})));
        assert_eq!(e.summary.chars().count(), SUMMARY_MAX_CHARS + 1);
        assert!(e.summary.ends_with('…'));
    }

    #[test]
    fn base64_payloads_are_removed_from_raw_json() {
        let data = "QUJD".repeat(1000);
        let e = entry(user(
            json!([{"type":"image","source":{"type":"base64","media_type":"image/png","data":data}},
                   {"type":"text","text":"see image"}]),
            json!({"origin":{"kind":"human"}}),
        ));
        assert!(!e.raw_json.contains(&data));
        assert!(e.raw_json.contains("[4000 bytes omitted]"));
        assert_eq!(e.summary, "[image]\nsee image");

        let r = entry(user(
            json!([{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"image"}]}]),
            json!({"toolUseResult":{"type":"image","file":{"type":"image/png","base64":data}}}),
        ));
        assert!(!r.raw_json.contains(&data));
    }

    #[test]
    fn original_file_snapshots_are_removed_from_raw_json() {
        let body = "line\n".repeat(500);
        let e = entry(user(
            json!([{"type":"tool_result","tool_use_id":"t1","content":"updated"}]),
            json!({"toolUseResult":{"filePath":"/a.ts","oldString":"x","newString":"y","originalFile":body}}),
        ));
        assert!(!e.raw_json.contains("line\\nline"));
        assert!(e.raw_json.contains("[2500 bytes omitted]"));
        assert!(e.raw_json.contains(r#""newString":"y""#));
    }

    #[test]
    fn full_text_is_complete_and_redacted() {
        let long = "y".repeat(SUMMARY_MAX_CHARS * 2);
        let secret = format!("ghp_{}", "a1B2".repeat(10));
        let line = user(json!(format!("{long} {secret}")), json!({})).to_string();
        let text = full_text(&line);
        assert!(text.starts_with(&long));
        assert!(!text.contains(&secret));

        let call = json!({"type":"assistant","message":{"id":"m","content":[
            {"type":"tool_use","id":"t","name":"Edit","input":{"file_path":"/a.ts","old_string":"x"}}]}});
        let text = full_text(&call.to_string());
        assert!(text.contains("\"file_path\": \"/a.ts\""));
        assert_eq!(full_text("not json"), "");
    }

    #[test]
    fn ide_context_blocks_are_left_out_of_prompt_summaries() {
        let e = entry(user(
            json!([
                {"type":"text","text":"<ide_opened_file>The user opened the file /a.ts in the IDE.</ide_opened_file>"},
                {"type":"text","text":"Fix the failing test."}
            ]),
            json!({"origin":{"kind":"human"}}),
        ));
        assert_eq!(e.kind, EntryKind::Prompt);
        assert_eq!(e.summary, "Fix the failing test.");
        assert!(e.raw_json.contains("ide_opened_file"));

        let only = entry(user(
            json!([{"type":"text","text":"<ide_selection>x</ide_selection>"}]),
            json!({}),
        ));
        assert_eq!(only.summary, "<ide_selection>x</ide_selection>");

        // A prompt that merely starts with a tag-like word is the user's text.
        let own = entry(user(
            json!([{"type":"text","text":"<ide_x> is a tag I made"}]),
            json!({}),
        ));
        assert_eq!(own.summary, "<ide_x> is a tag I made");
    }

    #[test]
    fn kind_names_round_trip() {
        for k in [
            EntryKind::Prompt,
            EntryKind::ToolUse,
            EntryKind::CustomTitle,
            EntryKind::Compact,
        ] {
            assert_eq!(EntryKind::parse(k.as_str()), Some(k));
        }
    }
}
