#!/usr/bin/env bash
set -euo pipefail

destination="${1:?Usage: install-age.sh OUTPUT_DIRECTORY}"
if [[ "$(uname -s)/$(uname -m)" != Linux/x86_64 ]]; then
  echo 'Install age 1.3.2 for your platform from https://age-encryption.org.' >&2
  exit 1
fi
archive="$(mktemp)"
unpacked="$(mktemp -d)"
trap 'rm -f -- "$archive"; rm -rf -- "$unpacked"' EXIT
curl --fail --silent --show-error --location --connect-timeout 10 --max-time 120 \
  https://github.com/FiloSottile/age/releases/download/v1.3.2/age-v1.3.2-linux-amd64.tar.gz \
  --output "$archive"
printf '%s  %s\n' cbe24006683f8eb669266162894b9a522a1af52f2665fbc63a4bb032ed26ac10 "$archive" | sha256sum --check
tar -xzf "$archive" -C "$unpacked"
mkdir -p "$destination"
install -m 755 "$unpacked/age/age" "$unpacked/age/age-keygen" "$destination/"
