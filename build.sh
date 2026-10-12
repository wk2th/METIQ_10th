#!/usr/bin/env sh
# Builds core/ (Rust, no_std) into site/wasm/metiq_core.wasm.
# Requires: rustup target add wasm32-unknown-unknown
set -eu
cd "$(dirname "$0")/core"
cargo build --release
cp target/wasm32-unknown-unknown/release/metiq_core.wasm ../site/wasm/metiq_core.wasm
ls -l ../site/wasm/metiq_core.wasm
