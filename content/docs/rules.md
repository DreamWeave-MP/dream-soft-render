+++
title = "The rules"
description = "The contract every drawing path follows: the surface, coordinates, which pixels a shape covers, clipping, color, blending, sampling, limits and failures."
weight = 20

[extra]
kind = "reference"
+++

Every drawing path follows these rules, egui frames included. They are why the same draw calls
give the same bytes everywhere: none of them leaves room for a platform, a driver or a SIMD width
to decide. If the code and this page ever disagree, the code is wrong.

## The surface

`SoftwareSurface::pixels` is row-major RGBA8, top row first: exactly `width × height × 4` bytes,
red, green, blue, alpha for each pixel.

- At most [`MAX_SURFACE_PIXELS`](@/docs/api/renderer.md#limits) pixels, 921,600 (1280x720), and
  at most 65535 on a side. Anything bigger is `Error::SurfaceSize`.
- It is an opaque framebuffer. Every blended pixel is written with alpha 255, so the alpha
  channel does not carry coverage.
- A new surface, or one whose width or height changed, starts transparent black: every byte 0.
  Otherwise a frame starts with whatever the last one left. Nothing is cleared unless you call
  `Frame::clear`, and `clear` overwrites; it does not blend.

## Coordinates

Positions are `f32` surface pixels. The origin is the top-left corner, x grows to the right and y
grows down. Pixel `(x, y)` is the square `[x, x + 1) × [y, y + 1)`, and every test of whether a
shape covers it is made at its center, `(x + 0.5, y + 0.5)`.

## Which pixels a shape covers

**Rectangles** (`fill_rect`, `textured_rect`) cover the pixels whose centers satisfy
`min < center <= max` on both axes. A rectangle with `max <= min` on either axis covers nothing.
Along one row:

| Rectangle's x range | Covers |
|---|---|
| `0.0` to `1.0` | pixel 0 |
| `0.4` to `0.6` | pixel 0: its center, 0.5, is inside |
| `0.49` to `0.5` | pixel 0: `max` is inclusive |
| `0.5` to `0.5000001` | nothing: `min` is exclusive |
| `0.5` to `2.5` | pixels 1 and 2 |

**Triangles** cover the pixels whose centers they contain, whichever way they wind. A center
exactly on an edge goes to one side of it by a fixed tie-break, so two triangles that share an
edge never both blend a pixel on it: a translucent quad split along its diagonal has no darker
seam. A triangle with no area draws nothing.

```rust
use dream_soft_render::{
    ClipRect, Color, Error, Mesh, SoftwareRenderer, Vertex,
};

fn main() -> Result<(), Error> {
    let mut renderer = SoftwareRenderer::default();
    let mut frame = renderer.begin_frame(16, 16)?;
    frame.clear(Color::BLACK);

    // Half-transparent red, as two triangles whose shared diagonal runs
    // through pixel centers.
    let red = Color::from_rgba_premultiplied(100, 0, 0, 100);
    let corner = |x: f32, y: f32| Vertex::new([x, y], [0.0, 0.0], red);
    let vertices = [
        corner(0.0, 0.0),
        corner(16.0, 0.0),
        corner(0.0, 16.0),
        corner(16.0, 16.0),
    ];
    frame.mesh(
        Mesh {
            vertices: &vertices,
            indices: &[0, 1, 3, 0, 3, 2],
            texture: None,
        },
        ClipRect::ALL,
    )?;

    // Every pixel was blended exactly once: 100 + round(0 * 155 / 255) = 100.
    assert!(
        frame
            .surface()
            .pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [100, 0, 0, 255])
    );
    Ok(())
}
```

## Clipping

Every draw call takes a `ClipRect`: integer pixels, half-open, keeping `min_x <= x < max_x` and
`min_y <= y < max_y`. Bounds past the surface are clamped to it, and `min >= max` on either axis
keeps nothing.

`ClipRect::ALL` clips to the surface and nothing else. `ClipRect::default()` is `0, 0, 0, 0`,
which keeps nothing: a draw through it succeeds and writes no pixel.

## Color

`Color` is RGBA8 with **premultiplied** alpha: red, green and blue are already scaled by
`alpha / 255`. Texture pixels are premultiplied too. Opaque colors are the same either way; for
translucent ones, `Color::from_rgba_unmultiplied` converts straight alpha, per channel
`(channel × alpha + 127) / 255`.

A channel above its alpha is allowed and adds light, as it does in egui: `(100, 50, 0, 1)` over
`(200, 200, 200)` gives `(255, 249, 199)`.

A vertex color multiplies the texel sampled under it, per channel, `round(vertex × texel / 255)`.
Across a triangle, vertex colors interpolate linearly; untextured meshes multiply opaque white,
so they draw the vertex colors alone.

## Blending

Premultiplied source-over, onto an opaque destination. Per color channel:

```text
dst = min(255, src + round(dst × (255 − src_alpha) / 255))
```

and the alpha byte is written as 255. A source with alpha 0 leaves the destination alone, even if
its color channels are not 0. A source with alpha 255 replaces the destination.

| Source | Destination | Result |
|---|---|---|
| `(128, 0, 0, 128)` | `(0, 0, 255, 255)` | `(128, 0, 127, 255)` |
| `(100, 50, 0, 128)` | `(0, 0, 0, 0)`, cleared transparent | `(100, 50, 0, 255)` |
| `(100, 50, 0, 0)` | `(200, 200, 200, 255)` | unchanged |

## Sampling

Nearest texel, no filtering. UV `(0, 0)` is the texture's top-left corner and `(1, 1)` its
bottom-right. Coordinates are clamped to that range, and the texel on each axis is
`round(uv × (size − 1))`, halves rounding up.

Stretching a 4-texel row over 8 pixels with `Rect::FULL_UV` samples texels
`0, 1, 1, 1, 2, 2, 2, 3`: the end texels get half the pixels of the middle ones. Drawn at its own
size with `FULL_UV`, a texture comes out texel for texel.

```rust
use dream_soft_render::{ClipRect, Color, Error, Rect, SoftwareRenderer};

fn main() -> Result<(), Error> {
    let mut renderer = SoftwareRenderer::default();
    // Seven texels whose red channel is 0, 40, 80, ... 240.
    let texels: Vec<u8> =
        (0..7u8).flat_map(|x| [x * 40, 0, 0, 255]).collect();
    let texture = renderer.create_texture(7, 1, &texels)?;

    let mut frame = renderer.begin_frame(7, 1)?;
    frame.textured_rect(
        Rect::from_min_max([0.0, 0.0], [7.0, 1.0]),
        Rect::FULL_UV,
        texture,
        Color::WHITE,
        ClipRect::ALL,
    )?;
    assert_eq!(frame.surface().pixels, texels);
    Ok(())
}
```

## Determinism

The output is a function of the draw calls, the textures and the surface's previous contents.
Nothing else goes in: not the platform, not the CPU's SIMD, not timing. The NEON code on AArch64
writes exactly the bytes the portable code writes; [Platforms](@/docs/platforms.md) lists where
each runs and how that is checked.

egui is not quite as strict about its own output. The discs it draws into its font atlas and its
shadow easing call `powf` from the platform's C math library. glibc and musl agree on the golden
scenes, but if a hash of an egui frame fails on one operating system only, check what egui handed
the renderer before blaming the renderer.

## Limits

| Limit | Value | Past it |
|---|---|---|
| Surface pixels, `MAX_SURFACE_PIXELS` | 921,600 (1280x720) | `Error::SurfaceSize` |
| Surface side | 65535 pixels | `Error::SurfaceSize` |
| Texture bytes per renderer, `MAX_TEXTURE_BYTES` | 8 MiB, egui's textures included | `Error::TextureBudget` |
| Texture side | 65536 texels | `Error::TextureSize` |

The pixel and byte budgets are there to catch accidents on small machines: a desktop-sized
surface on a handheld, a texture uploaded every frame and never freed. The two side limits are
the rasterizer's own. It computes pixel and texel positions in `f32` through a 16-bit clamp, and
past those sizes the positions would be wrong.

## Failures

Malformed input is an error, and the call that gets it draws nothing:

- a mesh whose index count is not a multiple of three, or that has an index past its vertices;
- a vertex with a NaN or infinite position or texture coordinate, referenced or not;
- a rectangle with a NaN or infinite coordinate, in its position or its UV range;
- pixel data of the wrong length, a texture of zero size, a texture update outside its texture;
- a texture handle that was freed, or belongs to another renderer.

The renderer does not clip bad input into something drawable and carry on. Code that feeds it
garbage has a bug, and a quietly wrong frame would only hide where. Every error and when it
happens is on the [Errors](@/docs/api/errors.md) page; egui frames fail with the same checks,
described in [Hosting egui](@/docs/egui.md#when-a-frame-fails).
