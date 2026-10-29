#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../apps/api"

cargo machete --with-metadata

# SQLx's lockfile includes its optional MySQL driver and rsa, even for PostgreSQL.
# RUSTSEC-2023-0071 has no patch. Never suppress it if rsa enters an enabled graph.
dependency_tree="$(cargo tree --locked --target all --edges normal,build,dev --prefix none)"
if printf '%s\n' "$dependency_tree" | grep -E '^rsa v' >/dev/null; then
  printf '%s\n' 'rsa is enabled; the lockfile-only advisory exception no longer applies.' >&2
  exit 1
fi
cargo audit --ignore RUSTSEC-2023-0071 --deny unsound --deny yanked
