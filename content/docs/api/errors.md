+++
title = "Errors"
description = "Error and VertexField: every variant, which calls return it, and the text it displays."
weight = 30

[extra]
kind = "api"
+++

## Error

{{ api_signature(value="#[non_exhaustive] pub enum Error") }}

Why a call was refused. A call that returns an `Error` has drawn and changed nothing. `Clone`,
`Debug`, `PartialEq`, `Eq`, `Display`, `std::error::Error`. It is `#[non_exhaustive]`: match it
with a wildcard arm.

| Variant | Returned by | When | Displays |
|---|---|---|---|
| `SurfaceSize { width: usize, height: usize }` | `begin_frame` | The pixel count overflows or exceeds `MAX_SURFACE_PIXELS`, or a side is longer than 65535 pixels | `surface size 1281x720 overflows, exceeds the pixel budget, or is longer than 65535 pixels on a side` |
| `TextureSize { width: usize, height: usize }` | `create_texture` | A side is zero or longer than 65536 texels, or the byte size overflows | `texture size 0x4 is empty, overflows, or is longer than 65536 texels on a side` |
| `PixelDataLength { expected: usize, actual: usize }` | `create_texture`, `update_texture` | The pixel data is not `width × height × 4` bytes | `pixel data is 15 bytes, expected 16` |
| `TextureBudget { requested: usize, budget: usize }` | `create_texture` | The renderer's textures would total more than `MAX_TEXTURE_BYTES`. `requested` is that total | `texture storage would need 8388612 bytes, over the 8388608 byte budget` |
| `UnknownTexture(TextureId)` | `update_texture`, `free_texture`, `textured_rect`, `mesh` | The handle was freed, or another renderer issued it | `unknown or freed texture #6` |
| `TextureIdsExhausted` | `create_texture` | Every handle from 0 to `u64::MAX - 1` has been issued in this process | `every texture handle has been issued` |
| `TextureUpdateOutOfBounds(TextureId)` | `update_texture` | The region does not fit inside the texture, or its byte size overflows | `update region exceeds the bounds of texture #6` |
| `NonFiniteRect` | `fill_rect`, `textured_rect` | A coordinate of the rectangle or its UV range is NaN or infinite | `rectangle has a non-finite coordinate` |
| `NonFiniteVertex { index: usize, field: VertexField }` | `mesh` | Vertex `index` has a NaN or infinite position or texture coordinate | `mesh vertex 2 has a non-finite texture coordinate` |
| `IndexCount(usize)` | `mesh`; `render_egui`, inside an I/O error | The index count is not a multiple of three | `mesh has 5 indices, not a multiple of three` |
| `IndexOutOfRange { index: u32, vertex_count: usize }` | `mesh`; `render_egui`, inside an I/O error | An index is past the last vertex | `mesh index 3 is out of range for 3 vertices` |
| `Raster(String)` | The `From<std::io::Error>` conversion | Carries an I/O error's message | `rasterizer error: ` and the message |

The drawing API never returns `Raster` on 32- and 64-bit targets. The conversion exists so a
function that returns `Error` can use `?` on
[`render_egui`](@/docs/api/egui-adapter.md#render-egui), whose errors are `std::io::Error`.

The index errors reach `render_egui`'s caller wrapped in an `std::io::Error`, and can be taken
back out:

```rust
use dream_soft_render::Error;

fn describe(error: &std::io::Error) -> String {
    match error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<Error>())
    {
        Some(Error::IndexOutOfRange {
            index,
            vertex_count,
        }) => {
            format!("an egui mesh used vertex {index} of {vertex_count}")
        }
        Some(other) => other.to_string(),
        None => error.to_string(),
    }
}

fn main() {
    let error = std::io::Error::other(Error::IndexOutOfRange {
        index: 9,
        vertex_count: 4,
    });
    assert_eq!(describe(&error), "an egui mesh used vertex 9 of 4");
}
```

## VertexField

{{ api_signature(value="pub enum VertexField { Position, Uv }") }}

The part of a vertex `Error::NonFiniteVertex` names: `Position` for `Vertex::pos`, `Uv` for
`Vertex::uv`. A color cannot be invalid; every bit pattern is a color. `Clone`, `Copy`, `Debug`,
`PartialEq`, `Eq`, `Hash`, and `Display` as `position` or `texture coordinate`.
