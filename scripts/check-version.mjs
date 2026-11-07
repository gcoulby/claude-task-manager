// Fails if the version files disagree, or (given a tag such as v0.2.0) if
// they disagree with the tag. CI runs this before building a release.
import { readVersions } from "./version-files.mjs";

const tag = process.argv[2]?.replace(/^refs\/tags\//, "");
const versions = readVersions();
const distinct = new Set(Object.values(versions));
let ok = true;

if (distinct.size !== 1 || distinct.has(undefined)) {
  console.error("Version files disagree:");
  for (const [file, v] of Object.entries(versions)) console.error(`  ${file}: ${v ?? "not found"}`);
  ok = false;
}
const [version] = distinct;
if (ok && tag !== undefined && tag !== `v${version}`) {
  console.error(
    `Tag ${tag} does not match version ${version}. Run pnpm bump ${tag.replace(/^v/, "")}.`,
  );
  ok = false;
}
if (!ok) process.exit(1);
console.log(`Version ${version}${tag ? ` matches ${tag}` : ""}.`);
