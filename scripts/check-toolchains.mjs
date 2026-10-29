import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const read = (path) =>
  readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
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
  `>=${node} <${nodeMajor + 1}`,
  "Node's supported range must start at the tested pin and bound its major",
);
assert.equal(
  capture("apps/web/Dockerfile", /^FROM node:([^\s]+) AS builder$/m),
  `${node}-alpine`,
  "The web image must use the pinned Node toolchain",
);
assert.equal(
  pkg.packageManager,
  `pnpm@${pkg.engines.pnpm}`,
  "The pnpm engine must match the Corepack/package-manager pin",
);

// This also supplies the exact version to the Rust setup action in CI.
console.log(`rust=${rust}`);
