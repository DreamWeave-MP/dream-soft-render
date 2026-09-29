+++
title = "Fast frames"
description = "What the rasterizer recognizes and draws as spans, what costs the most, how to keep an egui frame cheap, and the benchmarks."
weight = 70

[extra]
kind = "guide"
+++

Most of this crate is not a triangle rasterizer. It is the code that avoids needing one. A UI is
a pile of rectangles, rounded panels and glyphs, and each of those arrives as anonymous
triangles. Treated as triangles, a 5-pixel glyph costs two triangle setups, three edge functions
per pixel and an interpolated texture coordinate per pixel. Recognized as a rectangle, it costs a
row of texel copies.

## What the rasterizer recognizes

A mesh is read front to back. At each position the rasterizer tries, in order:

1. **A rectangle.** The next six indices name four distinct vertices forming an axis-aligned
   rectangle, all with one color. A solid one fills spans. A textured one copies texels along
   each row, if its UVs map linearly across it; glyph quads, images and everything `fill_rect`
   and `textured_rect` draw qualify.
2. **A fan.** Four or more consecutive triangles share a center vertex, walk its outline in one
   direction and have one color. The whole polygon fills spans, row by row. egui draws rounded
   rectangles, circles and filled shapes this way.
3. **One triangle.** A triangle with one color, or whose three corners sample the same texel,
   fills spans with that color. Anything else, a gradient, a feathered edge, a rotated or
   distorted texture, is covered and colored pixel by pixel.

All three give exactly the pixels general triangle rasterization would; the tests hold each fast
path to it. What they change is the cost.

## What costs the most

- **Per-pixel triangles.** Mixed vertex colors send a quad or fan to the last case. That is every
  gradient, every shadow, and every one of egui's feathered edges.
- **Feathering.** egui anti-aliases shape edges with a one-pixel strip of translucent triangles
  along each of them. They are thin, numerous and all per-pixel. Counted in AArch64 instructions
  per steady frame of the benchmark scenes, turning feathering off removes 44% of the work for a
  form, 17% for a page of text, and 35% for a window with a shadow.
  [Hosting egui](@/docs/egui.md#feathering) shows how, and what it looks like.
- **Pixels.** Every covered pixel costs a blend or a write. A full-surface `clear`, or a panel that
  covers the screen, touches every pixel once more.
- **Translucency.** An opaque span is a copy. A translucent one reads, blends and writes each
  pixel.

## Keeping an egui frame cheap

- Let unchanged frames skip. It is on by default, and a frame that matches the last one costs one
  comparison instead of a rasterization. [Hosting egui](@/docs/egui.md#frames-that-did-not-change)
  lists what counts as unchanged.
- Turn feathering off on slow CPUs. Text keeps its anti-aliasing.
- Present only frames whose `surface_changed` is true. The copy to the screen touches every
  pixel, changed or not.
- Render at the screen's own size. Scaling afterwards is one more pass over every pixel.

Drawing with `Frame` yourself, draw rectangles with `fill_rect` and `textured_rect`, give each
rectangle one color, and prefer a fan of one color to separate triangles.

## On AArch64

The hot loops run NEON code on AArch64: translucent spans, textured and glyph rows, gradient
rows, and the coverage scans that find where each triangle row starts and ends. It writes the
same bytes as the portable code. [Platforms](@/docs/platforms.md#architectures) lists each
kernel.

## Benchmarks

```sh
cargo bench --bench draw
cargo bench --features egui --bench frame
cargo bench --features egui --bench frame -- --no-feathering --scene form
```

Both report microseconds per frame at 640x480, as the median (p50) with the 10th and 90th
percentiles. Neither has `--help`; the options are at the top of each file under `benches/`.

`draw` uses the drawing API alone, without egui:

| Workload | Draws |
|---|---|
| `glyphs` | 3,200 7x9 textured rectangles, cut from a coverage atlas and tinted: a page of text |
| `panels` | 48 overlapping 120x60 rectangles, a third opaque and the rest translucent |
| `mesh` | a 256-triangle fan with a different color at every outer vertex, translucent |

`--frames N` sets the number of frames and `--workload NAME` runs one.

`frame` renders the three egui scenes the golden tests pin, `form`, `preview` and `window`, and
reports the rasterization and the whole call separately:

| Option | Does |
|---|---|
| `--frames N` | Frames measured per scene |
| `--scene NAME` | One scene only |
| `--no-feathering` | Turns egui's feathering off, as dream-ini's PortMaster build does |
| `--hash` | Prints each scene's golden hash |
| `--dump DIR` | Writes each scene's surface to `DIR/<scene>.rgba` |
| `--stats` | Prints the renderer's deep statistics for one frame |
| `--settle N` | Frames rendered before measuring; shorter, for tracing under qemu |

Run without `--bench`, as `cargo test --all-targets` does, each draws once, which keeps them
compiling and working.

### Numbers

Every release runs `cargo bench` in CI and attaches the output to its
[GitHub release](https://github.com/DreamWeave-MP/dream-soft-render/releases) as `BENCHMARKS.md`.
For 1.0.0, on GitHub's `ubuntu-22.04` x86-64 runner:

| Workload | p50 | p10 | p90 |
|---|---:|---:|---:|
| `glyphs` | 3089 µs | 3083 µs | 3117 µs |
| `panels` | 354 µs | 350 µs | 363 µs |
| `mesh` | 4791 µs | 4775 µs | 4820 µs |

A desktop x86-64 CPU tells you about a desktop x86-64 CPU. For the handhelds this crate is for,
compare AArch64 instruction counts, as the numbers above for feathering do, or measure on the
device.
