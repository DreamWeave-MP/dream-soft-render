+++
title = "egui adapter"
description = "render_egui, set_skip_unchanged_frames, and the egui_adapter module: RenderFrame, RenderOutcome, RenderTimings, TextureEvidence, egui_texture_id, format_repaint_delay and the benchmark workload."
weight = 40

[extra]
kind = "api"
+++

Everything here needs the `egui` feature. [Hosting egui](@/docs/egui.md) explains how the pieces
fit together.

```rust
use dream_soft_render::SoftwareRenderer;
use dream_soft_render::egui_adapter::{RenderFrame, RenderOutcome, egui};

fn main() -> std::io::Result<()> {
    let context = egui::Context::default();
    let mut renderer = SoftwareRenderer::default();
    let outcome: RenderOutcome = renderer.render_egui(
        320,
        240,
        &RenderFrame::new(&context),
        |ui| {
            ui.label("Hello");
        },
    )?;
    assert!(outcome.surface_changed);
    // The first frame uploads egui's font atlas.
    assert_eq!(outcome.texture_evidence.full_upload_count, 1);
    Ok(())
}
```

## render_egui

{{ api_signature(value="pub fn render_egui(&mut self, width: usize, height: usize, frame: &RenderFrame<'_>, run_ui: impl FnMut(&mut egui::Ui)) -> io::Result<RenderOutcome>") }}

A method of `SoftwareRenderer`. Runs `run_ui` for one egui frame of `width`x`height` points,
applies egui's texture uploads, tessellates at one pixel per point, and rasterizes into the
surface, unless the frame is identical to the last one. Then frees the textures egui released.
With `frame.synthetic_workload` set, it rasterizes that workload instead and does not run
`run_ui`.

Returns an error when the surface cannot be sized, an upload is malformed, over the texture
budget or longer than 65536 texels on a side, a mesh's index count is not a multiple of three or
an index is past its vertices, a clip rectangle is not finite, or egui emits a paint callback.
Mesh errors carry the crate's [`Error`](@/docs/api/errors.md). [When a frame
fails](@/docs/egui.md#when-a-frame-fails) lists each error's text and what state the renderer is
left in.

## set_skip_unchanged_frames

{{ api_signature(value="pub const fn set_skip_unchanged_frames(&mut self, skip: bool)") }}

A method of `SoftwareRenderer`. Whether `render_egui` may skip a frame whose tessellated output is
bit-identical to the last one, with no texture uploads, and keep the surface. On by default. Turn
it off to rasterize every frame, as a benchmark must.

## RenderFrame

{{ api_signature(value="pub struct RenderFrame<'a>") }}

What one `render_egui` call needs besides the UI: the egui context, and the diagnostics switches.
Every field is public.

| Field | Type | |
|---|---|---|
| `context` | `&'a egui::Context` | The context the UI runs in |
| `log` | `Option<&'a dyn Fn(&str)>` | Where diagnostic lines go; `None` discards them |
| `log_frame` | `bool` | Measure each stage, and log surface totals and stage timings |
| `log_render_stats` | `bool` | Log the renderer's deep statistics. Slow |
| `hitch_log_threshold` | `Option<Duration>` | When set, measure each stage even without `log_frame`. Nothing compares against it but the host |
| `frame_index` | `u64` | Printed in the log lines |
| `repaint_request_due_before_frame` | `bool` | Printed in the `render_stats` line |
| `synthetic_workload` | `Option<&'a SampledRectModulatedWorkload>` | Rasterize this instead of running the UI |

{{ api_signature(value="pub const fn new(context: &'a egui::Context) -> RenderFrame<'a>") }}

A frame for `context` with every diagnostic off and no workload. Build others from it with struct
update syntax: `RenderFrame { log_frame: true, ..RenderFrame::new(&context) }`.

## RenderOutcome

{{ api_signature(value="pub struct RenderOutcome") }}

What one `render_egui` call did. `Clone`, `Copy`, `Debug`.

| Field | Type | |
|---|---|---|
| `repaint_delay` | `Duration` | How long egui wants to wait before the next frame. `Duration::MAX`: no repaint requested |
| `timings` | `Option<RenderTimings>` | Stage timings, when `log_frame` or `hitch_log_threshold` asked for them |
| `primitive_count` | `usize` | The clipped primitives egui produced |
| `texture_evidence` | `TextureEvidence` | The texture store after the frame, and this frame's uploads |
| `surface_changed` | `bool` | `false` when the frame matched the last one and the surface was left as it was |

## RenderTimings

{{ api_signature(value="pub struct RenderTimings") }}

How long each stage of one `render_egui` call took, in microseconds, every field a `u128`.
`Clone`, `Copy`, `Debug`.

| Field | |
|---|---|
| `resize_clear` | Sizing the surface, and clearing it if the size changed |
| `egui_run` | Running the UI |
| `texture_apply` | Applying egui's texture uploads |
| `tessellate` | Tessellating egui's shapes |
| `rasterize` | Rasterizing, or for a skipped frame, finding that it is unchanged |
| `texture_free` | Freeing the textures egui released |
| `total` | The whole call |

## TextureEvidence

{{ api_signature(value="pub struct TextureEvidence") }}

The texture store after a frame, and the uploads egui made during it. Every field a `usize`.
`Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`.

| Field | |
|---|---|
| `count` | Textures stored, yours and egui's |
| `bytes` | Bytes of texture pixels stored, against `MAX_TEXTURE_BYTES` |
| `set_count` | Uploads egui made this frame |
| `set_bytes` | Bytes those uploads carried |
| `full_upload_count` | Uploads that replaced or created a whole texture |
| `partial_update_count` | Uploads that patched part of one |

## egui_texture_id

{{ api_signature(value="pub const fn egui_texture_id(texture: TextureId) -> egui::TextureId") }}

The `egui::TextureId` that makes an egui mesh, `egui::Image` or painter call sample a texture
made with `create_texture`: `egui::TextureId::User` with the handle's number. egui uploads only
`Managed` textures, so the two kinds never collide.

## format_repaint_delay

{{ api_signature(value="pub fn format_repaint_delay(repaint_delay: Duration) -> String") }}

A repaint delay for a log line: `none` for `Duration::MAX`, otherwise `Duration`'s `Debug` form,
such as `16ms`.

## egui

{{ api_signature(value="pub use egui;") }}

The egui crate this one is built against, 0.34. Using it through the re-export keeps your egui
types and the renderer's the same version.

## SampledRectModulatedWorkload

{{ api_signature(value="pub struct SampledRectModulatedWorkload") }}

A fixed rasterizer benchmark, used by dream-ini's PortMaster build to measure the renderer on a
device without egui in the measurement. `Debug`, `PartialEq`, `Eq`.

It is 192 one-pixel-tall textured rectangles, 20 of them 2 pixels wide, 152 of 5 and 20 of 10,
each sampling a run of a 64x4 texture with mixed alpha and tinted with one non-white color: the
shape of glyph rows. With `RenderFrame::synthetic_workload` set, `render_egui` clears the surface
to `(17, 20, 28)`, rasterizes the workload, and reports a changed surface with one primitive,
without running the UI.

| Item | |
|---|---|
| `fn new(viewport_width: usize, viewport_height: usize) -> SampledRectModulatedWorkload` | Lays the rectangles out in rows for a surface of that size |
| `const fn matches_viewport(&self, width: usize, height: usize) -> bool` | Whether it was built for that size. `render_egui` does not check; rebuild it when the surface size changes |
| `fn config_log_line(&self, frame_limit: u64) -> String` | One line describing the workload, for a benchmark log |
| `const DESCRIPTION: &'static str` | The workload's shape in words |
