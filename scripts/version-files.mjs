// The app version lives in three files (plus Cargo.lock's copy). These
// helpers read and write it in each, so bump and check agree on the rules.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const at = (p) => path.join(root, p);

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

export function isSemver(v) {
  return SEMVER.test(v);
}

// Matches the version line inside Cargo.toml's [package] table only.
const CARGO_PACKAGE_VERSION = /(\[package\][^[]*?\nversion\s*=\s*")([^"]+)(")/;
const LOCK_ENTRY = /(\[\[package\]\]\nname = "claude-task-manager"\nversion = ")([^"]+)(")/;

export const files = {
  "package.json": {
    read: (text) => JSON.parse(text).version,
    write: (text, v) => text.replace(/("version":\s*")[^"]+(")/, `$1${v}$2`),
  },
  "src-tauri/tauri.conf.json": {
    read: (text) => JSON.parse(text).version,
    write: (text, v) => text.replace(/("version":\s*")[^"]+(")/, `$1${v}$2`),
  },
  "src-tauri/Cargo.toml": {
    read: (text) => text.match(CARGO_PACKAGE_VERSION)?.[2],
    write: (text, v) => text.replace(CARGO_PACKAGE_VERSION, `$1${v}$3`),
  },
  "src-tauri/Cargo.lock": {
    read: (text) => text.match(LOCK_ENTRY)?.[2],
    write: (text, v) => text.replace(LOCK_ENTRY, `$1${v}$3`),
  },
};

export function readVersions() {
  return Object.fromEntries(
    Object.entries(files).map(([name, f]) => [name, f.read(readFileSync(at(name), "utf8"))]),
  );
}

export function writeVersion(version) {
  for (const [name, f] of Object.entries(files)) {
    const before = readFileSync(at(name), "utf8");
    const after = f.write(before, version);
    if (f.read(after) !== version) {
      throw new Error(`Could not set the version in ${name}`);
    }
    writeFileSync(at(name), after);
  }
}
