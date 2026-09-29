+++
title = "Hosting egui"
description = "Rendering egui frames with render_egui: what one call does, frames that did not change, textures, feathering, failures, and the diagnostics log."
weight = 50

[extra]
kind = "guide"
+++

With the `egui` feature, a `SoftwareRenderer` renders egui frames into the same surface
[`Frame`](@/docs/drawing.md) draws into.

```toml
[dependencies]
dream-soft-render = { version = "1", features = ["egui"] }
```

The crate re-exports the egui it is built against as `dream_soft_render::egui_adapter::egui`,
egui 0.34. Use it, or depend on the same egui version yourself; types from two versions of egui
do not mix.

## One frame

```rust
use dream_soft_render::SoftwareRenderer;
use dream_soft_render::egui_adapter::{RenderFrame, egui};

fn render_ui(
    renderer: &mut SoftwareRenderer,
    context: &egui::Context,
) -> std::io::Result<bool> {
    let outcome = renderer.render_egui(
        640,
        480,
        &RenderFrame::new(context),
        |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                ui.heading("Import settings");
                ui.label("Choose Morrowind.ini to continue.");
            });
        },
    )?;
    // false: the frame matched the previous one bit for bit, and nothing
    // was redrawn.
    Ok(outcome.surface_changed)
}

fn main() -> std::io::Result<()> {
    let mut renderer = SoftwareRenderer::default();
    let context = egui::Context::default();
    assert!(render_ui(&mut renderer, &context)?);
    assert!(!render_ui(&mut renderer, &context)?);
    Ok(())
}
```

One `render_egui(width, height, frame, run_ui)` call:

1. Sizes the surface, which starts transparent black if the size changed. It does not clear
   otherwise; egui's panels paint their own background.
2. Runs `run_ui` through `egui::Context::run_ui`, which calls it again when egui wants another
   layout pass. egui is told the screen is `width × height` points, and that the largest texture
   side is 1024 texels, which bounds its font atlas.
3. Applies egui's texture uploads: the font atlas, and images loaded through the context.
4. Tessellates egui's shapes at one pixel per point.
5. Rasterizes the meshes, unless the frame is the same as the last one.
6. Frees the textures egui released, and returns a `RenderOutcome`.

`render_egui` builds egui's `RawInput` itself, and it holds the screen size, the texture limit
and nothing else: no pointer, no keys, no time. A host with input handles it inside the UI closure.
dream-ini's PortMaster build does: it reads its controller there and moves focus and state
itself.

## Points and pixels

One egui point is one surface pixel, always. Leave egui's zoom factor and pixels per point at 1:
the frame is laid out and tessellated at one pixel per point whatever they say, and text drawn for
another scale comes out with misplaced glyphs.

## Frames that did not change

egui asks for frames it does not need: animations settling, cursor blinks, extra passes when a
widget changes size. Many come out identical to the last. A frame is skipped when all of these
hold:

- skipping is on, which it is by default;
- the surface kept its size;
- egui uploaded no texture this frame;
- the tessellated meshes and clip rectangles are bit-identical to the last frame's, texture ids
  and indices included;
- nothing touched the surface or the textures since: no `begin_frame`, `create_texture`,
  `update_texture` or `free_texture`, and the last frame did not fail.

A skipped frame leaves the surface as it was and reports `surface_changed: false`. Skip
presenting it too. dream-ini skips its framebuffer copy that way.

```rust
use dream_soft_render::SoftwareRenderer;
use dream_soft_render::egui_adapter::{RenderFrame, egui};

fn main() -> std::io::Result<()> {
    let context = egui::Context::default();
    let mut renderer = SoftwareRenderer::default();
    // Rasterize every frame, as a benchmark must.
    renderer.set_skip_unchanged_frames(false);

    for _ in 0..3 {
        let outcome = renderer.render_egui(
            320,
            240,
            &RenderFrame::new(&context),
            |ui| {
                ui.label("The same every frame");
            },
        )?;
        assert!(outcome.surface_changed);
    }
    Ok(())
}
```

The comparison is bitwise, so `-0.0` against `0.0` counts as a change. It costs one pass over the
frame's vertices and indices.

## When to render again

`RenderOutcome::repaint_delay` is how long egui wants to wait before the next frame:
`Duration::ZERO` for right away, `Duration::MAX` when it asked for no repaint at all. A host on a
battery waits for input or for that delay, whichever comes first, instead of rendering in a loop.
`egui_adapter::format_repaint_delay` spells it for a log line, with `none` for `Duration::MAX`.

## Your textures in egui

A texture made with `create_texture` appears in egui through `egui_texture_id`:

```rust
use dream_soft_render::SoftwareRenderer;
use dream_soft_render::egui_adapter::{RenderFrame, egui, egui_texture_id};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut renderer = SoftwareRenderer::default();
    let context = egui::Context::default();
    let red = renderer.create_texture(1, 1, &[255, 0, 0, 255])?;

    renderer.render_egui(64, 64, &RenderFrame::new(&context), |ui| {
        ui.painter().image(
            egui_texture_id(red),
            egui::Rect::from_min_max(
                egui::pos2(0.0, 0.0),
                egui::pos2(16.0, 16.0),
            ),
            egui::Rect::from_min_max(
                egui::pos2(0.0, 0.0),
                egui::pos2(1.0, 1.0),
            ),
            egui::Color32::WHITE,
        );
    })?;
    assert_eq!(renderer.surface().pixels[..4], [255, 0, 0, 255]);
    Ok(())
}
```

`egui_texture_id(id)` is `egui::TextureId::User` with the id's number. egui itself only uploads
`Managed` textures, so the two never collide. `egui::Image` takes the same id.

## egui's textures

egui's font atlas and the images you load through `Context::load_texture` arrive as texture
uploads and live in the renderer's store beside yours, under the same 8 MiB
[budget](@/docs/textures.md#the-budget). They are freed when egui releases them. You do not manage
them.

A mesh that names a texture the renderer does not hold, such as one you freed, is skipped. It is
not an error; egui's own GPU backends skip such meshes too, with a warning.

## Feathering

egui anti-aliases the edges of its shapes by *feathering*: a one-pixel strip of translucent
triangles along every edge. They are thin, numerous, and the most expensive pixels the renderer
draws. On a slow CPU, turn feathering off:

```rust
use dream_soft_render::egui_adapter::egui;

fn main() {
    let context = egui::Context::default();
    context.tessellation_options_mut(|options| options.feathering = false);
}
```

Text keeps its anti-aliasing either way, because it comes from the font atlas. Shapes get hard
edges. Counted in AArch64 instructions per frame of the benchmark scenes, feathering off removes
44% of the work for a form, 17% for a page of text and 35% for a window with a shadow.

{{ figure(src="/img/docs/feathering.png", alt="Two enlarged crops of the same egui checkboxes: on top with feathering, smooth edges on the check mark and the boxes; below without, the same shapes with stepped edges and the same smooth text.", caption="Feathering on (top) and off (bottom), enlarged 4x. The text is the same.") }}

## Paint callbacks

egui paint callbacks exist to run GPU code, and there is no GPU. A frame that contains one fails
with an error; nothing in it past the callback is drawn.

## When a frame fails

`render_egui` returns `std::io::Result`. It fails when:

| Cause | Error text |
|---|---|
| The surface is bigger than `MAX_SURFACE_PIXELS`, or longer than 65535 pixels on a side | `software surface pixel budget exceeded: 922320 > 921600` or `software surface 70000x2 is longer than 65535 pixels on a side` |
| An upload would take the store past `MAX_TEXTURE_BYTES` | `texture byte budget exceeded: 8519696 > 8388608` |
| An upload is longer than 65536 texels on a side | `texture 65537x1 is longer than 65536 texels on a side` |
| An upload whose pixel count does not match its size | `texture pixel count mismatch: 3 != 4` |
| A partial upload names a texture that was never uploaded, or does not fit it | `partial texture update for missing texture`, `partial texture update exceeds texture bounds` |
| A mesh's index count is not a multiple of three | `mesh has 5 indices, not a multiple of three` |
| A mesh has an index past its vertices | `mesh index 9 is out of range for 4 vertices` |
| A clip rectangle has a NaN or infinite edge | `non-finite clip rectangle value` |
| A paint callback | `unsupported egui paint callback in software renderer` |

The two mesh errors carry the crate's own [`Error`](@/docs/api/errors.md), the one `Frame::mesh`
returns for the same indices; `error.get_ref()` and `downcast_ref::<dream_soft_render::Error>()`
reach it. egui checks mesh indices only in debug builds, so in release a hand-built
`egui::Shape::mesh` with a bad index gets this far.

A failed frame may have drawn some of its meshes, and it has applied the uploads before the one
that failed. The next frame rasterizes in full. egui does not send an upload twice, so a texture
whose upload failed stays missing, and meshes that use it are skipped, until egui uploads it
again.

`Error` converts from `std::io::Error`, as `Error::Raster` with the message, so a function that
returns `dream_soft_render::Error` can use `?` on `render_egui`.

## The diagnostics log

`RenderFrame::new(context)` turns every diagnostic off. The other fields turn them on, and a
struct update keeps the rest:

```rust
use std::time::Duration;

use dream_soft_render::SoftwareRenderer;
use dream_soft_render::egui_adapter::{RenderFrame, egui};

fn main() -> std::io::Result<()> {
    let context = egui::Context::default();
    let mut renderer = SoftwareRenderer::default();
    let print = |line: &str| eprintln!("{line}");
    let frame = RenderFrame {
        log: Some(&print),
        log_frame: true,
        frame_index: 1,
        hitch_log_threshold: Some(Duration::from_millis(33)),
        ..RenderFrame::new(&context)
    };
    let outcome = renderer.render_egui(64, 48, &frame, |ui| {
        ui.label("Logged");
    })?;
    let timings = outcome.timings.expect("log_frame measures the stages");
    if timings.total > Duration::from_millis(33).as_micros() {
        eprintln!("hitch: {} us", timings.total);
    }
    Ok(())
}
```

| Field | Does |
|---|---|
| `log` | Where log lines go. `None` discards them. |
| `log_frame` | Measures each stage and logs two lines per frame: surface and texture totals, then stage timings. |
| `log_render_stats` | Logs a `render_stats` line every frame, and for each frame it rasterizes, the renderer's deep statistics: which fast path took each primitive, why the others were rejected, and per-path timings. Slow; for profiling. |
| `hitch_log_threshold` | Measures each stage even when `log_frame` is off, so the host can compare `timings.total` with its own threshold. The renderer does not compare or log anything itself. |
| `frame_index` | A number printed in the log lines, nothing more. |
| `repaint_request_due_before_frame` | Printed in the `render_stats` line, nothing more. |
| `synthetic_workload` | Rasterizes a fixed benchmark mesh instead of running the UI. See [the API](@/docs/api/egui-adapter.md#sampledrectmodulatedworkload). |

With `log_frame`, the lines look like this:

```text
software renderer frame=1 surface=64x48 bytes=12288 textures=1 texture_bytes=131072 primitives=1
software renderer timings frame=1 resize_clear_us=47 egui_run_us=3531 texture_apply_us=1105 tessellate_us=8 rasterize_us=110 texture_free_us=0 repaint_delay=0ns raster_skipped=false total_us=4810
```

The lines are for people reading a log. The same numbers are in `RenderOutcome`'s
`texture_evidence` and `timings`, which is where a program should read them.
