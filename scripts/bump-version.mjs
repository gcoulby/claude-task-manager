// Usage: pnpm bump 0.2.0
import { isSemver, readVersions, writeVersion } from "./version-files.mjs";

const next = process.argv[2]?.replace(/^v/, "");
if (!next || !isSemver(next)) {
  console.error("Usage: pnpm bump <version>, for example pnpm bump 0.2.0");
  process.exit(1);
}
const before = readVersions();
writeVersion(next);
for (const [file, version] of Object.entries(before)) {
  console.log(`${file}: ${version} -> ${next}`);
}
console.log(`\nNext: commit, then tag v${next}. Pushing the tag publishes a release.`);
