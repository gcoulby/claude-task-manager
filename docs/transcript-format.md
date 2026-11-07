# Claude Code transcript format

What is actually in `~/.claude/projects/`, worked out by structural analysis of real files on 2026-11-07 (Claude Code versions 2.1.215 to 2.1.318, VS Code extension and desktop app). This is not a public contract. Treat everything here as observed, not guaranteed, and keep the parser tolerant.

The analysis looked only at types, field names and counts, never at message text. Sample size: 16 projects, 25 session files, 32 subagent files, about 29,000 entries, 260 MB.

## Layout

```
<projects>/
  <encoded-project>/                  # cwd with / and . replaced by -, lossy
    <session-id>.jsonl                # main transcript, one per session
    <session-id>/
      subagents/
        agent-<agent-id>.jsonl        # one subagent run
        agent-<agent-id>.meta.json    # sidecar: links the run to its parent tool call
      tool-results/
        <name>.txt                    # large tool output persisted outside the transcript
        pdf-<uuid>/page-N.jpg         # rendered PDF pages
    memory/*.md                       # Claude's memory files, not transcripts
```

- `<session-id>` is a UUID and always equals the `sessionId` field inside the file.
- The project folder name is lossy (`/Users/me/my.app` and `/Users/me/my-app` collide). Read `cwd` from the entries. Use the folder name only as a display fallback.
- `cwd` can change within one session (13 of 25 sessions had more than one, from `cd` into subfolders). Use the first `cwd` seen as the session's project.
- Sessions from the Claude Code desktop app (`entrypoint: "claude-desktop"`) land in the same folder as VS Code and CLI sessions.

## Lines

- One JSON object per line, `\n` terminated. No unparseable lines were found, but a file being written can end mid-line, so only process up to the last `\n`.
- Lines can be large: up to 2.5 MB for a `user` entry holding a big tool result or image, and for `api-request-blob`.
- **File order is the only reliable order.** Timestamps go backwards 1,465 times in the sample, mostly on `assistant` entries, which are stamped when the API message started rather than when written. Store and sort by `(file, byte_offset)`. Use `timestamp` for display and durations only.
- `timestamp` format is always ISO 8601 UTC with milliseconds: `2026-11-07T09:38:01.123Z`.
- No entry `uuid` appears in more than one main file, so resumed sessions do not copy history.

## Entry types

Every line has `type`. Counts are from the sample.

| type                    | count | keep? | what it is                                                       |
| ----------------------- | ----: | ----- | ---------------------------------------------------------------- |
| `assistant`             |  8240 | yes   | One content block from a model response                          |
| `attachment`            |  4931 | no    | Context injected into the prompt (reminders, file edits, env)    |
| `user`                  |  4335 | yes   | Human prompt, tool result, or injected message                   |
| `api-request-blob`      |  3873 | no    | Raw request bodies, large                                        |
| `api-request`           |  2782 | no    | Request metadata                                                 |
| `last-prompt`           |  1898 | no    | Copy of the latest prompt text, repeated                         |
| `atis-latch`            |  1878 | no    | Internal                                                         |
| `ai-title`              |  1837 | yes   | Generated session title, repeated as it changes; last one wins   |
| `queue-operation`       |   480 | maybe | Prompts queued while the agent was busy (enqueue/dequeue/remove) |
| `session-owner`         |   320 | no    | Account and organisation UUIDs                                   |
| `file-history-delta`    |   276 | no    | Checkpoint bookkeeping                                           |
| `file-history-snapshot` |   181 | no    | Checkpoint bookkeeping                                           |
| `mode`                  |    94 | maybe | Session mode change                                              |
| `api-request-shape`     |    30 | no    | Internal                                                         |
| `cost-state`            |    25 | no    | Cost totals (cost tracking is out of scope)                      |
| `system`                |    24 | yes   | API errors, compaction boundaries, hook summaries                |
| `custom-title`          |    13 | yes   | Title the user set; beats `ai-title`                             |
| `tools-used`            |    10 | no    | Internal                                                         |

"No" means the parser skips it without storing it. That avoids about 70 MB of request blobs and keeps account identifiers out of the archive. Unknown types must be skipped, not treated as errors.

### Common fields on `user`, `assistant`, `system`

`uuid`, `parentUuid` (null at the root), `sessionId`, `timestamp`, `cwd`, `gitBranch`, `version`, `entrypoint` (`claude-vscode`, `claude-desktop`, absent for some), `isSidechain`, `userType` (always `external`), `slug`.

Subagent entries also carry `agentId`, which equals the `<agent-id>` in the file name.

### `assistant`

```json
{
  "type": "assistant",
  "uuid": "…",
  "parentUuid": "…",
  "requestId": "…",
  "message": {
    "id": "msg_…",
    "role": "assistant",
    "model": "…",
    "content": [{ "type": "tool_use", "id": "toolu_…", "name": "Bash", "input": {} }],
    "stop_reason": "tool_use",
    "usage": {}
  }
}
```

- **One content block per entry.** A single API response is split into 1 to 6 entries that share `message.id`. Group by `message.id` to show one response.
- Block types: `thinking`, `text`, `tool_use`.
- `stop_reason` values: `tool_use`, `end_turn`, `stop_sequence`, or `null` on earlier blocks of a split message. Only the last entry of a message carries the final reason.
- `isApiErrorMessage: true` with `error` and `apiErrorStatus` marks a synthetic message reporting an API failure.

### `user`

`message.content` is a string or a list of blocks (`text`, `tool_result`, `image`, `document`). Every `user` entry has a `promptId`.

Observed shapes, which decide what counts as a task:

| shape                                                             | meaning                              |
| ----------------------------------------------------------------- | ------------------------------------ |
| `tool_result` block, plus `toolUseResult`                         | Tool result                          |
| `text`/`image`/`document`, `origin.kind: "human"`                 | Human prompt, starts a task          |
| string, no `origin`, not meta                                     | Older-style or SDK prompt, or marker |
| string, `origin.kind: "task-notification"`, `<task-notification>` | Background task finished             |
| `isMeta: true`                                                    | Injected context, not a prompt       |
| `isCompactSummary: true`                                          | Summary written after compaction     |
| text starting `[Request interrupted by user]`                     | User pressed stop                    |
| text starting `<command-name>`                                    | Local slash command                  |
| text starting `<local-command-stdout>` / `<local-command-caveat>` | Slash command output, not a prompt   |

`tool_result` blocks carry `tool_use_id` and optional `is_error`. Tool results inside a turn share the turn's `promptId` (3,271 of 3,283; the misses follow interruptions).

`toolUseResult` is the structured tool output. Its shape depends on the tool, for example:

- Bash: `stdout`, `stderr`, `interrupted`, `isImage`, `noOutputExpected`, sometimes `persistedOutputPath` and `persistedOutputSize` when output went to `tool-results/`.
- Edit/Write: `filePath`, `oldString`, `newString`, `structuredPatch`, `originalFile`.
- Agent: `agentId`, `description`, `prompt`, `status`, `isAsync`, `outputFile`.
- TaskCreate/TaskUpdate: `task`, or `taskId`, `statusChange`, `updatedFields`.

It is sometimes a plain string (69 cases). Persisted outputs in `tool-results/` are separate files and are deleted with the session, so only the truncated copy in the transcript survives in our archive.

### `system`

`subtype` values seen: `api_error` (with `error.status`, `error.message`, `retryAttempt`, `maxRetries`), `compact_boundary`, `stop_hook_summary`, `informational`. `level` is `error`, `info`, `notice` or `suggestion`.

### Titles

`custom-title.customTitle` (set by the user) wins over the last `ai-title.aiTitle`. Both appear many times; take the last one in file order.

## Subagents

- Each run is `subagents/agent-<agent-id>.jsonl`. Every entry has `isSidechain: true` and `agentId`. The first entry is a `user` prompt with `parentUuid: null`.
- `agent-<agent-id>.meta.json`: `{ agentType, description, spawnDepth, toolUseId, stoppedByUser? }`. **`toolUseId` is the `tool_use` id of the parent's `Agent` call**, which makes each subagent run a child task of whichever task holds that call.
- The parent's `Agent` tool result also has `agentId`; all 32 sampled links resolved to an existing file.
- Main session files contain no sidechain entries.
- All 32 sampled runs were background agents (`isAsync: true`, status `async_launched`), so the parent's turn usually ends before the subagent does. A finished parent says nothing about the subagent.
- Background agents report back through a `user` entry in the main transcript with `origin.kind: "task-notification"`, whose text contains `<task-id><agent-id></task-id>` and `<status>completed|failed</status>`. 14 of the 32 had one. That report is the best evidence of how a background agent ended, especially when its own transcript stops without an `end_turn`, which happened 3 times.

## Task boundaries

A task is one prompt through to the end of that turn:

- **Starts** at a `user` entry in the main file that is not `isMeta`, not `isCompactSummary`, has no `tool_result` block, and is not a `<local-command-*>` or interruption marker. Human prompts and task notifications both start tasks; keep `origin.kind` so the UI can tell them apart.
- **Ends** at the next start, or at an assistant `stop_reason: "end_turn"` with nothing after it. `end_turn` alone is not final: it was followed by more `assistant` entries 118 times out of 319 (background work and stop hooks continue the turn).
- An interruption marker ends the current task as stopped by the user.

## What this means for status

- **Done:** the turn's last assistant message ended with `end_turn` and nothing followed.
- **Running:** the last entry is recent activity mid-turn.
- **Waiting:** a pending `AskUserQuestion` call is definitely waiting for the user. Any other `tool_use` without a result could be a long tool run or a permission prompt, and the transcript cannot tell these apart. Say so instead of guessing.
- **Failed:** `system` `api_error` with no later success, or an `isApiErrorMessage` assistant entry ending the turn.
- **Stale:** no new bytes for N minutes and not done.
- **Subagents:** a background agent's own transcript decides its status; if that looks unfinished, its `<task-notification>` report wins.
- Checked against the sample: 16 subagents inferred "stopped by you" matches the 16 `meta.json` files with `stoppedByUser`.

## Retention

`cleanupPeriodDays` was unset in `~/.claude/settings.json`, so the 30-day default applies. Sessions disappear with their `subagents/` and `tool-results/` folders.

## Sensitive content

Transcripts contain full tool inputs and outputs: file contents, command output, environment values. `session-owner` holds account UUIDs. Redact before display, never log raw content, and do not store skipped types.

## Fixtures

Anonymised, hand-written fixtures live in `src-tauri/tests/fixtures/`. They copy the observed shapes, with invented text, ids and paths.
