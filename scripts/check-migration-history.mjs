import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";

const base = process.argv[2];
assert.match(base || "", /^[a-f0-9]{40}$/, "Pass the base commit SHA");
if (/^0+$/.test(base)) {
  console.log("Initial push has no existing migration history.");
  process.exit(0);
}
const git = (...args) => execFileSync("git", args, { encoding: "utf8" });
const directory = "apps/api/migrations";
const files = (revision) =>
  git("ls-tree", "-r", "--name-only", "-z", revision, "--", directory)
    .split("\0")
    .filter((path) => path.endsWith(".sql"));
const version = (path) => {
  const match = path.match(/^apps\/api\/migrations\/(\d+)_[^/\n]+\.sql$/);
  assert.ok(match, `Invalid migration filename: ${path}`);
  return BigInt(match[1]);
};
const latest = files(base).reduce(
  (max, path) => (version(path) > max ? version(path) : max),
  0n,
);
const changes = git(
  "diff",
  "--no-renames",
  "--name-status",
  "-z",
  base,
  "HEAD",
  "--",
  directory,
).split("\0");
for (let i = 0; i < changes.length - 1; i += 2) {
  const [status, path] = changes.slice(i, i + 2);
  if (!path.endsWith(".sql")) continue;
  assert.equal(
    status,
    "A",
    `Applied migrations are immutable: ${path}. Add a new forward migration.`,
  );
  assert.ok(
    version(path) > latest,
    `New migration must follow version ${latest}: ${path}`,
  );
}
const versions = files("HEAD").map((path) => String(version(path)));
assert.equal(
  new Set(versions).size,
  versions.length,
  "Duplicate migration versions",
);
console.log(
  "Migration history is immutable; new migrations follow the base history.",
);
