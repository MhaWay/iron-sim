# Building for Windows (from Linux)

Cross-compilation target: x86_64-pc-windows-gnu.

## Toolchain (one-time)
    sudo apt-get install gcc-mingw-w64-x86-64 binutils-mingw-w64-x86-64
    rustup target add x86_64-pc-windows-gnu

The workspace .cargo/config.toml pins the mingw linker for that target.

## Build
    cd rts-game
    cargo build --release --target x86_64-pc-windows-gnu --bins --examples
    # artifacts: target/x86_64-pc-windows-gnu/release/{demo_v2,server,client,simdemo,bench}.exe
    cd ../rimworld-game
    cargo build --release --target x86_64-pc-windows-gnu --bins
    # artifacts: target/x86_64-pc-windows-gnu/release/{server,client}.exe

## Determinism note
The simulation is integer-only and deterministic across targets: identical command streams
produce identical state hashes on Windows, Linux, and the C++ port. No floating point is used
in any sim or renderer code path. The renderer (crates/render) and PNG encoder are also
integer-only, so rendered demo frames are byte-identical cross-platform for a given seed.

## Known limitations
- 32-bit Windows (i686) and ARM64 Windows (aarch64-pc-windows) are not cross-built here.
  Add the corresponding rustup target + mingw if needed.
- The binaries link standard MinGW runtime libraries; no non-system DLLs are required.
