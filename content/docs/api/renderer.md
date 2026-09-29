+++
title = "Renderer and frames"
description = "SoftwareRenderer, Frame, Mesh, SoftwareSurface, and the two limits: every method, what it does, and how it fails."
weight = 10

[extra]
kind = "api"
+++

## SoftwareRenderer

{{ api_signature(value="pub struct SoftwareRenderer") }}

A CPU rasterizer that owns one surface and a texture store. `Debug`, `Default`. Make one with
`SoftwareRenderer::default()`; it starts with an empty 0x0 surface and no textures. It is `Send`
and `Sync`.

Drawing through [`begin_frame`](#begin-frame) and, with the `egui` feature,
[`render_egui`](@/docs/api/egui-adapter.md#render-egui) write the same surface and share the same
textures.

### surface

{{ api_signature(value="pub const fn surface(&self) -> &SoftwareSurface") }}

The surface as the last frame left it.

### begin_frame

{{ api_signature(value="pub fn begin_frame(&mut self, width: usize, height: usize) -> Result<Frame<'_>, Error>") }}

Starts drawing into a `width`x`height` surface. If the size is the one the surface already has,
its pixels are kept. If not, it is resized and starts transparent black. Nothing else is cleared.
The next egui frame after it rasterizes in full.

| Error | When |
|---|---|
| `SurfaceSize` | `width × height` overflows or is more than `MAX_SURFACE_PIXELS`, or either side is longer than 65535 pixels. The surface is unchanged. |

A size of zero on either side is allowed. The surface is empty, and draw calls check their input
as usual and write nothing.

### create_texture

{{ api_signature(value="pub fn create_texture(&mut self, width: usize, height: usize, pixels: &[u8]) -> Result<TextureId, Error>") }}

Stores a copy of `pixels` as a `width`x`height` texture and returns its handle. `pixels` is
premultiplied RGBA8, `width × height × 4` bytes, top row first. See [Textures](@/docs/textures.md).

| Error | When |
|---|---|
| `TextureSize` | A side is zero or longer than 65536 texels, or the byte size overflows |
| `PixelDataLength` | `pixels` is not `width × height × 4` bytes |
| `TextureBudget` | The renderer's textures would total more than `MAX_TEXTURE_BYTES` |
| `TextureIdsExhausted` | The process has issued every handle |

### update_texture

{{ api_signature(value="pub fn update_texture(&mut self, texture: TextureId, x: usize, y: usize, width: usize, height: usize, pixels: &[u8]) -> Result<(), Error>") }}

Replaces the `width`x`height` region of `texture` whose top-left texel is `(x, y)` with `pixels`,
premultiplied RGBA8, top row first. An empty region inside the texture changes nothing.

Checked in this order, and nothing is written when any of them is returned:

| Error | When |
|---|---|
| `TextureUpdateOutOfBounds` | `width × height × 4` overflows |
| `PixelDataLength` | `pixels` is not `width × height × 4` bytes |
| `UnknownTexture` | The handle was freed or belongs to another renderer |
| `TextureUpdateOutOfBounds` | The region does not fit inside the texture |

### free_texture

{{ api_signature(value="pub fn free_texture(&mut self, texture: TextureId) -> Result<(), Error>") }}

Releases the texture and returns its bytes to the budget. The handle is invalid afterwards, and
never issued again.

| Error | When |
|---|---|
| `UnknownTexture` | The handle was already freed, or belongs to another renderer |

Creating, updating and freeing a texture make the next egui frame rasterize in full.

### The egui methods

With the `egui` feature. See [egui adapter](@/docs/api/egui-adapter.md).

## Frame

{{ api_signature(value="pub struct Frame<'a>") }}

One frame of drawing, from `begin_frame`. It borrows the renderer mutably; drop it to finish.
`Debug`. Draw calls rasterize before they return, in call order, each blended over what is
already there; [The rules](@/docs/rules.md) say exactly how.

### width and height

{{ api_signature(value="pub const fn width(&self) -> usize") }}
{{ api_signature(value="pub const fn height(&self) -> usize") }}

The surface's size in pixels.

### surface

{{ api_signature(value="pub const fn surface(&self) -> &SoftwareSurface") }}

The pixels drawn so far.

### clear

{{ api_signature(value="pub fn clear(&mut self, color: Color)") }}

Writes `color` into every pixel, alpha included, without blending. It cannot fail.

### fill_rect

{{ api_signature(value="pub fn fill_rect(&mut self, rect: Rect, color: Color, clip: ClipRect) -> Result<(), Error>") }}

Blends `color` over the pixels whose centers lie in `rect`: `min < center <= max` on each axis. A
rectangle with `max <= min` on either axis covers nothing.

| Error | When |
|---|---|
| `NonFiniteRect` | A coordinate of `rect` is NaN or infinite |

### textured_rect

{{ api_signature(value="pub fn textured_rect(&mut self, rect: Rect, uv: Rect, texture: TextureId, tint: Color, clip: ClipRect) -> Result<(), Error>") }}

Blends `texture` over `rect`, mapping `uv.min` to the rectangle's top-left corner and `uv.max` to
its bottom-right, each texel multiplied by the premultiplied `tint`. `Color::WHITE` leaves the
texels unchanged. Coverage is as for `fill_rect`, and a UV rectangle with `min` past `max`
mirrors the texture.

| Error | When |
|---|---|
| `NonFiniteRect` | A coordinate of `rect` or `uv` is NaN or infinite. Checked first |
| `UnknownTexture` | The handle was freed or belongs to another renderer, even when nothing would be covered |

### mesh

{{ api_signature(value="pub fn mesh(&mut self, mesh: Mesh<'_>, clip: ClipRect) -> Result<(), Error>") }}

Blends a triangle mesh over the surface. A pixel is covered when its center is inside a
triangle; centers on a shared edge go to exactly one of the two triangles. Vertex colors
interpolate across each triangle, and texels are sampled nearest-texel.

The whole mesh is checked before anything is drawn, in this order:

| Error | When |
|---|---|
| `IndexCount` | The index count is not a multiple of three |
| `IndexOutOfRange` | An index is past the last vertex; the first such index is reported |
| `NonFiniteVertex` | A vertex, referenced by an index or not, has a NaN or infinite position or texture coordinate; the first such vertex is reported |
| `UnknownTexture` | `mesh.texture` was freed or belongs to another renderer |

## Mesh

{{ api_signature(value="pub struct Mesh<'a> { pub vertices: &'a [Vertex], pub indices: &'a [u32], pub texture: Option<TextureId> }") }}

Triangles for `Frame::mesh`. Every three `indices` name one triangle's vertices. `texture` is
sampled at each vertex's `uv`; `None` draws the vertex colors alone. The slices are read in place
during the call; nothing is copied or kept. `Clone`, `Copy`, `Debug`.

## SoftwareSurface

{{ api_signature(value="pub struct SoftwareSurface { pub width: usize, pub height: usize, pub pixels: Vec<u8> }") }}

The pixels. `pixels` is `width × height × 4` bytes of RGBA8, top row first, rows packed with no
padding. `Debug`, `Default`.

The renderer hands it out by shared reference only. The fields are public so you can read them,
and so a host can build a surface of its own, for a test or a copy; a surface you build is never
drawn into.

## Limits

{{ api_signature(value="pub const MAX_SURFACE_PIXELS: usize = 1280 * 720") }}

921,600: the most pixels a surface may have, whatever its shape. Enough for 640x480 and 1280x720
handheld screens, and a refusal for an accidental desktop-sized one.

{{ api_signature(value="pub const MAX_TEXTURE_BYTES: usize = 8 * 1024 * 1024") }}

8,388,608: the most texture bytes one renderer may hold, egui's font atlas and images included.

Two more limits are not constants: a surface side is at most 65535 pixels, and a texture side at
most 65536 texels. Past them the rasterizer's `f32` pixel and texel coordinates would be wrong.
