// SPDX-License-Identifier: MIT OR Apache-2.0

//! CPU rasterizer for egui.
//!
//! [`SoftwareRenderer`] runs one egui frame, tessellates it, and rasterizes the
//! primitives into an RGBA8 [`SoftwareSurface`]. Presenting that surface
//! (framebuffer, window, file) is left to the caller.

mod raster;
mod render_benchmark;
mod renderer;
mod surface;
mod texture;

use std::time::Duration;

pub use render_benchmark::SampledRectModulatedWorkload;
pub use renderer::{RenderFrame, RenderOutcome, RenderTimings, SoftwareRenderer, TextureEvidence};
pub use surface::SoftwareSurface;

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
