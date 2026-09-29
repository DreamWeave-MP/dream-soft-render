+++
title = "Drawing"
description = "Frames, clearing, rectangles, textured rectangles, meshes, clip rectangles, draw order, and what happens when a call fails."
weight = 30

[extra]
kind = "guide"
+++

All drawing goes through a `Frame`, which `SoftwareRenderer::begin_frame` returns. A frame
borrows the renderer mutably and has no end call: drop it, and the surface holds what you drew.

```rust
use dream_soft_render::{ClipRect, Color, Error, Rect, SoftwareRenderer};

fn main() -> Result<(), Error> {
    let mut renderer = SoftwareRenderer::default();
    {
        let mut frame = renderer.begin_frame(64, 48)?;
        frame.clear(Color::from_rgb(17, 20, 28));
        frame.fill_rect(
            Rect::from_min_size([4.0, 4.0], [56.0, 8.0]),
            Color::WHITE,
            ClipRect::ALL,
        )?;
    }
    let surface = renderer.surface();
    assert_eq!(
        (surface.width, surface.height, surface.pixels.len()),
        (64, 48, 64 * 48 * 4)
    );
    Ok(())
}
```

Because the frame holds the renderer, textures are made before `begin_frame` or after the frame
is dropped, not in the middle.

## One frame after another

`begin_frame(width, height)` sizes the surface and nothing else. If the size is the one the last
frame had, the pixels are still there, and you can draw only what changed. If the size changed,
the surface starts transparent black. Either way, start with `clear` when you want a background:

```rust
use dream_soft_render::{Color, Error, SoftwareRenderer};

fn main() -> Result<(), Error> {
    let mut renderer = SoftwareRenderer::default();
    renderer.begin_frame(4, 4)?.clear(Color::from_rgb(9, 8, 7));

    // Same size: the last frame's pixels are kept.
    assert_eq!(
        renderer.begin_frame(4, 4)?.surface().pixels[..4],
        [9, 8, 7, 255]
    );
    // New size: transparent black.
    assert_eq!(
        renderer.begin_frame(8, 2)?.surface().pixels[..4],
        [0, 0, 0, 0]
    );
    Ok(())
}
```

`clear` writes its color into every pixel as it is, alpha included. It does not blend.

## Rectangles

`fill_rect` blends one color over a rectangle. `textured_rect` blends a texture over it, mapping
the UV rectangle `uv.min` to the rectangle's top-left corner and `uv.max` to its bottom-right, each
texel multiplied by `tint`. `Color::WHITE` leaves the texels as they are; any other tint colors
them, and a translucent one fades them.

```rust
use dream_soft_render::{ClipRect, Color, Error, Rect, SoftwareRenderer};

fn main() -> Result<(), Error> {
    let mut renderer = SoftwareRenderer::default();
    // Two texels, red then blue.
    let texture =
        renderer.create_texture(2, 1, &[255, 0, 0, 255, 0, 0, 255, 255])?;

    let mut frame = renderer.begin_frame(2, 1)?;
    // A UV rectangle whose min is right of its max mirrors the texture.
    frame.textured_rect(
        Rect::from_min_max([0.0, 0.0], [2.0, 1.0]),
        Rect::from_min_max([1.0, 0.0], [0.0, 1.0]),
        texture,
        Color::WHITE,
        ClipRect::ALL,
    )?;
    assert_eq!(frame.surface().pixels, [0, 0, 255, 255, 255, 0, 0, 255]);
    Ok(())
}
```

`Rect::from_min_size` and `Rect::from_min_max` build rectangles; `Rect::FULL_UV` is the whole
texture. A rectangle covers the pixels whose centers are inside it, `min` exclusive and `max`
inclusive, so rectangles on whole-pixel coordinates cover exactly the pixels you would expect,
and two rectangles that share an edge do not overlap. [The rules](@/docs/rules.md#which-pixels-a-shape-covers)
has the exact test.

A rectangle is drawn as two triangles laid out the way egui lays out its own, which the
rasterizer recognizes and fills as spans. A `fill_rect` and the same rectangle from egui give the
same bytes.

## Meshes

Everything that is not an axis-aligned rectangle is a mesh: vertices, and indices that take them
three at a time.

```rust
use dream_soft_render::{
    ClipRect, Color, Error, Mesh, SoftwareRenderer, Vertex,
};

fn main() -> Result<(), Error> {
    let mut renderer = SoftwareRenderer::default();
    let mut frame = renderer.begin_frame(64, 64)?;
    frame.clear(Color::BLACK);

    // One color per corner: the colors blend across both triangles.
    let vertices = [
        Vertex::new([8.0, 8.0], [0.0, 0.0], Color::from_rgb(255, 0, 0)),
        Vertex::new([56.0, 8.0], [0.0, 0.0], Color::from_rgb(0, 255, 0)),
        Vertex::new([8.0, 56.0], [0.0, 0.0], Color::from_rgb(0, 0, 255)),
        Vertex::new([56.0, 56.0], [0.0, 0.0], Color::WHITE),
    ];
    frame.mesh(
        Mesh {
            vertices: &vertices,
            indices: &[0, 1, 2, 2, 1, 3],
            texture: None,
        },
        ClipRect::ALL,
    )?;
    Ok(())
}
```

- `Vertex::new(pos, uv, color)`: a position in surface pixels, a texture coordinate, and a
  premultiplied color that multiplies the texel under it.
- `Mesh::texture` is `Some(id)` to sample a texture at each vertex's `uv`, or `None` to draw the
  vertex colors alone. With `None`, `uv` is ignored but must still be finite.
- The slices are read where they are, for the length of the call. Nothing is copied and nothing
  is kept, so a mesh you rebuild every frame can live in one `Vec` you clear and refill.

`Vertex` is `#[repr(C)]`, 20 bytes: position, then texture coordinate, then color. egui's vertex
has the same layout, so vertex data from egui or anything else with that layout can be drawn
without conversion.

## Clip rectangles

Every draw call takes a `ClipRect`, in whole pixels, and draws only inside it. Use
`ClipRect::ALL` for no clipping beyond the surface. A scrolling list clips its rows to its
viewport:

```rust
use dream_soft_render::{ClipRect, Color, Error, Rect, SoftwareRenderer};

fn main() -> Result<(), Error> {
    let mut renderer = SoftwareRenderer::default();
    let mut frame = renderer.begin_frame(100, 100)?;
    frame.clear(Color::BLACK);

    let viewport = ClipRect::new(10, 10, 90, 50); // x 10..90, y 10..50
    let scroll = 7.0;
    for row in 0..8 {
        let top = 10.0 + row as f32 * 12.0 - scroll;
        frame.fill_rect(
            Rect::from_min_size([10.0, top], [80.0, 10.0]),
            Color::from_rgb(40, 44, 58),
            viewport,
        )?;
    }

    let pixel = |x: usize, y: usize| {
        &frame.surface().pixels[(y * 100 + x) * 4..][..4]
    };
    assert_eq!(pixel(20, 12), [40, 44, 58, 255]); // inside the viewport
    assert_eq!(pixel(20, 60), [0, 0, 0, 255]); // a row below it, clipped away
    Ok(())
}
```

`ClipRect::new(min_x, min_y, max_x, max_y)` keeps `min <= x < max`. Bounds past the surface are
clamped. `ClipRect::default()` keeps nothing at all, which is rarely what you meant.

## Order

Calls draw in the order you make them, each blended over what is already there. There is no depth
buffer and no sorting: to put something on top, draw it last.

## When a call fails

A call that returns an error has drawn nothing, and the frame is still usable: fix the input, or
skip it, and carry on drawing. The errors each call can return are listed with it in the
[API reference](@/docs/api/renderer.md#frame), and [Errors](@/docs/api/errors.md) says what each
one means. A mesh is checked completely before any of it is drawn, so one bad index does not leave
half a mesh on the surface.

## Drawing next to egui

`Frame` and [`render_egui`](@/docs/egui.md) draw into the same surface with the same textures. A
frame drawn with `begin_frame` makes the next egui frame rasterize in full, even if egui's output
did not change, because the surface no longer holds what egui drew. `render_egui` does not clear
the surface either: egui's panels paint their own background, and anything they leave uncovered
keeps what was there.
