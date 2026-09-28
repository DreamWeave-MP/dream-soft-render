// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drawing [egui] frames with a [`SoftwareRenderer`](crate::SoftwareRenderer).
//!
//! [`SoftwareRenderer::render_egui`](crate::SoftwareRenderer::render_egui) runs a UI closure
//! for one frame, applies egui's texture uploads, tessellates, and rasterizes into the same
//! surface the generic [`Frame`](crate::Frame) API draws into. Everything egui-specific lives
//! here; the rest of the crate does not expose egui types.
//!
//! Notes for egui hosts:
//!
//! - Pixels per point is fixed at 1: one egui point is one surface pixel.
//! - egui paint callbacks are not supported and fail the frame.
//! - A frame whose tessellated output is bit-identical to the previous one, with no texture
//!   uploads, skips rasterization and reports
//!   [`RenderOutcome::surface_changed`]` == false`; see
//!   [`SoftwareRenderer::set_skip_unchanged_frames`](crate::SoftwareRenderer::set_skip_unchanged_frames).
//! - Shape anti-aliasing (egui's `feathering` tessellation option) is the most expensive thing
//!   the rasterizer draws. Hosts on slow CPUs may want
//!   `ctx.tessellation_options_mut(|options| options.feathering = false)`.
//! - The `egui` re-export is the version this crate is built against.

use std::time::Duration;

pub use egui;

pub use crate::render_benchmark::SampledRectModulatedWorkload;
pub use crate::renderer::{RenderFrame, RenderOutcome, RenderTimings, TextureEvidence};
use crate::texture::TextureId;

/// The egui texture id that shows `texture` (made with
/// [`SoftwareRenderer::create_texture`](crate::SoftwareRenderer::create_texture)) in egui
/// meshes, for example through `egui::Image`.
#[must_use]
pub const fn egui_texture_id(texture: TextureId) -> egui::TextureId {
    texture.to_egui()
}

/// Formats an egui repaint delay for log lines, spelling "no repaint requested" as `none`.
#[must_use]
pub fn format_repaint_delay(repaint_delay: Duration) -> String {
    if repaint_delay == Duration::MAX {
        "none".to_owned()
    } else {
        format!("{repaint_delay:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_repaint_delay_names_missing_repaint() {
        assert_eq!(format_repaint_delay(Duration::MAX), "none");
        assert_eq!(format_repaint_delay(Duration::from_millis(16)), "16ms");
    }
}
