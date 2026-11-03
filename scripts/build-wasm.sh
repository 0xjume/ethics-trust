#!/usr/bin/env bash
# Release wasm for keyless-trust, then wasm-opt -Oz when binaryen is installed.
# rustc stays 1.93 (near-sys 0.2.14). cargo-near's 1.86 pin does not compile that.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
cargo build -p keyless-trust --target wasm32-unknown-unknown --release
WASM="$ROOT/target/wasm32-unknown-unknown/release/keyless_trust.wasm"
BEFORE=$(wc -c < "$WASM")
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz -o "$WASM" "$WASM"
  AFTER=$(wc -c < "$WASM")
  echo "wasm-opt -Oz: ${BEFORE} -> ${AFTER} bytes"
else
  echo "wasm-opt not found; leaving unoptimised ${BEFORE} bytes. Install binaryen."
fi
ls -la "$WASM"
