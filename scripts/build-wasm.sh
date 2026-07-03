#!/usr/bin/env bash
# Build the SANDGUN wasm package for the web app.
#
# Why this script exists: on this machine `rustc`/`cargo` on PATH are Homebrew's,
# which ship only the host target and CANNOT build wasm32-unknown-unknown. rustup's
# stable toolchain HAS the wasm target, but Homebrew's rustc shadows it on PATH and
# there are no rustup shims in ~/.cargo/bin. So we point rustc/cargo at rustup's
# toolchain explicitly while still using wasm-pack from ~/.cargo/bin.
#
# Usage: scripts/build-wasm.sh   (run from anywhere in the repo)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TOOLCHAIN_BIN="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin"
WASM_PACK="$HOME/.cargo/bin/wasm-pack"

if [ ! -x "$TOOLCHAIN_BIN/cargo" ]; then
  echo "error: rustup stable toolchain not found at $TOOLCHAIN_BIN" >&2
  echo "install with: rustup toolchain install stable && rustup target add wasm32-unknown-unknown" >&2
  exit 1
fi
if [ ! -x "$WASM_PACK" ]; then
  echo "error: wasm-pack not found at $WASM_PACK (install: cargo install wasm-pack)" >&2
  exit 1
fi

PATH="$TOOLCHAIN_BIN:$HOME/.cargo/bin:$PATH" RUSTUP_TOOLCHAIN=stable \
  "$WASM_PACK" build crates/sandgun-wasm --release --target web --out-dir ../../web/src/pkg

echo "✅ wasm built to web/src/pkg — refresh the dev server"
