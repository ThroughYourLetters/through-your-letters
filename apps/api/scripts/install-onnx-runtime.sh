#!/usr/bin/env bash
set -euo pipefail

# Official CPU releases for ort 2.0.0-rc.11 (native API 23).
runtime_dir="${1:?Usage: install-onnx-runtime.sh OUTPUT_DIRECTORY}"
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64)
    platform=linux-x64
    digest=1fa4dcaef22f6f7d5cd81b28c2800414350c10116f5fdd46a2160082551c5f9b
    ;;
  Linux/aarch64)
    platform=linux-aarch64
    digest=7c63c73560ed76b1fac6cff8204ffe34fe180e70d6582b5332ec094810241e5c
    ;;
  *)
    echo 'This installer supports Linux x86_64/aarch64; install ONNX Runtime 1.23.2 for your platform and set ORT_DYLIB_PATH.' >&2
    exit 1
    ;;
esac

archive="$(mktemp)"
trap 'rm -- "$archive"' EXIT
curl --fail --silent --show-error --location \
  "https://github.com/microsoft/onnxruntime/releases/download/v1.23.2/onnxruntime-$platform-1.23.2.tgz" \
  --output "$archive"
printf '%s  %s\n' "$digest" "$archive" | sha256sum --check
mkdir -p "$runtime_dir"
tar -xzf "$archive" --strip-components=1 -C "$runtime_dir"
test -f "$runtime_dir/lib/libonnxruntime.so.1.23.2"
