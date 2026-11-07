# Claude Task Manager

A desktop app for macOS and Windows that shows what your Claude Code agents are doing: tasks, status, logs and timings. It reads the local Claude Code session folder (`~/.claude/projects/`) and keeps its own SQLite archive, so history survives Claude Code's 30-day clean-up.

It is read-only, local only and makes no network calls apart from an optional update check.

> Early development. The app currently builds and runs, but does not read sessions yet.

## Requirements

- Node 20 or newer
- pnpm
- Rust 1.90 or newer (stable). Newer Tauri releases can raise this, so check before bumping Tauri.
- Platform prerequisites for Tauri v2: <https://v2.tauri.app/start/prerequisites/>

## Development

```sh
pnpm install
pnpm tauri dev
```

Checks:

```sh
pnpm typecheck
pnpm lint
pnpm format:check
cd src-tauri && cargo test
cd src-tauri && cargo clippy --all-targets -- -D warnings
```

Release build:

```sh
pnpm tauri build
```

## Unsigned builds

The installers are not yet code signed by Apple or Microsoft, so expect:

- **macOS:** Gatekeeper says the app cannot be opened. Right-click the app, choose **Open**, then confirm.
- **Windows:** SmartScreen shows "Windows protected your PC". Choose **More info**, then **Run anyway**.
