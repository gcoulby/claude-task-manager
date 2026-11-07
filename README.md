# Claude Task Manager

A desktop app for macOS and Windows that shows what your Claude Code agents are doing: tasks, status, logs and timings. It reads the local Claude Code session folder (`~/.claude/projects/`) and keeps its own SQLite archive, so history survives Claude Code's 30-day clean-up.

It is read-only, local only and makes no network calls apart from an optional update check.

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

## Releasing

1. `pnpm bump 0.2.0` sets the version in `package.json`, `Cargo.toml` and `tauri.conf.json`.
2. Commit, tag `v0.2.0` and push the tag. The release workflow checks the versions match the tag, builds Apple Silicon macOS and Windows installers, signs them for the updater and publishes the release with `latest.json`.

Builds are Apple Silicon only on macOS.

### Updater key

Updates are signed with a key made for this app only:

```sh
pnpm tauri signer generate -w ~/.tauri/claude-task-manager.key
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/claude-task-manager.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

The public half (`.key.pub`) goes in `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`. Keep a backup of the private key and its password outside this repository: if they are lost, installed copies cannot update and need a manual reinstall. The first version that contains the updater has to be installed by hand.

Local builds do not need the key. Release builds add `src-tauri/tauri.release.conf.json`, which turns on updater artifacts.

## Unsigned builds

The installers are not yet code signed by Apple or Microsoft, so expect:

- **macOS:** builds are ad-hoc signed but not notarised, so Gatekeeper says it cannot verify the developer. Right-click the app, choose **Open**, then confirm, or allow it under **System Settings > Privacy & Security**. Version 0.1.1 was not signed as a whole bundle, so macOS calls it "damaged" instead; to open that one, run `xattr -dr com.apple.quarantine "/Applications/Claude Task Manager.app"`.
- **Windows:** SmartScreen shows "Windows protected your PC". Choose **More info**, then **Run anyway**.

## Licence

MIT. See [LICENSE](LICENSE).
