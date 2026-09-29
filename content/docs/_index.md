+++
title = "Documentation"
description = "How to draw with dream-soft-render, the rules every pixel follows, hosting egui, what makes a frame cheap, and the complete Rust API."
template = "docs/section.html"
page_template = "docs/page.html"
sort_by = "weight"

[extra]
docs_root = true
docs_project_name = "dream-soft-render"
docs_short_title = "dream-soft-render docs"
docs_project_path = "@/home/index.md"
docs_repository_url = "https://github.com/DreamWeave-MP/dream-soft-render/tree/main/content/docs"
docs_sidebar_label = "Documentation"
hide_child_cards = true
kind = "guide"
+++

dream-soft-render rasterizes 2D triangles into a buffer of premultiplied RGBA8 bytes, on the CPU,
the same way on every platform. You draw through its own API, or hand it an egui UI with the
`egui` feature; both write the same surface.

## Learn it

- **[Start here](@/docs/start-here.md)**: add the crate, draw a frame, and save it as an image.
- **[The rules](@/docs/rules.md)**: the contract every pixel follows. Coordinates, which pixels a
  shape covers, color, blending, sampling, limits and errors.

## Use it

- **[Drawing](@/docs/drawing.md)**: frames, rectangles, meshes, clipping and draw order.
- **[Textures](@/docs/textures.md)**: making, patching and freeing them, premultiplying, and the
  budget.
- **[Hosting egui](@/docs/egui.md)**: `render_egui`, egui's textures and yours, frames that did not
  change, and the diagnostics log.
- **[Presenting frames](@/docs/presenting.md)**: getting the surface onto a framebuffer, into a
  file, or into a test.
- **[Fast frames](@/docs/performance.md)**: what the rasterizer recognizes, what costs the most,
  and the benchmarks.

## Look it up

- **[Platforms](@/docs/platforms.md)**: what is architecture-specific, the NEON kernels, the
  supported Rust, the license, and what is tested where.
- **[Rust API](@/docs/api/_index.md)**: every public type, method and constant, with its exact
  signature and every way it fails.
