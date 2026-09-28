// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::Color;

/// A mesh vertex: a position in surface pixels, a normalized texture coordinate, and a
/// premultiplied color that multiplies the sampled texel.
///
/// The layout is fixed at 20 bytes (`#[repr(C)]`: four `f32` then four color bytes), so
/// vertex data can be produced in bulk by other code without conversion.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Vertex {
    /// Position in surface pixels; see the crate docs for the coordinate conventions.
    pub pos: [f32; 2],
    /// Texture coordinate, `0.0..=1.0` across the texture. Untextured meshes sample white
    /// wherever it points, but it must still be finite.
    pub uv: [f32; 2],
    /// Premultiplied color, multiplied with the sampled texel.
    pub color: Color,
}

const _: () = assert!(size_of::<Vertex>() == 20);

impl Vertex {
    /// A vertex from its parts.
    #[must_use]
    pub const fn new(pos: [f32; 2], uv: [f32; 2], color: Color) -> Self {
        Self { pos, uv, color }
    }

    /// The first coordinate that is NaN or infinite, if any. The color cannot be invalid:
    /// every bit pattern is a color.
    pub(crate) fn non_finite_field(self) -> Option<crate::VertexField> {
        if !self.pos.iter().all(|value| value.is_finite()) {
            Some(crate::VertexField::Position)
        } else if !self.uv.iter().all(|value| value.is_finite()) {
            Some(crate::VertexField::Uv)
        } else {
            None
        }
    }

    pub(crate) const fn pos2(&self) -> Pos2 {
        pos2(self.pos[0], self.pos[1])
    }

    pub(crate) const fn uv2(&self) -> Pos2 {
        pos2(self.uv[0], self.uv[1])
    }

    #[cfg(test)]
    pub(crate) const fn to_egui(self) -> egui::epaint::Vertex {
        egui::epaint::Vertex {
            pos: egui::pos2(self.pos[0], self.pos[1]),
            uv: egui::pos2(self.uv[0], self.uv[1]),
            color: self.color.to_egui(),
        }
    }
}

/// A point inside the rasterizer: surface pixels for positions, normalized for texture
/// coordinates. Public vertices use `[f32; 2]`; this is the named-field view the raster code
/// computes with.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Pos2 {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

impl Pos2 {
    pub(crate) const ZERO: Self = pos2(0.0, 0.0);
}

pub(crate) const fn pos2(x: f32, y: f32) -> Pos2 {
    Pos2 { x, y }
}

/// A per-pixel step through texture space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Vec2 {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

pub(crate) const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

/// The triangles the raster core draws: borrowed vertices and indices, three per triangle.
/// [`Frame::mesh`](crate::Frame::mesh) hands over the caller's slices as they are, and the
/// egui adapter reinterprets egui's vertex buffer in place, so neither path copies geometry.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RasterMesh<'a> {
    pub(crate) vertices: &'a [Vertex],
    pub(crate) indices: &'a [u32],
}

/// An axis-aligned rectangle in floating-point surface pixels (for positions) or normalized
/// texture coordinates (for UV ranges).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Top-left corner.
    pub min: [f32; 2],
    /// Bottom-right corner.
    pub max: [f32; 2],
}

impl Rect {
    /// The whole texture, as a UV range.
    pub const FULL_UV: Self = Self::from_min_max([0.0, 0.0], [1.0, 1.0]);

    /// A rectangle from its top-left and bottom-right corners.
    #[must_use]
    pub const fn from_min_max(min: [f32; 2], max: [f32; 2]) -> Self {
        Self { min, max }
    }

    /// A rectangle from its top-left corner and size.
    #[must_use]
    pub fn from_min_size(min: [f32; 2], size: [f32; 2]) -> Self {
        Self::from_min_max(min, [min[0] + size[0], min[1] + size[1]])
    }

    pub(crate) fn is_finite(self) -> bool {
        self.min
            .iter()
            .chain(&self.max)
            .all(|value| value.is_finite())
    }
}

/// A clip rectangle in integer surface pixels, half-open: it keeps pixels with
/// `min_x <= x < max_x` and `min_y <= y < max_y`.
///
/// Bounds past the surface are clamped to it, and a rectangle with `min >= max` on either axis
/// is empty, so drawing through it writes nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ClipRect {
    /// First column kept.
    pub min_x: u32,
    /// First row kept.
    pub min_y: u32,
    /// One past the last column kept.
    pub max_x: u32,
    /// One past the last row kept.
    pub max_y: u32,
}

impl ClipRect {
    /// No clipping beyond the surface itself.
    pub const ALL: Self = Self::new(0, 0, u32::MAX, u32::MAX);

    /// A clip rectangle from its bounds.
    #[must_use]
    pub const fn new(min_x: u32, min_y: u32, max_x: u32, max_y: u32) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub(crate) fn to_bounds(self, width: usize, height: usize) -> crate::raster::ClipBounds {
        let clamp = |value: u32, max: usize| usize::try_from(value).map_or(max, |v| v.min(max));
        crate::raster::ClipBounds {
            min_x: clamp(self.min_x, width),
            min_y: clamp(self.min_y, height),
            max_x: clamp(self.max_x, width),
            max_y: clamp(self.max_y, height),
        }
    }
}
