#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../apps/api"

cargo machete --with-metadata

# SQLx locks optional MySQL/SQLite dependencies that this PostgreSQL build excludes.
# These exceptions apply only while both affected crates remain disabled on every target.
dependency_tree="$(cargo tree --locked --target all --edges normal,build,dev --prefix none)"
if printf '%s\n' "$dependency_tree" | grep -E '^(rsa|libsqlite3-sys) v' >/dev/null; then
  printf '%s\n' 'An excluded database dependency is enabled; review and remove its lockfile-only audit exception.' >&2
  exit 1
fi
cargo audit --ignore RUSTSEC-2023-0071 --ignore RUSTSEC-2026-0459 --deny unsound --deny yanked
