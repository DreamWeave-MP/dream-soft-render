# dream-soft-render

A CPU rasterizer for 2D triangle meshes and [egui](https://github.com/emilk/egui) frames. It
writes premultiplied RGBA8 into a buffer you own, and the same draw calls produce the same
bytes on every platform.

There is no GPU here, no window, and no swapchain. You hand it rectangles, textured rectangles,
and meshes, or an egui UI closure, and you get pixels back. Whether those pixels go to a Linux
framebuffer, a PNG, or a golden-test hash is up to you.

It was pulled out of [Dream-INI](https://github.com/DreamWeave-MP/dream-ini), whose PortMaster
build draws its whole egui interface through it on handhelds with no usable GPU and a small ARM
core that notices every wasted instruction.

```toml
[dependencies]
dream-soft-render = "0.1"
```

The crate has no egui dependency by default. The egui adapter is behind a feature:

```toml
[dependencies]
dream-soft-render = { version = "0.1", features = ["egui"] }
```

## Drawing

```rust
use dream_soft_render::{ClipRect, Color, Mesh, Rect, SoftwareRenderer, Vertex};

fn draw_status_panel(renderer: &mut SoftwareRenderer) -> Result<Vec<u8>, dream_soft_render::Error> {
    let icon = renderer.create_texture(2, 2, &[
        255, 255, 255, 255, 0, 0, 0, 255,
        0, 0, 0, 255, 255, 255, 255, 255,
    ])?;

    let mut frame = renderer.begin_frame(320, 240)?;
    frame.clear(Color::from_rgb(17, 20, 28));
    frame.fill_rect(
        Rect::from_min_size([8.0, 8.0], [304.0, 40.0]),
        Color::from_rgba_unmultiplied(80, 160, 255, 192),
        ClipRect::ALL,
    )?;
    frame.textured_rect(
        Rect::from_min_size([16.0, 16.0], [24.0, 24.0]),
        Rect::FULL_UV,
        icon,
        Color::WHITE,
        ClipRect::ALL,
    )?;

    let warning = Color::from_rgb(220, 40, 40);
    frame.mesh(
        Mesh {
            vertices: &[
                Vertex::new([160.0, 80.0], [0.0, 0.0], warning),
                Vertex::new([220.0, 180.0], [0.0, 0.0], warning),
                Vertex::new([100.0, 180.0], [0.0, 0.0], warning),
            ],
            indices: &[0, 1, 2],
            texture: None,
        },
        ClipRect::new(0, 0, 320, 160),
    )?;

    Ok(frame.surface().pixels.clone())
}
```

Draw calls rasterize immediately, in call order. There is no command list to flush, and no
deferred state that can disagree with what you asked for. `Frame::mesh` rasterizes straight out
of the slices you pass; the vertices are not copied into some internal format first, because
`Vertex` is the internal format.

`begin_frame` does not clear. The surface keeps the previous frame's pixels until you call
`clear`, except that a new or resized surface starts transparent black. If you want a
background, draw one.

## Drawing egui

This needs the `egui` feature.

```rust
use dream_soft_render::SoftwareRenderer;
use dream_soft_render::egui_adapter::{RenderFrame, egui};

fn render_ui(renderer: &mut SoftwareRenderer, context: &egui::Context) -> std::io::Result<bool> {
    let outcome = renderer.render_egui(640, 480, &RenderFrame::new(context), |ui| {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Import settings");
            ui.label("Choose Morrowind.ini to continue.");
        });
    })?;
    // false: the frame matched the previous one bit for bit and nothing was redrawn.
    Ok(outcome.surface_changed)
}
```

`render_egui` runs the closure, applies egui's texture uploads, tessellates at one pixel per
point, and rasterizes into the same surface `Frame` draws into. egui's vertex buffers are read
in place: egui's vertex has the same 20-byte layout as `Vertex`, compile-time assertions hold
the two to it, and the build fails if a future egui changes it. Textures from `create_texture`
show up in egui through `egui_adapter::egui_texture_id`.

egui repaints a lot: animations, cursor blinks, the extra layout passes it takes when widgets
change size. Many of those frames come out identical. When a frame's tessellated output matches
the previous one bit for bit and no texture changed, the rasterizer skips the frame and says so
through `surface_changed`, so the host can skip presenting it too. Dream-INI skips its
framebuffer blit that way.

egui paint callbacks are not supported. They exist to run GPU code, and there is no GPU.

## The Rules

The crate docs hold the full contract, and every drawing path follows it, egui frames included.
The short version:

- **Colors are premultiplied RGBA8.** So are texture pixels. `Color::from_rgba_unmultiplied`
  converts straight alpha. `Color::to_packed` has a fixed bit layout (red in the low byte), so
  host byte order never leaks into your data.
- **The surface is an opaque framebuffer.** Blending is premultiplied source-over and writes
  alpha 255. The alpha channel does not carry coverage.
- **Coordinates are pixels,** with the origin at the top-left and y pointing down. Pixels are
  sampled at their centers.
- **Clip rectangles are integer pixels,** half-open and clamped to the surface.
- **Sampling is nearest-texel.** There is no filtering.
- **Bad input is an error.** An index past the end of its vertices, pixel data of the wrong
  length, a NaN rectangle, a freed texture. The renderer does not clip garbage into something
  drawable and carry on, because a quietly wrong frame hides the bug that produced it.

Surfaces are capped at 1280x720 pixels and one renderer's textures at 8 MiB
(`MAX_SURFACE_PIXELS`, `MAX_TEXTURE_BYTES`). Those limits exist to catch accidents on small
machines, not because the rasterizer falls over past them.

## Where the Time Goes

Most of this crate is not the triangle rasterizer. It is the code that avoids needing it.

egui draws a UI as a pile of anonymous triangles. Treated that way, a 5-pixel glyph costs two
full triangle setups, edge functions, and per-pixel barycentric interpolation. The renderer
looks at each pair of triangles first and recognizes what they are: axis-aligned rectangles
(solid, textured, or glyph quads in egui's own vertex layout), triangle fans with one solid
color (rounded panels, filled shapes), and triangles whose texels are all the same. Those draw
as spans. Only what is left pays for general triangle rasterization.

On `AArch64`, the hot spans (glyph blending, translucent fills, textured rows) run NEON code.
It produces the same bytes as the portable code, and the tests hold it to that.

The one thing the renderer cannot skip is egui's shape anti-aliasing. egui calls it
*feathering*: it adds a one-pixel strip of translucent triangles along every shape edge, and
those triangles are thin, numerous, and cost the most per pixel of anything egui draws. On slow
CPUs, turn it off:

```rust
use dream_soft_render::egui_adapter::egui;

fn disable_feathering(context: &egui::Context) {
    context.tessellation_options_mut(|options| options.feathering = false);
}
```

Text keeps its anti-aliasing either way; that lives in the font atlas. Measured as `AArch64`
instructions per steady frame of the benchmark scenes (640x480, counted under qemu), turning
feathering off removes 44% of the work for a form, 17% for a text-heavy preview, and 35% for a
shadowed window. Instruction counts are not cycles; they leave out memory stalls. Measure on
the device before quoting a frame rate.

## Tests

```sh
cargo test --all-features
```

`--all-features` includes the egui adapter's tests. Beyond the unit tests, `tests/golden.rs`
renders three egui scenes (a form, a monospace
preview, a shadowed window) and `tests/api.rs` renders a scene through the plain drawing API.
Each is pinned by an FNV-1a hash of the whole surface. A change that moves one byte fails.
That is deliberate: "looks the same" is not a test.

The NEON paths only compile on `AArch64`, so test them there. No ARM machine is needed; qemu
user mode and Rust's bundled linker are enough:

```sh
rustup target add aarch64-unknown-linux-musl
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld \
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUNNER=qemu-aarch64-static \
cargo test --release --all-features --target aarch64-unknown-linux-musl
```

The golden hashes are the same on both architectures. If they are not, the NEON code is wrong.

## Benchmarks

```sh
cargo bench --features egui --bench frame
cargo bench --features egui --bench frame -- --no-feathering --scene form
```

The benchmark renders the golden scenes a few hundred times each and reports rasterization and
whole-frame times in microseconds. `--help` does not exist; the options are listed at the top of
`benches/frame.rs`.

Wall-clock numbers from a desktop x86 CPU tell you about a desktop x86 CPU. For the handhelds
this crate targets, compare `AArch64` instruction counts, or run it on the device.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in this crate by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.
