+++
title = "Rust API"
description = "Every public type, method, constant and error in dream-soft-render, with exact signatures."
template = "docs/section.html"
page_template = "docs/page.html"
sort_by = "weight"
weight = 90

[extra]
kind = "api"
hide_child_cards = true
+++

The crate `dream_soft_render` exports its drawing API from the root, and the egui adapter from
the module `egui_adapter`, behind the `egui` feature.

```toml
[dependencies]
dream-soft-render = "1"
```

```rust
use dream_soft_render::{
    ClipRect, Color, Error, Mesh, Rect, SoftwareRenderer, Vertex,
};

fn main() -> Result<(), Error> {
    let mut renderer = SoftwareRenderer::default();
    let texture = renderer.create_texture(1, 1, &[255, 255, 255, 255])?;

    let mut frame = renderer.begin_frame(64, 48)?;
    frame.clear(Color::from_rgb(17, 20, 28));
    frame.textured_rect(
        Rect::from_min_size([4.0, 4.0], [16.0, 16.0]),
        Rect::FULL_UV,
        texture,
        Color::from_rgb(80, 160, 255),
        ClipRect::ALL,
    )?;
    let warning = Color::from_rgb(220, 40, 40);
    frame.mesh(
        Mesh {
            vertices: &[
                Vertex::new([8.0, 40.0], [0.0, 0.0], warning),
                Vertex::new([24.0, 24.0], [0.0, 0.0], warning),
                Vertex::new([40.0, 40.0], [0.0, 0.0], warning),
            ],
            indices: &[0, 1, 2],
            texture: None,
        },
        ClipRect::ALL,
    )?;
    assert_eq!(frame.surface().pixels.len(), 64 * 48 * 4);
    Ok(())
}
```

| Page | Covers |
|---|---|
| [Renderer and frames](@/docs/api/renderer.md) | `SoftwareRenderer`, `Frame`, `Mesh`, `SoftwareSurface`, `MAX_SURFACE_PIXELS`, `MAX_TEXTURE_BYTES` |
| [Geometry and color](@/docs/api/types.md) | `Color`, `Vertex`, `Rect`, `ClipRect`, `TextureId` |
| [Errors](@/docs/api/errors.md) | `Error`, every variant and its text, and `VertexField` |
| [egui adapter](@/docs/api/egui-adapter.md) | `render_egui`, `set_skip_unchanged_frames`, and the `egui_adapter` module |

## Features

| Feature | Default | Adds |
|---|---|---|
| `egui` | No | `SoftwareRenderer::render_egui`, `SoftwareRenderer::set_skip_unchanged_frames`, the `egui_adapter` module, and egui 0.34 as a dependency |

## Threads

`SoftwareRenderer`, `SoftwareSurface` and every value type are `Send` and `Sync`: a renderer can be
built on one thread and drawn with on another. Texture ids come from one process-wide atomic
counter, so renderers on different threads never issue the same id. `RenderFrame` borrows the
egui context and the log closure, and is neither.
