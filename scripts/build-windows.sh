#!/usr/bin/env bash
# Cross-build Windows x86_64 artifacts for both games. Requires: gcc-mingw-w64-x86-64, rustup target x86_64-pc-windows-gnu.
set -euo pipefail
cd "$(dirname "$0")/.."
for w in rts-game rimworld-game; do
  echo "==> $w"
  (cd "$w" && cargo build --release --target x86_64-pc-windows-gnu --bins --examples)
done
echo "==> artifacts under */target/x86_64-pc-windows-gnu/release/"
