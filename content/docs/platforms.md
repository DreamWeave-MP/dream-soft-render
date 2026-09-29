+++
title = "Platforms"
description = "What is architecture-specific, each NEON kernel and how it is held to the portable code, features, the supported Rust, the license, and what is tested where."
weight = 80

[extra]
kind = "reference"
+++

dream-soft-render is plain Rust with no platform code of its own beyond the NEON kernels below.
It builds wherever Rust and, with the `egui` feature, egui do, and it draws the same bytes on each.

## Architectures

| Target | Hot loops run |
|---|---|
| AArch64, little-endian: Linux handhelds, Android, Apple silicon, Windows on Arm | NEON kernels, where they apply, and portable code for the rest |
| Everything else: x86-64, 32-bit ARM, big-endian AArch64, WebAssembly | Portable code only |

The choice is made when the crate compiles, by `cfg(all(target_arch = "aarch64", target_endian =
"little"))`. There is no runtime detection, because every AArch64 CPU has NEON. There are no
x86 intrinsics; on x86-64 the compiler vectorizes the portable loops as it sees fit.

Every kernel produces exactly the bytes the portable code produces for the same input, rounding
included. A kernel that cannot handle a case, because a 16-byte access would leave the texture or
the surface, or the run is too short, hands it to the portable code.

### The NEON kernels

| Kernel | Used for | Width | Source |
|---|---|---|---|
| Translucent span | One translucent color over a run of pixels: `fill_rect`, fans, one-color triangles | 16 pixels per block, portable tail | `src/surface.rs` |
| Textured row | A textured rectangle drawn one texel per pixel across, untinted | 16 pixels per block | `src/raster/quad.rs` |
| Tinted textured row | The same with a vertex color: glyphs, tinted images | 16 pixels per block, then the rest of the row up to 4 pixels at a time, each as one 16-byte vector | `src/raster/quad.rs` |
| Gradient row | Triangles whose vertex colors differ over a white texel: gradients, feathered edges, shadows | 4 pixels per step | `src/raster/textured.rs` |
| Coverage scan | Where each triangle row's covered pixels start and end, for rows of 8 or more candidate pixels | 4 pixels per step | `src/raster/coverage.rs` |

The newer kernels each have a randomized test on AArch64 that runs them against the portable
code over generated inputs: 50,000 rows for the coverage scan, 20,000 runs for gradient rows, and
20,000 short runs of 1 to 4 pixels for tinted rows. The textured-row blocks are checked against general
triangle rasterization on every architecture. The translucent span is checked against a scalar reference
on every architecture. Above those, the golden tests hash four whole frames, three egui scenes
and one drawn through `Frame`, and the hashes are the same constants on every target.

### Running the tests

```sh
cargo test --all-features
```

`--all-features` includes the egui adapter's tests and the three egui golden scenes in
`tests/golden.rs`; `tests/api.rs` renders its golden scene through `Frame` alone. Each golden
frame is pinned by an FNV-1a hash of the whole surface, so a change that moves one byte fails.
That is deliberate: "looks the same" is not a test.

### Checking them without an ARM machine

qemu's user mode runs AArch64 test binaries on an x86-64 Linux host, and Rust's bundled linker
links them statically against musl:

```sh
rustup target add aarch64-unknown-linux-musl
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld \
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUNNER=qemu-aarch64-static \
cargo test --release --all-features --target aarch64-unknown-linux-musl
```

It runs three more unit tests than an x86-64 host, the three AArch64-only kernel tests, and the
same golden hashes. If a golden hash fails there and not on x86-64, a kernel is wrong.

For a quicker check that the kernels still compile and pass Clippy, without linking or running:

```sh
cargo clippy --target aarch64-unknown-linux-gnu --all-targets --all-features \
    -- -W clippy::pedantic -D warnings
```

## Features

| Feature | Default | Adds |
|---|---|---|
| `egui` | No | The [`egui_adapter`](@/docs/api/egui-adapter.md) module, `SoftwareRenderer::render_egui` and `set_skip_unchanged_frames`, and a dependency on egui 0.34 |

Without it the crate has no dependencies.

## Rust

Rust 1.92 or newer, declared as `rust-version` in `Cargo.toml`. That is egui 0.34's floor, and it
applies with or without the `egui` feature.

`Error` is `#[non_exhaustive]`: match it with a wildcard arm.

## License

MIT OR Apache-2.0, at your option, since the first release.

## What is tested

Every push runs [StroggForge](https://github.com/DreamWeave-MP/StroggForge)'s library workflow:

- the tests, with every feature, on Linux x86-64, Windows x86-64, macOS on Intel, and macOS on
  Apple silicon, where the NEON kernels run;
- Clippy at the pedantic level with every warning an error, and `rustfmt`;
- `cargo audit` against the RustSec advisory database;
- `cargo check` on Rust 1.92, the declared minimum;
- a dry run of the crates.io publish.

A daily workflow repeats `rustfmt`, Clippy and `cargo audit` against the latest stable Rust.

The suite covers every fast path against general triangle rasterization, the blending and
rounding tables in full, every validation error, texture handles across renderers, egui texture
uploads and frees, unchanged-frame skipping against full rendering over 120 frames, and the
golden frames. Beyond CI, dream-ini's PortMaster build renders its GUI through the crate on
AArch64 Linux handhelds.
