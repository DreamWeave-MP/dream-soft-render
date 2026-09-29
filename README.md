# dream-soft-render

A CPU rasterizer for 2D triangle meshes and [egui](https://github.com/emilk/egui) frames. It
writes premultiplied RGBA8 into a buffer you own, and the same draw calls produce the same bytes
on every platform, AArch64's NEON code included.

There is no GPU here, no window and no swapchain. You hand it rectangles, textured rectangles and
meshes, or an egui UI closure, and you get pixels back. Whether they go to a Linux framebuffer, a
PNG or a golden-test hash is up to you. It was pulled out of
[dream-ini](https://github.com/DreamWeave-MP/dream-ini), whose PortMaster build draws its whole
egui interface through it on handhelds with no usable GPU.

**Documentation, including the full API reference:
<https://dreamweave-mp.github.io/dream-soft-render/>**

## Install

```sh
cargo add dream-soft-render
```

Rust 1.92 or newer. Without features the crate has no dependencies. The egui adapter is the `egui`
feature:

```toml
[dependencies]
dream-soft-render = { version = "1", features = ["egui"] }
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

Each call rasterizes before it returns, in call order. `begin_frame` does not clear: call `clear`
for a background. Colors and texture pixels are premultiplied, pixels are sampled at their
centers, and bad input, such as an index past the vertices or a NaN rectangle, is an error rather
than a quietly wrong frame. [The rules](https://dreamweave-mp.github.io/dream-soft-render/docs/rules/)
state all of it exactly.

## Drawing egui

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

`render_egui` needs the `egui` feature. It runs the closure, applies egui's texture uploads,
tessellates at one pixel per point and rasterizes into the same surface. A frame identical to the
last one is skipped, and `surface_changed` says so, so the host can skip presenting it too.

## Where to read next

- [Start here](https://dreamweave-mp.github.io/dream-soft-render/docs/start-here/): draw a frame
  and save it as an image
- [Hosting egui](https://dreamweave-mp.github.io/dream-soft-render/docs/egui/): textures,
  unchanged frames, feathering and failures
- [Fast frames](https://dreamweave-mp.github.io/dream-soft-render/docs/performance/): what the
  rasterizer recognizes, and the benchmarks
- [Platforms](https://dreamweave-mp.github.io/dream-soft-render/docs/platforms/): the NEON
  kernels, and testing them under qemu
- [Rust API](https://dreamweave-mp.github.io/dream-soft-render/docs/api/) and the
  [changelog](https://dreamweave-mp.github.io/dream-soft-render/home/changelog/)

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in this crate by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.

## Support

Has dream-soft-render been useful to you? Consider
[amplifying the signal](https://ko-fi.com/magicaldave) through ko-fi.
