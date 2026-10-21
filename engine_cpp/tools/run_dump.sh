#!/usr/bin/env bash
# Regenerate testdata/*.json from the Rust reference. Requires cargo; does NOT
# modify anything under engine/ or rts-game/ — path deps are read-only.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
source "${CARGO_ENV:-/root/.cargo/env}" 2>/dev/null || true
mkdir -p "$here/testdata"
tmp="$(mktemp -d)"
mkdir -p "$tmp/src"
cp "$here/tools/dump_vectors.rs" "$tmp/src/main.rs"
cat > "$tmp/Cargo.toml" <<EOF
[package]
name = "dump_vectors"
version = "0.0.0"
edition = "2021"
[dependencies]
engine-core = { path = "$here/../engine/crates/core" }
shared = { path = "$here/../rts-game/crates/shared" }
[workspace]
EOF
cargo build --release --quiet --manifest-path "$tmp/Cargo.toml"
OUT_DIR="$here/testdata" "$tmp/target/release/dump_vectors"
rm -rf "$tmp"
echo "testdata regenerated"
