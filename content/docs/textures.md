+++
title = "Textures"
description = "Creating, patching and freeing textures, premultiplying their pixels, the 8 MiB budget, and what a TextureId is."
weight = 40

[extra]
kind = "guide"
+++

A texture is premultiplied RGBA8 that the renderer owns. You hand it the pixels once, get a
`TextureId` back, and draw with the id.

```rust
use dream_soft_render::{Color, Error, SoftwareRenderer};

fn main() -> Result<(), Error> {
    let mut renderer = SoftwareRenderer::default();

    // Straight-alpha pixels, as an image decoder hands them over.
    // Premultiply them first.
    let straight: [[u8; 4]; 2] = [[255, 128, 0, 128], [255, 255, 255, 255]];
    let pixels: Vec<u8> = straight
        .iter()
        .flat_map(|&[r, g, b, a]| {
            Color::from_rgba_unmultiplied(r, g, b, a).to_array()
        })
        .collect();
    assert_eq!(pixels[..4], [128, 64, 0, 128]);

    let texture = renderer.create_texture(2, 1, &pixels)?;
    // ... draw with it ...
    renderer.free_texture(texture)?;
    Ok(())
}
```

## Premultiplied pixels

The renderer never premultiplies for you. PNG decoders and most image libraries produce straight
alpha, where a half-transparent orange is `(255, 128, 0, 128)`; the renderer wants
`(128, 64, 0, 128)`. Pass straight-alpha pixels unconverted and every translucent texel draws too
bright, with light fringes where it fades out. `Color::from_rgba_unmultiplied` does the
conversion, rounding to nearest, and `Color::to_array` gives the bytes back in texture order.

Fully opaque images are the same either way and need no conversion.

## Size and layout

`create_texture(width, height, pixels)` takes `width × height × 4` bytes, rows top to bottom, each
pixel red, green, blue, alpha. Any other length is `Error::PixelDataLength`. A side of zero, or
longer than 65536 texels, is `Error::TextureSize`. The pixels are copied, so the slice is yours
again when the call returns.

Sampling is nearest-texel, with the UV rectangle `(0, 0)` to `(1, 1)` covering the whole
texture; [The rules](@/docs/rules.md#sampling) has the exact texel formula. Drawn at its own size
with `Rect::FULL_UV`, a texture comes out texel for texel.

## Patching a texture

`update_texture` replaces a rectangle of an existing texture: `width × height` texels whose
top-left is `(x, y)`, from `width × height × 4` bytes.

```rust
use dream_soft_render::{Error, SoftwareRenderer};

fn main() -> Result<(), Error> {
    let mut renderer = SoftwareRenderer::default();
    let atlas = renderer.create_texture(4, 4, &[0; 64])?;

    // Write one opaque white texel at (2, 1).
    renderer.update_texture(atlas, 2, 1, 1, 1, &[255, 255, 255, 255])?;

    // A region that runs off the texture is refused, and nothing is written.
    let error = renderer
        .update_texture(atlas, 3, 3, 2, 2, &[0; 16])
        .unwrap_err();
    assert_eq!(error, Error::TextureUpdateOutOfBounds(atlas));
    Ok(())
}
```

An empty region, zero wide or zero high, inside the texture is allowed and changes nothing. This
is how a glyph cache grows: one atlas texture, patched as new glyphs are rasterized.

## Freeing, and handles

`free_texture` releases a texture's pixels and returns its bytes to the budget. The id is dead
afterwards: drawing with it, updating it or freeing it again is `Error::UnknownTexture`.

Ids are unique across the whole process and never reused. Two renderers never hand out the same
id, so an id from one renderer is `UnknownTexture` to every other, instead of quietly naming
whatever texture the other holds under the same number. The ids run from 0 up to
`u64::MAX - 1`; after that, `create_texture` fails with `Error::TextureIdsExhausted`, which at a
billion textures a second takes about 585 years.

`TextureId` is `Copy`, `Eq`, `Ord` and `Hash`, so it can key your own maps, and it displays as
`#` and its number.

## The budget

One renderer holds at most `MAX_TEXTURE_BYTES`, 8 MiB, of texture pixels: yours and, with the
`egui` feature, egui's font atlas and images. A texture that would take the total past it is
`Error::TextureBudget`, which reports the total the call would have needed. Freeing makes room
again. Two 1024x1024 textures fill the budget exactly, 4 MiB each.

The budget is there to catch a texture made every frame and never freed before it takes a
handheld's memory, not because more would not draw.

## Textures in egui

With the `egui` feature, egui meshes can sample your textures: pass
`egui_adapter::egui_texture_id(id)` wherever egui wants a `TextureId`, such as `egui::Image`.
[Hosting egui](@/docs/egui.md#your-textures-in-egui) shows it.

Creating, updating or freeing a texture makes the next egui frame rasterize in full, since what
the surface shows may no longer match the textures.
