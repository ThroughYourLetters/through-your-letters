#!/usr/bin/env bash
set -euo pipefail

# Official Linux x86_64 release archives, checked against their published digests.
auditor_dir="${1:?Usage: install-dependency-auditors.sh OUTPUT_DIRECTORY}"
mkdir -p "$auditor_dir"
install_auditor() {
  local binary="$1" url="$2" digest="$3"
  local archive="$auditor_dir/$binary.tar.gz"
  curl --fail --silent --show-error --location "$url" --output "$archive"
  printf '%s  %s\n' "$digest" "$archive" | sha256sum --check
  mkdir -p "$auditor_dir/$binary-release"
  tar -xzf "$archive" --strip-components=1 -C "$auditor_dir/$binary-release"
  install -m 755 "$auditor_dir/$binary-release/$binary" "$auditor_dir/$binary"
}
install_auditor cargo-audit \
  https://github.com/rustsec/rustsec/releases/download/cargo-audit/v0.22.2/cargo-audit-x86_64-unknown-linux-musl-v0.22.2.tgz \
  7fb9497f8594b389e5fce5ef9b92db08432996895b2e0c5a0167a69ed445c428
install_auditor cargo-machete \
  https://github.com/bnjbvr/cargo-machete/releases/download/v0.9.2/cargo-machete-v0.9.2-x86_64-unknown-linux-musl.tar.gz \
  48200087f54c55aabcd4db4af1e25742b49846c02a1b1bfa134711945b35b2e9
