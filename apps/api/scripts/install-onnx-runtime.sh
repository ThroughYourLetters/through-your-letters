#!/usr/bin/env bash
set -euo pipefail

# Microsoft's CPU package for ort 2.0.0-rc.11 (native API 23).
runtime_dir="${1:?Usage: install-onnx-runtime.sh OUTPUT_DIRECTORY}"
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64)
    platform=linux-x64
    ;;
  Linux/aarch64)
    platform=linux-arm64
    ;;
  *)
    echo 'This installer supports Linux x86_64/aarch64; install ONNX Runtime 1.23.2 for your platform and set ORT_DYLIB_PATH.' >&2
    exit 1
    ;;
esac

archive="$(mktemp)"
trap 'rm -- "$archive"' EXIT
curl --fail --silent --show-error --location \
  --connect-timeout 10 --max-time 120 \
  'https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/1.23.2/microsoft.ml.onnxruntime.1.23.2.nupkg' \
  --output "$archive"
printf '%s  %s\n' '25fe172f7fcdf34f2b5b02c8b997f6b88abc282956849d4af4335af23d0c4e4a' "$archive" | sha256sum --check
mkdir -p "$runtime_dir/lib"
unzip -p "$archive" "runtimes/$platform/native/libonnxruntime.so" > "$runtime_dir/lib/libonnxruntime.so.1.23.2"
unzip -p "$archive" "runtimes/$platform/native/libonnxruntime_providers_shared.so" > "$runtime_dir/lib/libonnxruntime_providers_shared.so"
unzip -p "$archive" LICENSE > "$runtime_dir/LICENSE"
unzip -p "$archive" ThirdPartyNotices.txt > "$runtime_dir/ThirdPartyNotices.txt"
ln -sfn libonnxruntime.so.1.23.2 "$runtime_dir/lib/libonnxruntime.so.1"
ln -sfn libonnxruntime.so.1.23.2 "$runtime_dir/lib/libonnxruntime.so"
test -f "$runtime_dir/lib/libonnxruntime.so.1.23.2"
