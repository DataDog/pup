#!/usr/bin/env bash
# Build pup's browser WASM library and generate wasm-bindgen glue into ./pkg.
# Requires: rustup target wasm32-unknown-unknown, wasm-bindgen-cli 0.2.114.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"

cd "$root"
CARGO_PROFILE_RELEASE_OPT_LEVEL=z \
CARGO_PROFILE_RELEASE_LTO=thin \
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
CARGO_PROFILE_RELEASE_STRIP=symbols \
  cargo build --target wasm32-unknown-unknown --no-default-features --features browser --lib --release
wasm-bindgen --target web --out-dir "$here/pkg" \
  target/wasm32-unknown-unknown/release/pup_wasm.wasm
ls -la "$here/pkg"
