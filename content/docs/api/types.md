+++
title = "Geometry and color"
description = "Color, Vertex, Rect, ClipRect and TextureId: fields, constructors, constants and layouts."
weight = 20

[extra]
kind = "api"
+++

## Color

{{ api_signature(value="#[repr(C, align(4))] pub struct Color { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }") }}

An RGBA8 color with **premultiplied** alpha: `r`, `g` and `b` are already scaled by `a / 255`.
Four bytes, four-byte aligned. `Clone`, `Copy`, `Debug`, `Default` (transparent), `PartialEq`,
`Eq`, `Hash`. A channel above `a` is allowed and adds light when blended.

| Item | |
|---|---|
| `const TRANSPARENT: Color` | `(0, 0, 0, 0)` |
| `const BLACK: Color` | `(0, 0, 0, 255)` |
| `const WHITE: Color` | `(255, 255, 255, 255)` |
| `const fn from_rgba_premultiplied(r: u8, g: u8, b: u8, a: u8) -> Color` | The channels as they are |
| `const fn from_rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Color` | Premultiplies straight alpha: each of `r`, `g`, `b` becomes `(channel × a + 127) / 255` |
| `const fn from_rgb(r: u8, g: u8, b: u8) -> Color` | Opaque |
| `const fn to_array(self) -> [u8; 4]` | `[r, g, b, a]`, the order of surface and texture bytes |
| `const fn to_packed(self) -> u32` | Red in bits 0-7, green 8-15, blue 16-23, alpha 24-31: `0xAABBGGRR`, the same number on every host |
| `const fn from_packed(packed: u32) -> Color` | The reverse. Every `u32` is a color |

```rust
use dream_soft_render::Color;

fn main() {
    let orange = Color::from_rgba_unmultiplied(255, 128, 0, 128);
    assert_eq!(orange.to_array(), [128, 64, 0, 128]);
    assert_eq!(orange.to_packed(), 0x8000_4080);
    assert_eq!(Color::from_packed(0x8000_4080), orange);
}
```

## Vertex

{{ api_signature(value="#[repr(C)] pub struct Vertex { pub pos: [f32; 2], pub uv: [f32; 2], pub color: Color }") }}

One corner of a triangle. `Clone`, `Copy`, `Debug`, `Default`, `PartialEq`.

| Field | |
|---|---|
| `pos` | Position in surface pixels, `[x, y]`, y down |
| `uv` | Texture coordinate, `[0.0, 0.0]` top-left to `[1.0, 1.0]` bottom-right. Must be finite even when the mesh has no texture |
| `color` | Premultiplied; multiplies the texel sampled under it, and interpolates across the triangle |

{{ api_signature(value="pub const fn new(pos: [f32; 2], uv: [f32; 2], color: Color) -> Vertex") }}

The layout is fixed at 20 bytes: `pos` at offset 0, `uv` at 8, `color` at 16. egui's
`epaint::Vertex` has the same layout, and with the `egui` feature the crate fails to compile if a
future egui changes it, because it reads egui's vertex buffers in place.

## Rect

{{ api_signature(value="pub struct Rect { pub min: [f32; 2], pub max: [f32; 2] }") }}

An axis-aligned rectangle in surface pixels, or in texture coordinates for a UV range. `min` is
the top-left corner and `max` the bottom-right. `Clone`, `Copy`, `Debug`, `Default` (all zero,
which covers nothing), `PartialEq`.

| Item | |
|---|---|
| `const FULL_UV: Rect` | `[0.0, 0.0]` to `[1.0, 1.0]`: the whole texture |
| `const fn from_min_max(min: [f32; 2], max: [f32; 2]) -> Rect` | From two corners |
| `fn from_min_size(min: [f32; 2], size: [f32; 2]) -> Rect` | From the top-left corner and a size: `max = min + size` |

## ClipRect

{{ api_signature(value="pub struct ClipRect { pub min_x: u32, pub min_y: u32, pub max_x: u32, pub max_y: u32 }") }}

A clip rectangle in whole surface pixels, half-open: it keeps pixels with `min_x <= x < max_x`
and `min_y <= y < max_y`. Bounds past the surface are clamped to it; `min >= max` on either axis
keeps nothing. `Clone`, `Copy`, `Debug`, `Default`, `PartialEq`, `Eq`, `Hash`.

| Item | |
|---|---|
| `const ALL: ClipRect` | `0, 0, u32::MAX, u32::MAX`: the whole surface |
| `const fn new(min_x: u32, min_y: u32, max_x: u32, max_y: u32) -> ClipRect` | From its bounds |

`ClipRect::default()` is `0, 0, 0, 0` and keeps nothing. Use `ClipRect::ALL` for no clipping.

## TextureId

{{ api_signature(value="pub struct TextureId(/* private */)") }}

A handle to a texture, from `SoftwareRenderer::create_texture`. `Clone`, `Copy`, `Debug`,
`PartialEq`, `Eq`, `Hash`, `PartialOrd`, `Ord`, and `Display` as `#` and its number.

Handles come from one counter for the whole process, starting at 0 and never reused: a freed
handle stays invalid, and one renderer's handle is `UnknownTexture` to every other renderer. You
cannot build one yourself. With the `egui` feature,
[`egui_texture_id`](@/docs/api/egui-adapter.md#egui-texture-id) turns one into the
`egui::TextureId` that egui meshes use to sample it.
