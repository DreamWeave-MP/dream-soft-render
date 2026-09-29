+++
title = "Start here"
description = "Add the crate, draw a rectangle, a texture and a triangle, and save the frame as an image."
weight = 10

[extra]
kind = "tutorial"
+++

This page takes you from an empty Cargo project to a program that draws a frame and writes it to
an image file you can open. Nothing here needs a GPU, a window or egui.

## Add the crate

```sh
cargo add dream-soft-render
```

It needs Rust 1.92 or newer. Without features it has no dependencies at all. The egui adapter is
the `egui` feature; [Hosting egui](@/docs/egui.md) starts there.

## Draw a frame

```rust
use std::fs::File;
use std::io::{BufWriter, Write};

use dream_soft_render::{
    ClipRect, Color, Mesh, Rect, SoftwareRenderer, SoftwareSurface, Vertex,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut renderer = SoftwareRenderer::default();

    // A 2x2 checkerboard: premultiplied RGBA8, top row first.
    let checker = renderer.create_texture(
        2,
        2,
        &[
            255, 255, 255, 255, 0, 0, 0, 255, //
            0, 0, 0, 255, 255, 255, 255, 255,
        ],
    )?;

    let mut frame = renderer.begin_frame(160, 120)?;
    frame.clear(Color::from_rgb(17, 20, 28));
    frame.fill_rect(
        Rect::from_min_size([8.0, 8.0], [144.0, 24.0]),
        Color::from_rgba_unmultiplied(80, 160, 255, 192),
        ClipRect::ALL,
    )?;
    frame.textured_rect(
        Rect::from_min_size([8.0, 40.0], [32.0, 32.0]),
        Rect::FULL_UV,
        checker,
        Color::WHITE,
        ClipRect::ALL,
    )?;
    let amber = Color::from_rgb(240, 170, 40);
    frame.mesh(
        Mesh {
            vertices: &[
                Vertex::new([100.0, 40.0], [0.0, 0.0], amber),
                Vertex::new([150.0, 112.0], [0.0, 0.0], amber),
                Vertex::new([50.0, 112.0], [0.0, 0.0], amber),
            ],
            indices: &[0, 1, 2],
            texture: None,
        },
        ClipRect::ALL,
    )?;

    write_ppm(frame.surface(), "frame.ppm")?;
    Ok(())
}

/// A binary PPM: a short header, then RGB bytes. Most image viewers open it.
fn write_ppm(surface: &SoftwareSurface, path: &str) -> std::io::Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    write!(file, "P6\n{} {}\n255\n", surface.width, surface.height)?;
    for pixel in surface.pixels.chunks_exact(4) {
        file.write_all(&pixel[..3])?;
    }
    file.flush()
}
```

Run it and open `frame.ppm`. Shown here at twice its size:

{{ figure(src="/img/docs/start-here.png", alt="A dark 160 by 120 pixel frame: a translucent blue bar across the top, a two by two checkerboard stretched to a 32 pixel square below it on the left, and an amber triangle on the right.", caption="`frame.ppm`, scaled 2x with nearest-neighbor.") }}

What happened, in order:

1. `create_texture` stored the checkerboard and returned a `TextureId`, a handle you draw with.
2. `begin_frame(160, 120)` sized the surface. A new surface starts transparent black; nothing is
   cleared for you after that.
3. `clear` overwrote every pixel with an opaque background.
4. `fill_rect` blended a translucent rectangle over it. `from_rgba_unmultiplied` took the color as
   you would write it in a color picker and premultiplied it, because every color the renderer
   stores is premultiplied.
5. `textured_rect` stretched the texture over a 32x32 square, nearest-texel, so the checkerboard
   stays sharp.
6. `mesh` drew one triangle from three vertices.

Each call drew its pixels before it returned. `frame.surface().pixels` is the result: 160 × 120 ×
4 bytes, red, green, blue and alpha per pixel, rows top to bottom. The PPM writer drops the alpha
byte, which is 255 everywhere here: the background is opaque, and blending always writes 255.

## Next

- [The rules](@/docs/rules.md): which pixels a shape covers, and what the bytes mean.
- [Drawing](@/docs/drawing.md) and [Textures](@/docs/textures.md): the rest of the drawing API.
- [Hosting egui](@/docs/egui.md), if you are here to draw an egui interface.
- [Presenting frames](@/docs/presenting.md): from the surface to a screen.
