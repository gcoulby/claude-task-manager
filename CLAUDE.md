# CLAUDE.md

## What this is

A desktop app (Tauri v2, macOS and Windows) that shows what my Claude Code agents are doing: tasks, status, logs and timings. It works by reading the local Claude Code session folder. No hooks, no server.

The problem it solves: I run several agents and sessions and have no single place to see what is running, what finished, what is stuck and what happened. Claude Code also deletes old session files after 30 days by default, so history disappears.

Single user, local only, no accounts. The only network call is the update check (see "Auto-update").

Read-only in v1. Launching or controlling agents is out of scope.

## Stack

- **Shell:** Tauri v2 (Rust backend)
- **Frontend:** React, TypeScript (strict), Tailwind CSS, shadcn/ui
- **State:** Zustand for UI state, TanStack Query for anything that comes from Rust commands
- **Database:** SQLite via `rusqlite` (with the `bundled` feature), FTS5 for log search
- **Watching and walking:** `notify` (debounced) and `walkdir`
- **Paths:** `dirs` or Tauri's path API
- **Package manager:** pnpm
- **UI extras:** `react-resizable-panels` for the panes, `cmdk` for the command palette
- **Tooling:** `prettier`, `eslint` (flat config), `cargo fmt`, `cargo clippy`
- **Toolchain:** Node 20+, pnpm, a current stable Rust. Pin versions through the lockfiles. A newer Tauri release can require a newer Rust than is installed, so check before bumping, and record the minimum Rust version in the README.

Do not add Electron, a Node backend, or an ORM. Keep the Rust side thin and boring.

## Requirements

### Must have (v1)

1. **Session discovery.** Find and watch the Claude Code session folder (`~/.claude/projects/` on macOS, the equivalent under the user profile on Windows). Location is overridable in settings.
2. **Task list.** Every task across every session, with status, project, start time, duration and last activity. Filter by status and project.
3. **Task detail.** Timeline of entries plus readable logs. Tool calls show truncated output, expandable.
4. **Live updates.** The UI updates as files change, with no manual refresh.
5. **Own archive.** Everything parsed is copied into SQLite so history survives Claude Code deleting its files.
6. **Auto-update.** See below.

### Nice to have (in this order)

1. Full-text search over logs.
2. Session overview with per-session task rollups.
3. TodoWrite items shown as the session's task list, if they are present in the transcripts.
4. Export a task's log to Markdown.

### Out of scope unless asked

Launching, stopping or controlling agents, editing anything under `~/.claude/`, remote or multi-machine views, notifications, cost tracking. Do not build these speculatively.

## Architecture

Repo root:

```
.github/workflows/   # release workflow
public/
scripts/             # version bump script and similar
src/                 # frontend
src-tauri/           # Rust backend
.gitignore
.prettierrc
.prettierignore
CLAUDE.md
README.md
LICENSE
app-icon.svg
eslint.config.js
index.html
package.json
pnpm-lock.yaml
tsconfig.json        # plus tsconfig.app.json and tsconfig.node.json
vite.config.ts
```

App structure:

```
src-tauri/
  src/
    main.rs
    commands/        # thin Tauri command handlers, one file per area
      sessions.rs
      tasks.rs
      settings.rs
      updater.rs
    db/
      mod.rs         # connection, migrations, backup
      schema.sql
      queries.rs
    ingest/
      scanner.rs     # walks the session folder, parses new bytes
      watcher.rs     # notify-based incremental updates
      parser.rs      # transcript line parsing, tolerant of unknown types
      status.rs      # status inference with reasons
      redact.rs      # secret redaction
src/
  components/
    tasks/           # list, filters, detail, timeline
    layout/
    settings/        # includes About and updates
    ui/              # shadcn components
  features/
  lib/               # tauri invoke wrappers, utils
  store/
  types/             # TS types mirroring Rust structs
```

Rust owns the filesystem, database and parsing. The frontend never touches files and never guesses paths. All data crosses the boundary through typed commands and events. Do not enable the Tauri fs plugin for the webview.

## Data source

Session files: `<projects>/<encoded-project>/<session-id>.jsonl`, plus a `subagents/` area per session for subagent runs. Find the home directory with the OS API, never a hard-coded path.

Be careful about:

- **The format is not a public contract.** First job is to inspect real files and write what is actually there to `docs/transcript-format.md`, then build anonymised fixtures. The parser must skip unknown entry types instead of failing.
- **Files are written while we read.** The last line may be partial. Only process complete lines and resume from a stored byte offset.
- **Windows locking.** Open read-only with shared access so Claude Code is never blocked from writing.
- **Project folder names are lossy.** Do not decode the cwd from the folder name. Read the real cwd from inside the entries, and use the folder name only as a display fallback.
- **Retention.** Claude Code deletes session files older than `cleanupPeriodDays` (default 30) at startup. When a source file has gone but we hold our copy, mark `source_missing` and say so in the UI. Never change that setting ourselves.
- **Secrets.** Transcripts can contain keys, tokens and `.env` contents. Redact likely secrets before display. Never log raw transcript content.
- **Read-only.** Never write to the Claude Code folder.

## Status is inferred

No hooks, so status is a heuristic from transcript content and file activity: running, waiting, done, failed, stale (no activity for N minutes, configurable).

Every status carries the reason it was chosen, shown in the UI. Where the data cannot tell "waiting for my permission" from "still running", say so instead of guessing. Check against real files before trusting any rule.

## Data model

Sketch:

```
sessions(id, project_cwd, source_path, started_at, last_activity_at,
         status, status_reason, source_missing)
tasks(id, session_id, parent_task_id, title, status, started_at, ended_at)
entries(id, session_id, task_id, kind, tool_name, summary, raw_json, ts, byte_offset)
file_offsets(path, byte_offset, size, mtime)
settings(key, value)
entries_fts   -- FTS5 over entry summaries
```

A task is one user prompt through to the end of that turn. Subagent runs are child tasks. Entries are append-only. Sessions and tasks are derived and must be rebuildable from entries.

Use numbered SQL migrations and a migrations table.

## UI guidelines

- Dark theme first, with light and system options. shadcn/ui and Tailwind.
- Task list on the left, detail on the right, resizable panes that remember their size.
- Command palette (`cmdk`) on Ctrl/Cmd+K.
- Keyboard first: arrows to move, Enter to open, `/` to search.
- Virtualise long lists.
- Empty, loading and error states everywhere. No blank panes.
- Show ingest progress in a status bar, never a blocking modal.
- UI text in British English.

## Auto-update

GitHub-hosted, signed and opt-out. Build it as follows.

Behaviour:

- On startup, check GitHub for a newer version in the background. Never delay startup. Fail silently when offline.
- If one exists, offer to install it, showing the version and release notes. Never install without asking.
- **Settings > About** shows the current version, a switch to turn the startup check off, and a "Check for updates" button.
- Updates are signed. The app only accepts bundles signed by the key whose public half is in `tauri.conf.json`.
- The check sends nothing about the user and is the only network call the app makes.

Implementation notes (verify against current Tauri v2 docs):

- `tauri-plugin-updater`, plus `tauri-plugin-process` for relaunch.
- `bundle.createUpdaterArtifacts` enabled in `tauri.conf.json`.
- Endpoint: `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`, with the public key in the config.
- Grant only the updater and process permissions in the capabilities file.
- Keep the `tauri` crate and the `@tauri-apps/*` npm packages on the same minor version.

Data safety on update:

- Back up the SQLite file before running any migration, and keep the last few backups.
- Migrations only move forward and avoid destroying data.
- If the database was written by a newer app version than the one running, refuse to open it and explain, instead of risking corruption.

## Releasing

- Pushing a version tag (`v0.2.0`) triggers GitHub Actions to build the Windows and macOS installers, sign them for the updater, and publish a release that includes `latest.json`.
- Repository secrets: `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- The version lives in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`. Add a `pnpm bump <version>` script that changes all three, and a CI step that fails if the tag and the versions disagree.
- Generate a separate updater key for this app (do not share one between my two apps) and keep a backup outside the repo. If the key is lost, installed copies cannot update and need a manual reinstall.
- The first version that contains the updater has to be installed by hand.
- Updater signing is separate from OS code signing. Windows SmartScreen and macOS Gatekeeper warnings are expected until I sort out signing. Document them in the README.

## Git

I explicitly authorise git use in this project. This overrides any global rule of mine against using git, and applies to this repository only.

You may: run `git init`, check status and diffs, create branches, stage and commit, merge, and create tags. You may push to this project's own `origin` remote.

Ask first for: force pushes or any history rewriting (amending or rebasing commits that are already pushed, `reset --hard` over work I have not seen), deleting remote branches or tags, and pushing a version tag, because a tag push publishes a release.

Practice:

- Small commits, one concern each, with messages that say why. If a commit adds a dependency, say why.
- Check `git status` before every commit.
- Never commit secrets, the updater private key, `.env` files, session data or export data. Keep these in `.gitignore`: `.env*`, `*.key`, `data/`, `node_modules`, `src-tauri/target`, local database files.
- Test fixtures must be anonymised. Never commit real transcripts or exports.

## Conventions

- TypeScript strict mode, no `any`. Small components, plain hooks.
- Rust: no `unwrap()` outside tests. Commands return `Result<T, AppError>` with a serialisable error type the UI can show.
- Heavy work runs off the main thread (`tokio` tasks or `spawn_blocking`). The UI never freezes during ingest.
- Emit progress with Tauri events, not polling.
- Paths use `PathBuf` and OS APIs, never string concatenation with slashes.
- Format with `prettier` and `cargo fmt`. Lint with `eslint` and `cargo clippy`. Fix warnings, do not silence them.
- Do not add a dependency without a reason, and say why in the commit message.
- Tests for the parts that go wrong quietly: the transcript parser (including partial lines and unknown types), status inference, offset resume, redaction, migrations and backup. UI tests are optional.
- Comments explain why, not what. British English. No em dashes.

## Commands

```
pnpm install
pnpm tauri dev
pnpm tauri build
pnpm lint
pnpm typecheck
pnpm bump <version>
cd src-tauri && cargo test
cd src-tauri && cargo clippy --all-targets -- -D warnings
```

## Build order

Get each step usable before starting the next.

1. Scaffold Tauri v2 with React, TS, Tailwind and shadcn. Confirm a hello-world builds and runs on both macOS and Windows.
2. Inspect real transcripts, write `docs/transcript-format.md`, make fixtures.
3. SQLite schema and migrations, parser and ingest, with tests.
4. Watcher with partial-line handling and offset resume.
5. Status inference with reasons, with tests.
6. Task list and detail UI with live updates.
7. Retention handling and the source-missing notice.
8. Auto-update, release workflow, version bump script. Test a real update from one tagged release to the next before moving on.
9. Search, then polish.

## Things to ask me rather than guess

- Whether macOS builds should be universal or separate arm64 and x64 artifacts.
- Whether in-app updates should be expected to work on macOS before notarisation. Test it and tell me what you find.
- Whether the Claude Code desktop app's own session list should be read as well as the CLI folder.
- Any real transcript examples that break the parser.
