# Windows build (cross-compile)

The launcher `iron-sim` (crates/ui) cross-builds from Linux to Windows via the
`x86_64-pc-windows-gnu` target and a mingw-w64 toolchain.

Prereqs (Debian/Ubuntu host):

```bash
sudo apt-get install -y mingw-w64
rustup target add x86_64-pc-windows-gnu
```

Linker pin (already committed in `rts-game/.cargo/config.toml`):

```toml
[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
```

Build the launcher:

```bash
cd rts-game
cargo build --release -p game-ui --target x86_64-pc-windows-gnu
# -> target/x86_64-pc-windows-gnu/release/iron-sim.exe
```

Notes:

- eframe/egui + wgpu compile cleanly for windows-gnu.
- Determinism note: the Windows build is the SAME deterministic core (integer
  only, SplitMix64); the platform layer only renders.