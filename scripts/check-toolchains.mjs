import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

/** Resolve repository files relative to this script, independent of the shell's working directory. */
const read = (path) =>
  readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
/** Reject missing or malformed version declarations before CI selects its toolchain. */
const capture = (path, pattern) => {
  const match = read(path).match(pattern);
  assert.ok(match, `Missing version pin in ${path}`);
  return match[1];
};

const rust = capture(
  "apps/api/rust-toolchain.toml",
  /^channel\s*=\s*"(\d+\.\d+\.\d+)"$/m,
);
assert.equal(
  capture("apps/api/Cargo.toml", /^rust-version\s*=\s*"([^"]+)"$/m),
  rust,
  "Cargo's tested minimum must match the Rust toolchain pin",
);
assert.equal(
  capture("apps/api/Dockerfile", /^FROM rust:([^\s]+) AS builder$/m),
  `${rust}-slim-trixie`,
  "The API image must use the pinned Rust toolchain",
);

const node = read(".nvmrc").trim();
assert.match(node, /^\d+\.\d+\.\d+$/);
const nodeMajor = Number(node.split(".")[0]);
const pkg = JSON.parse(read("package.json"));
assert.match(
  pkg.devDependencies["@types/node"],
  new RegExp(`^\\^${nodeMajor}\\.`),
  "Node API types must target the selected runtime major",
);
assert.equal(
  pkg.engines.node,
  `${nodeMajor}.x`,
  "Hosted Node builds must select the same major as the exact development/CI pin",
);
assert.equal(
  capture("apps/web/Dockerfile", /^FROM node:([^\s]+) AS builder$/m),
  `${node}-alpine`,
  "The web image must use the pinned Node toolchain",
);
const pnpm = pkg.packageManager.match(/^pnpm@(\d+\.\d+\.\d+)$/);
assert.ok(pnpm, "Corepack must select an exact pnpm version");
const pnpmMajor = Number(pnpm[1].split(".")[0]);
assert.equal(
  pkg.engines.pnpm,
  `>=${pnpm[1]} <${pnpmMajor + 1}`,
  "Hosted pnpm builds must stay within the pinned package manager's major",
);

// This also supplies the exact version to the Rust setup action in CI.
console.log(`rust=${rust}`);
