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

// egui's vertex and this crate's `Vertex` must be the same 20 bytes for `vertices_from_egui` to
// reinterpret one as the other: position at 0, texture coordinate at 8, color at 16, each point
// two f32s (x then y), each color four bytes (r, g, b, a). egui's optional `unity` feature
// reorders its vertex fields; if anything enables it, these assertions stop the build instead
// of letting the renderer read colors as coordinates.
const _: () = {
    use std::mem::{align_of, offset_of, size_of};

    use egui::epaint::Vertex as EguiVertex;

    use crate::{Color, Vertex};

    assert!(size_of::<EguiVertex>() == size_of::<Vertex>());
    assert!(align_of::<Vertex>() <= align_of::<EguiVertex>());
    assert!(offset_of!(EguiVertex, pos) == offset_of!(Vertex, pos));
    assert!(offset_of!(EguiVertex, uv) == offset_of!(Vertex, uv));
    assert!(offset_of!(EguiVertex, color) == offset_of!(Vertex, color));
    assert!(size_of::<egui::Pos2>() == size_of::<[f32; 2]>());
    assert!(offset_of!(egui::Pos2, x) == 0 && offset_of!(egui::Pos2, y) == size_of::<f32>());
    // Color32's one field is private, but a 4-byte struct holding a [u8; 4] can only hold it at
    // offset 0, and a known color shows the byte order.
    assert!(size_of::<egui::Color32>() == size_of::<Color>());
    assert!(offset_of!(Color, r) == 0 && offset_of!(Color, g) == 1);
    assert!(offset_of!(Color, b) == 2 && offset_of!(Color, a) == 3);
    // SAFETY: Color32 is 4 bytes of plain u8 data (checked above); any bytes are a valid array.
    let bytes = unsafe {
        std::mem::transmute::<egui::Color32, [u8; 4]>(egui::Color32::from_rgba_premultiplied(
            1, 2, 3, 4,
        ))
    };
    assert!(bytes[0] == 1 && bytes[1] == 2 && bytes[2] == 3 && bytes[3] == 4);
};

/// egui's vertex buffer, viewed as this crate's vertices without copying.
pub(crate) const fn vertices_from_egui(vertices: &[egui::epaint::Vertex]) -> &[crate::Vertex] {
    // SAFETY: the assertions above prove both types have the same size and field layout, and
    // `Vertex`'s alignment is no stricter than egui's, so every egui vertex is a correctly
    // aligned `Vertex` in place. All fields are f32 or u8, for which every bit pattern is
    // valid. The returned slice borrows `vertices`, so it cannot outlive them.
    unsafe { std::slice::from_raw_parts(vertices.as_ptr().cast::<crate::Vertex>(), vertices.len()) }
}

/// An egui mesh's geometry as the raster core's mesh view, without copying.
pub(crate) fn raster_mesh_from_egui(mesh: &egui::Mesh) -> crate::geometry::RasterMesh<'_> {
    crate::geometry::RasterMesh {
        vertices: vertices_from_egui(&mesh.vertices),
        indices: &mesh.indices,
    }
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
