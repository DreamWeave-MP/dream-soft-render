+++
title = "dream-soft-render"
description = "A CPU rasterizer for 2D triangle meshes and egui frames. It writes premultiplied RGBA8 into a buffer you own, and the same draw calls give the same bytes on every platform."

[taxonomies]
tags = ["Rust", "Rasterizer", "egui", "PortMaster"]

[extra]
sections = ["overview", "media", "install", "releases", "credits"]
+++

A handheld running PortMaster has a framebuffer, a small ARM core, and no GPU a program can count
on. An egui interface still has to be drawn. dream-soft-render draws it on the CPU: rectangles,
textured rectangles and triangle meshes go in, and premultiplied RGBA8 comes out, in a buffer you
own. Copying it to `/dev/fb0`, writing a PNG or hashing it in a test is up to you.

There is no window, no swapchain and no driver. There is also no randomness: the same draw calls
produce the same bytes on x86-64 and on AArch64, whose NEON code is held to the portable code by
tests. It was pulled out of [dream-ini](https://dreamweave-mp.github.io/dream-ini/), whose
PortMaster build draws its whole GUI through it; the header image is one of the egui scenes its
golden tests pin, drawn by this crate.

{{ schematic(data_path="data/schematics/frame.json") }}

## What it does

{% features() %}
- **Same bytes everywhere.** Output is a function of the draw calls. Golden hashes of whole
  frames pass on x86-64 and on AArch64, NEON included.
- **Draws now.** Each call rasterizes the moment you make it, in call order. No command list, no
  deferred state, and no clear you did not ask for.
- **Refuses bad input.** An index past the vertices, a NaN coordinate, pixel data of the wrong
  length or a freed texture is an error, not a quietly wrong frame.
- **Hosts egui, if you want.** With the `egui` feature, `render_egui` runs a UI closure into the
  same surface, and skips frames that came out identical to the last.
- **Knows what it is drawing.** Rectangles, glyph quads, one-color fans and single-texel
  triangles draw as spans. Only the rest pays for per-pixel triangle work.
- **Depends on nothing.** Without the `egui` feature the crate has no dependencies, and no
  platform code beyond its NEON kernels.
{% end %}

```rust
use dream_soft_render::{ClipRect, Color, Error, Rect, SoftwareRenderer};

fn main() -> Result<(), Error> {
    let mut renderer = SoftwareRenderer::default();
    let mut frame = renderer.begin_frame(320, 240)?;
    frame.clear(Color::from_rgb(17, 20, 28));
    frame.fill_rect(
        Rect::from_min_size([8.0, 8.0], [304.0, 40.0]),
        Color::from_rgba_unmultiplied(80, 160, 255, 192),
        ClipRect::ALL,
    )?;

    // 320 * 240 * 4 bytes, top row first.
    let rgba: &[u8] = &frame.surface().pixels;
    assert_eq!(&rgba[..4], &[17, 20, 28, 255]);
    Ok(())
}
```

## Documentation

This site is the crate's documentation, its API reference included.

- **[Start here](@/docs/start-here.md)**: add the crate, draw a frame and look at it.
- **[The rules](@/docs/rules.md)**: coordinates, coverage, color, blending and sampling, exactly.
- **[Hosting egui](@/docs/egui.md)**: `render_egui`, textures, unchanged frames and feathering.
- **[Rust API](@/docs/api/_index.md)**: every type and method, with what it does and how it fails.
