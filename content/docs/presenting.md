+++
title = "Presenting frames"
description = "Getting the surface's RGBA8 bytes onto a framebuffer, into a window library, into an image file, or into a test."
weight = 60

[extra]
kind = "guide"
+++

The renderer stops at `SoftwareSurface::pixels`: `width × height` pixels of four bytes, red,
green, blue, alpha, rows top to bottom, with no padding between rows. Getting them to a screen is
the host's job, and usually means reordering the bytes.

## Only when something changed

With egui, present a frame only when `RenderOutcome::surface_changed` is true. A skipped frame
left the surface exactly as it was, so the screen already shows it. With `Frame`, you know what
you drew.

## Framebuffers

A Linux framebuffer (`/dev/fb0`) says its pixel layout in `fb_var_screeninfo`, and handhelds
disagree. The common 32-bit one is XRGB8888 in little-endian memory: blue, green, red, then an
unused byte. Each row may also be longer than `width × 4` bytes; the framebuffer's `line_length`
says by how much.

```rust
use dream_soft_render::SoftwareSurface;

/// Copies the surface into a 32-bit little-endian XRGB8888 framebuffer
/// mapping whose rows are `line_length` bytes apart.
fn blit_xrgb8888(
    surface: &SoftwareSurface,
    framebuffer: &mut [u8],
    line_length: usize,
) {
    let row_bytes = surface.width * 4;
    for (source_row, target_row) in surface
        .pixels
        .chunks_exact(row_bytes)
        .zip(framebuffer.chunks_mut(line_length))
    {
        for (source, target) in source_row
            .chunks_exact(4)
            .zip(target_row.chunks_exact_mut(4))
        {
            target
                .copy_from_slice(&[source[2], source[1], source[0], 0xff]);
        }
    }
}

fn main() {
    let surface = SoftwareSurface {
        width: 1,
        height: 1,
        pixels: vec![10, 20, 30, 255],
    };
    let mut framebuffer = vec![0; 8]; // one pixel, and a line_length of 8
    blit_xrgb8888(&surface, &mut framebuffer, 8);
    assert_eq!(framebuffer[..4], [30, 20, 10, 255]);
}
```

For a 16-bit RGB565 framebuffer, keep the top five, six and five bits of red, green and blue.
dream-ini's PortMaster build accepts 16- and 32-bit framebuffers: it packs each pixel into the
red, green and blue bitfields the framebuffer reports, with a NEON copy for the common 32-bit
layouts.

## Window libraries

Libraries that take a `u32` per pixel, such as `softbuffer` and `minifb`, usually want
`0x00RRGGBB`:

```rust
use dream_soft_render::SoftwareSurface;

fn to_0rgb(surface: &SoftwareSurface, out: &mut Vec<u32>) {
    out.clear();
    out.extend(surface.pixels.chunks_exact(4).map(|pixel| {
        u32::from_be_bytes([0, pixel[0], pixel[1], pixel[2]])
    }));
}

fn main() {
    let surface = SoftwareSurface {
        width: 1,
        height: 1,
        pixels: vec![0x12, 0x34, 0x56, 0xff],
    };
    let mut buffer = Vec::new();
    to_0rgb(&surface, &mut buffer);
    assert_eq!(buffer, [0x0012_3456]);
}
```

`Color::to_packed` is a different layout: red in the low byte, `0xAABBGGRR`, the same number
whatever the host's byte order.

## Image files

A surface is ready for any image encoder that takes RGBA8, such as the `png` crate. PNG stores
straight alpha, and the surface is premultiplied, but every pixel a draw call blended has alpha
255, where the two are the same. Only pixels no call touched, still transparent black, and pixels
`clear`ed to a translucent color differ; drop the alpha channel, or divide it back out, if your
frames have those. [Start here](@/docs/start-here.md) writes a PPM with no dependencies at all.

## Tests

The output is deterministic, so a test can pin a whole frame by its hash and fail on any changed
byte. The crate's own golden tests do this with FNV-1a over the surface:

```rust
use dream_soft_render::{ClipRect, Color, Error, Rect, SoftwareRenderer};

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn main() -> Result<(), Error> {
    let mut renderer = SoftwareRenderer::default();
    let mut frame = renderer.begin_frame(8, 8)?;
    frame.clear(Color::BLACK);
    frame.fill_rect(
        Rect::from_min_size([2.0, 2.0], [4.0, 4.0]),
        Color::from_rgba_unmultiplied(255, 140, 0, 128),
        ClipRect::ALL,
    )?;
    let hash = fnv1a(&frame.surface().pixels);
    println!("0x{hash:016x}");
    Ok(())
}
```

Record the hash once, after looking at the frame, and assert it from then on. A hash says a frame
changed, not how; when one fails, write both frames out as images and compare them. For egui
frames, see the caveat about egui's own math in [The rules](@/docs/rules.md#determinism).
