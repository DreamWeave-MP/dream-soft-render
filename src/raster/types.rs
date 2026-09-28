// SPDX-License-Identifier: MIT OR Apache-2.0

use std::io;

use super::math::f32_to_usize_floor_clamped;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TriangleClassification {
    Degenerate,
    Solid,
    Textured,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TexturedQuadFastPathRejection {
    NotRectangleDiagonal,
    NotAxisAlignedRectangle,
    CornerAttributeMismatch,
    NonUniformColor,
    NonAffineUv,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SolidTriangleColorDecision {
    Solid([u8; 4]),
    NonUniformVertexColor,
    NonUniformTexel,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TriangleScanWorkEstimate {
    pub(crate) candidate_px: usize,
    pub(crate) narrowed_rows: usize,
    pub(crate) full_scan_rows: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TriangleTexelSample {
    pub(crate) texels: Option<[(usize, usize); 3]>,
    pub(crate) uniform_color: Option<[u8; 4]>,
}

impl TriangleTexelSample {
    pub(crate) const fn is_uniform(self) -> bool {
        self.uniform_color.is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ClipBounds {
    pub(crate) min_x: usize,
    pub(crate) min_y: usize,
    pub(crate) max_x: usize,
    pub(crate) max_y: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TriangleRasterBounds {
    pub(crate) min_x: usize,
    pub(crate) min_y: usize,
    pub(crate) max_x: usize,
    pub(crate) max_y: usize,
}

impl TriangleRasterBounds {
    pub(crate) const fn pixel_area(self) -> usize {
        (self.max_x - self.min_x) * (self.max_y - self.min_y)
    }
}

impl ClipBounds {
    /// The whole `width`x`height` surface.
    pub(crate) const fn full(width: usize, height: usize) -> Self {
        Self {
            min_x: 0,
            min_y: 0,
            max_x: width,
            max_y: height,
        }
    }

    /// Pixel bounds covering a float rectangle given as `[min_x, min_y, max_x, max_y]`, grown
    /// outward to whole pixels and clamped to the surface. Non-finite edges are an error.
    pub(crate) fn from_float_edges(
        edges: [f32; 4],
        width: usize,
        height: usize,
    ) -> io::Result<Self> {
        let [min_x, min_y, max_x, max_y] = edges;
        let min_x = clamp_rect_value(min_x.floor(), width)?;
        let min_y = clamp_rect_value(min_y.floor(), height)?;
        let max_x = clamp_rect_value(max_x.ceil(), width)?;
        let max_y = clamp_rect_value(max_y.ceil(), height)?;
        Ok(Self {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    pub(crate) const fn is_empty(self) -> bool {
        self.min_x >= self.max_x || self.min_y >= self.max_y
    }
}

fn clamp_rect_value(value: f32, max: usize) -> io::Result<usize> {
    if !value.is_finite() {
        return Err(io::Error::other("non-finite clip rectangle value"));
    }
    Ok(f32_to_usize_floor_clamped(value, max))
}
