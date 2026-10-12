#!/usr/bin/env sh
# Builds core/ (Rust, no_std) into site/wasm/metiq_core.wasm, then renders the
# no-JS fallback logo GIF through that same binary.
# Requires: rustup target add wasm32-unknown-unknown, node
set -eu
root="$(cd "$(dirname "$0")" && pwd)"
cd "$root/core"
cargo build --release
cp target/wasm32-unknown-unknown/release/metiq_core.wasm "$root/site/wasm/metiq_core.wasm"
ls -l "$root/site/wasm/metiq_core.wasm"
node "$root/tools/fallback_gif.mjs"
