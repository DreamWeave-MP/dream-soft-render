// SPDX-License-Identifier: MIT OR Apache-2.0

use super::TriangleVertices;
use super::math::{
    PixelOffset, edge, edge_covers_pixel, edge_step_x, f32_to_usize_ceil_clamped,
    f32_to_usize_floor_clamped, same_f32, usize_to_f32,
};
use super::types::TriangleRasterBounds;
use crate::geometry::{Pos2, pos2};

const TRIANGLE_SCANLINE_NARROWING_GUARD_PX: usize = 2;

#[derive(Clone, Copy)]
pub(super) struct TriangleBoundaryIncludes {
    pub(super) edge0: bool,
    pub(super) edge1: bool,
    pub(super) edge2: bool,
}

#[derive(Clone, Copy)]
pub(super) struct TriangleCoverage {
    pub(super) inv_area: f32,
    pub(super) includes_boundary: TriangleBoundaryIncludes,
}

#[derive(Clone, Copy)]
pub(super) struct TriangleRowSearch<'a> {
    pub(super) vertices: TriangleVertices<'a>,
    pub(super) coverage: TriangleCoverage,
    pub(super) y: usize,
    pub(super) candidate_start_x: usize,
    pub(super) candidate_end_x: usize,
    pub(super) probe_start_x: usize,
    pub(super) probe_end_x: usize,
    pub(super) hinted: bool,
    pub(super) collect_stats: bool,
}

#[derive(Clone, Copy)]
pub(super) struct TriangleRowStateSearch {
    pub(super) coverage: TriangleCoverage,
    pub(super) row_start_edges: (f32, f32, f32),
    pub(super) x_steps: (f32, f32, f32),
    pub(super) candidate_start_x: usize,
    pub(super) candidate_end_x: usize,
    pub(super) collect_stats: bool,
}

pub(super) struct TriangleRowEndpoints {
    pub(super) span: Option<(usize, usize)>,
    pub(super) endpoint_probe_px: usize,
    pub(super) hint_probe_px: usize,
    pub(super) canary_probe_px: usize,
    pub(super) fallback_probe_px: usize,
    pub(super) direct_probe_px: usize,
    pub(super) fell_back: bool,
}

pub(super) fn triangle_row_endpoints(search: TriangleRowSearch<'_>) -> TriangleRowEndpoints {
    let (mut span_start, first_probe_px) = triangle_first_covered_x(
        search.probe_start_x,
        search.probe_end_x,
        search.y,
        search.vertices,
        search.coverage,
        search.collect_stats,
    );
    let mut hint_probe_px = 0;
    let mut canary_probe_px = 0;
    let mut fallback_probe_px = 0;
    let mut direct_probe_px = 0;
    if search.hinted {
        hint_probe_px += first_probe_px;
    } else {
        direct_probe_px += first_probe_px;
    }
    let mut fell_back = false;
    if search.hinted {
        fell_back = triangle_hint_needs_fallback(&search, span_start, &mut canary_probe_px);
        if fell_back {
            let (fallback_span_start, fallback_first_probe_px) = triangle_first_covered_x(
                search.candidate_start_x,
                search.candidate_end_x,
                search.y,
                search.vertices,
                search.coverage,
                search.collect_stats,
            );
            fallback_probe_px += fallback_first_probe_px;
            span_start = fallback_span_start;
        }
    }

    let Some(span_start) = span_start else {
        return TriangleRowEndpoints {
            span: None,
            endpoint_probe_px: hint_probe_px
                + canary_probe_px
                + fallback_probe_px
                + direct_probe_px,
            hint_probe_px,
            canary_probe_px,
            fallback_probe_px,
            direct_probe_px,
            fell_back,
        };
    };
    let (last_start_x, last_end_x) = if fell_back {
        (search.candidate_start_x, search.candidate_end_x)
    } else {
        (search.probe_start_x, search.probe_end_x)
    };
    let (last_x, last_probe_px) = triangle_last_covered_x(
        last_start_x,
        last_end_x,
        search.y,
        search.vertices,
        search.coverage,
        search.collect_stats,
    );
    if fell_back {
        fallback_probe_px += last_probe_px;
    } else if search.hinted {
        hint_probe_px += last_probe_px;
    } else {
        direct_probe_px += last_probe_px;
    }
    let span_end = last_x.map_or(span_start + 1, |last_x| last_x.max(span_start) + 1);
    TriangleRowEndpoints {
        span: Some((span_start, span_end)),
        endpoint_probe_px: hint_probe_px + canary_probe_px + fallback_probe_px + direct_probe_px,
        hint_probe_px,
        canary_probe_px,
        fallback_probe_px,
        direct_probe_px,
        fell_back,
    }
}

pub(super) fn triangle_row_state_endpoints(search: TriangleRowStateSearch) -> TriangleRowEndpoints {
    let (span_start, first_probe_px) = triangle_row_state_first_covered_x(search);
    let Some(span_start) = span_start else {
        return TriangleRowEndpoints {
            span: None,
            endpoint_probe_px: first_probe_px,
            hint_probe_px: 0,
            canary_probe_px: 0,
            fallback_probe_px: 0,
            direct_probe_px: first_probe_px,
            fell_back: false,
        };
    };

    let (last_x, last_probe_px) = triangle_row_state_last_covered_x(search);
    let span_end = last_x.map_or(span_start + 1, |last_x| last_x.max(span_start) + 1);
    let endpoint_probe_px = first_probe_px + last_probe_px;
    TriangleRowEndpoints {
        span: Some((span_start, span_end)),
        endpoint_probe_px,
        hint_probe_px: 0,
        canary_probe_px: 0,
        fallback_probe_px: 0,
        direct_probe_px: endpoint_probe_px,
        fell_back: false,
    }
}

fn triangle_row_state_first_covered_x(search: TriangleRowStateSearch) -> (Option<usize>, usize) {
    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    if !search.collect_stats && neon_row_scan::worth_it(search) {
        return (neon_row_scan::first_covered_x(search), 0);
    }
    let (step0, step1, step2) = search.x_steps;
    let (row_edge0, row_edge1, row_edge2) = search.row_start_edges;
    let mut probe_px = 0;
    let mut offset = PixelOffset::new(0);
    for x in search.candidate_start_x..search.candidate_end_x {
        if search.collect_stats {
            probe_px += 1;
        }
        let dx = offset.get();
        offset.advance();
        let edge0 = row_edge0 + step0 * dx;
        let edge1 = row_edge1 + step1 * dx;
        let edge2 = row_edge2 + step2 * dx;
        if triangle_row_state_covers_pixel(search.coverage, edge0, edge1, edge2) {
            return (Some(x), probe_px);
        }
    }
    (None, probe_px)
}

fn triangle_row_state_last_covered_x(search: TriangleRowStateSearch) -> (Option<usize>, usize) {
    let Some(last_candidate_x) = search.candidate_end_x.checked_sub(1) else {
        return (None, 0);
    };
    if last_candidate_x < search.candidate_start_x {
        return (None, 0);
    }
    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    if !search.collect_stats && neon_row_scan::worth_it(search) {
        return (neon_row_scan::last_covered_x(search), 0);
    }

    let (step0, step1, step2) = search.x_steps;
    let (row_edge0, row_edge1, row_edge2) = search.row_start_edges;
    let mut probe_px = 0;
    let mut offset = PixelOffset::new(last_candidate_x - search.candidate_start_x);
    for x in (search.candidate_start_x..search.candidate_end_x).rev() {
        if search.collect_stats {
            probe_px += 1;
        }
        let dx = offset.get();
        offset.retreat();
        let edge0 = row_edge0 + step0 * dx;
        let edge1 = row_edge1 + step1 * dx;
        let edge2 = row_edge2 + step2 * dx;
        if triangle_row_state_covers_pixel(search.coverage, edge0, edge1, edge2) {
            return (Some(x), probe_px);
        }
    }
    (None, probe_px)
}

/// The row scans four pixels per step. Each lane runs the scalar probe's operations in the
/// same order (multiply, add, multiply, compare; nothing fused), so the first and last covered
/// lanes are the pixels the one-at-a-time scans stop at.
#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
mod neon_row_scan {
    use core::arch::aarch64::{
        float32x4_t, uint32x4_t, vaddq_f32, vaddvq_u32, vandq_u32, vceqq_f32, vcgtq_f32,
        vdupq_n_f32, vdupq_n_u32, vld1q_f32, vld1q_u32, vminq_f32, vmulq_f32, vorrq_u32,
    };

    use super::{PixelOffset, TriangleRowStateSearch};

    const LANE_OFFSETS: [f32; 4] = [0.0, 1.0, 2.0, 3.0];
    const LANE_BITS: [u32; 4] = [1, 2, 4, 8];

    /// Bit `i` set when pixel `offset + i` of the row is covered.
    fn covered_lanes(search: TriangleRowStateSearch, offset: usize) -> u32 {
        let (row_edge0, row_edge1, row_edge2) = search.row_start_edges;
        let (step0, step1, step2) = search.x_steps;
        let includes = search.coverage.includes_boundary;
        // SAFETY: only register arithmetic and loads from the two four-element constant arrays.
        unsafe {
            let offsets = vminq_f32(
                vaddq_f32(
                    vdupq_n_f32(PixelOffset::new(offset).raw()),
                    vld1q_f32(LANE_OFFSETS.as_ptr()),
                ),
                vdupq_n_f32(f32::from(u16::MAX)),
            );
            let inv_area = vdupq_n_f32(search.coverage.inv_area);
            let edge_covers = |row_edge: f32, step: f32, includes_boundary: bool| -> uint32x4_t {
                let edge: float32x4_t =
                    vaddq_f32(vdupq_n_f32(row_edge), vmulq_f32(vdupq_n_f32(step), offsets));
                let weight = vmulq_f32(edge, inv_area);
                let zero = vdupq_n_f32(0.0);
                let on_edge = vandq_u32(
                    vceqq_f32(weight, zero),
                    vdupq_n_u32(if includes_boundary { u32::MAX } else { 0 }),
                );
                vorrq_u32(vcgtq_f32(weight, zero), on_edge)
            };
            let covered = vandq_u32(
                vandq_u32(
                    edge_covers(row_edge0, step0, includes.edge0),
                    edge_covers(row_edge1, step1, includes.edge1),
                ),
                edge_covers(row_edge2, step2, includes.edge2),
            );
            vaddvq_u32(vandq_u32(covered, vld1q_u32(LANE_BITS.as_ptr())))
        }
    }

    /// Short candidate rows (egui's feathered edges are mostly two or three pixels) usually
    /// hit on their first probe, where one scalar probe is cheaper than a four-lane step.
    pub(super) const fn worth_it(search: TriangleRowStateSearch) -> bool {
        search
            .candidate_end_x
            .saturating_sub(search.candidate_start_x)
            >= 8
    }

    const fn lane_mask(lanes: usize) -> u32 {
        (1 << lanes) - 1
    }

    pub(super) fn first_covered_x(search: TriangleRowStateSearch) -> Option<usize> {
        let mut x = search.candidate_start_x;
        while x < search.candidate_end_x {
            let lanes = (search.candidate_end_x - x).min(4);
            let covered = covered_lanes(search, x - search.candidate_start_x) & lane_mask(lanes);
            if covered != 0 {
                return Some(x + covered.trailing_zeros() as usize);
            }
            x += 4;
        }
        None
    }

    pub(super) fn last_covered_x(search: TriangleRowStateSearch) -> Option<usize> {
        let mut end = search.candidate_end_x;
        while end > search.candidate_start_x {
            let start = end.saturating_sub(4).max(search.candidate_start_x);
            let covered =
                covered_lanes(search, start - search.candidate_start_x) & lane_mask(end - start);
            if covered != 0 {
                return Some(start + (u32::BITS - 1 - covered.leading_zeros()) as usize);
            }
            end = start;
        }
        None
    }
}

fn triangle_row_state_covers_pixel(
    coverage: TriangleCoverage,
    edge0: f32,
    edge1: f32,
    edge2: f32,
) -> bool {
    edge_covers_pixel(edge0 * coverage.inv_area, coverage.includes_boundary.edge0)
        && edge_covers_pixel(edge1 * coverage.inv_area, coverage.includes_boundary.edge1)
        && edge_covers_pixel(edge2 * coverage.inv_area, coverage.includes_boundary.edge2)
}

fn triangle_hint_needs_fallback(
    search: &TriangleRowSearch<'_>,
    span_start: Option<usize>,
    endpoint_probe_px: &mut usize,
) -> bool {
    if span_start.is_none() {
        return true;
    }
    if search.probe_start_x > search.candidate_start_x {
        if search.collect_stats {
            *endpoint_probe_px += 1;
        }
        if triangle_covers_pixel(
            search.probe_start_x - 1,
            search.y,
            search.vertices,
            search.coverage,
        ) {
            return true;
        }
    }
    if search.probe_end_x < search.candidate_end_x {
        if search.collect_stats {
            *endpoint_probe_px += 1;
        }
        if triangle_covers_pixel(
            search.probe_end_x,
            search.y,
            search.vertices,
            search.coverage,
        ) {
            return true;
        }
    }
    false
}

fn triangle_first_covered_x(
    start_x: usize,
    end_x: usize,
    y: usize,
    vertices: TriangleVertices<'_>,
    coverage: TriangleCoverage,
    collect_stats: bool,
) -> (Option<usize>, usize) {
    let mut probe_px = 0;
    for x in start_x..end_x {
        if collect_stats {
            probe_px += 1;
        }
        if triangle_covers_pixel(x, y, vertices, coverage) {
            return (Some(x), probe_px);
        }
    }
    (None, probe_px)
}

fn triangle_last_covered_x(
    start_x: usize,
    end_x: usize,
    y: usize,
    vertices: TriangleVertices<'_>,
    coverage: TriangleCoverage,
    collect_stats: bool,
) -> (Option<usize>, usize) {
    let mut probe_px = 0;
    for x in (start_x..end_x).rev() {
        if collect_stats {
            probe_px += 1;
        }
        if triangle_covers_pixel(x, y, vertices, coverage) {
            return (Some(x), probe_px);
        }
    }
    (None, probe_px)
}

pub(super) fn triangle_hint_x_range(
    vertices: TriangleVertices<'_>,
    inv_area: f32,
    bounds: TriangleRasterBounds,
    pixel_center_y: f32,
    candidate_start_x: usize,
    candidate_end_x: usize,
) -> Option<(usize, usize)> {
    let TriangleVertices { v0, v1, v2 } = vertices;
    let mut min_center_x = f32::NEG_INFINITY;
    let mut max_center_x = f32::INFINITY;
    for (a, b) in [
        (v1.pos2(), v2.pos2()),
        (v2.pos2(), v0.pos2()),
        (v0.pos2(), v1.pos2()),
    ] {
        let slope_x = edge_step_x(a, b) * inv_area;
        let at_origin = edge(a, b, pos2(0.0, pixel_center_y)) * inv_area;
        if !slope_x.is_finite() || !at_origin.is_finite() {
            return None;
        }
        if same_f32(slope_x, 0.0) {
            if at_origin < 0.0 {
                return None;
            }
            continue;
        }
        let boundary_x = -at_origin / slope_x;
        if !boundary_x.is_finite() {
            return None;
        }
        if slope_x > 0.0 {
            min_center_x = min_center_x.max(boundary_x);
        } else {
            max_center_x = max_center_x.min(boundary_x);
        }
    }
    if !min_center_x.is_finite() || !max_center_x.is_finite() || min_center_x > max_center_x {
        return None;
    }

    let start_x = f32_to_usize_ceil_clamped(min_center_x - 0.5, bounds.max_x)
        .max(candidate_start_x)
        .saturating_sub(TRIANGLE_SCANLINE_NARROWING_GUARD_PX)
        .max(candidate_start_x)
        .max(bounds.min_x);
    let end_x = f32_to_usize_floor_clamped(max_center_x - 0.5, bounds.max_x)
        .saturating_add(1 + TRIANGLE_SCANLINE_NARROWING_GUARD_PX)
        .min(candidate_end_x)
        .min(bounds.max_x)
        .max(start_x);
    if start_x >= end_x {
        return None;
    }
    Some((start_x, end_x))
}

fn triangle_covers_pixel(
    x: usize,
    y: usize,
    vertices: TriangleVertices<'_>,
    coverage: TriangleCoverage,
) -> bool {
    let TriangleVertices { v0, v1, v2 } = vertices;
    let pixel_center = pos2(usize_to_f32(x) + 0.5, usize_to_f32(y) + 0.5);
    let w0 = edge(v1.pos2(), v2.pos2(), pixel_center) * coverage.inv_area;
    let w1 = edge(v2.pos2(), v0.pos2(), pixel_center) * coverage.inv_area;
    let w2 = edge(v0.pos2(), v1.pos2(), pixel_center) * coverage.inv_area;
    edge_covers_pixel(w0, coverage.includes_boundary.edge0)
        && edge_covers_pixel(w1, coverage.includes_boundary.edge1)
        && edge_covers_pixel(w2, coverage.includes_boundary.edge2)
}

pub(super) fn triangle_scanline_x_range(
    positions: [Pos2; 3],
    bounds: TriangleRasterBounds,
    pixel_center_y: f32,
) -> (usize, usize) {
    ScanlineEdges::new(positions).x_range(bounds, pixel_center_y)
}

/// A triangle's edges, prepared once for [`ScanlineEdges::x_range`] on every row. Everything
/// here depends only on the triangle, so hoisting it out of the row loop computes the same
/// values; the per-row intersection keeps its exact arithmetic.
#[derive(Clone, Copy)]
pub(super) struct ScanlineEdges {
    edges: [Option<ScanlineEdge>; 3],
}

#[derive(Clone, Copy)]
struct ScanlineEdge {
    start: Pos2,
    delta_x: f32,
    delta_y: f32,
    min_y: f32,
    max_y: f32,
}

impl ScanlineEdges {
    pub(super) fn new(positions: [Pos2; 3]) -> Self {
        let edge = |a: Pos2, b: Pos2| {
            (!same_f32(a.y, b.y)).then(|| ScanlineEdge {
                start: a,
                delta_x: b.x - a.x,
                delta_y: b.y - a.y,
                min_y: a.y.min(b.y),
                max_y: a.y.max(b.y),
            })
        };
        Self {
            edges: [
                edge(positions[0], positions[1]),
                edge(positions[1], positions[2]),
                edge(positions[2], positions[0]),
            ],
        }
    }

    /// The candidate pixel range for the row whose centers sit at `pixel_center_y`: the span
    /// between where the edges cross it, widened by a guard band and clamped to the bounds, or
    /// the whole bounds when the crossings cannot be trusted.
    pub(super) fn x_range(
        self,
        bounds: TriangleRasterBounds,
        pixel_center_y: f32,
    ) -> (usize, usize) {
        let mut intersections = [0.0; 3];
        let mut count = 0;
        for edge in self.edges.into_iter().flatten() {
            if pixel_center_y < edge.min_y || pixel_center_y > edge.max_y {
                continue;
            }
            let t = (pixel_center_y - edge.start.y) / edge.delta_y;
            let intersection = edge.start.x + edge.delta_x * t;
            if !intersection.is_finite() {
                return (bounds.min_x, bounds.max_x);
            }
            intersections[count] = intersection;
            count += 1;
        }
        if count < 2 {
            return (bounds.min_x, bounds.max_x);
        }

        let mut min_x = intersections[0];
        let mut max_x = intersections[0];
        for intersection in intersections.iter().take(count).skip(1) {
            min_x = min_x.min(*intersection);
            max_x = max_x.max(*intersection);
        }

        let start_x = f32_to_usize_floor_clamped(min_x - 0.5, bounds.max_x)
            .max(bounds.min_x)
            .saturating_sub(TRIANGLE_SCANLINE_NARROWING_GUARD_PX)
            .max(bounds.min_x);
        let end_x = f32_to_usize_ceil_clamped(max_x - 0.5, bounds.max_x)
            .saturating_add(1 + TRIANGLE_SCANLINE_NARROWING_GUARD_PX)
            .min(bounds.max_x)
            .max(start_x);
        if start_x >= end_x {
            return (bounds.min_x, bounds.max_x);
        }
        (start_x, end_x)
    }
}

#[cfg(all(test, target_arch = "aarch64", target_endian = "little"))]
mod neon_row_scan_tests {
    use super::*;
    use crate::geometry::pos2;

    #[test]
    fn neon_row_scans_match_scalar_scans() {
        let mut state = 0x9e37_79b9_u32;
        let mut unit = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            f32::from(u16::try_from(state >> 16).unwrap_or(0)) / 65_535.0
        };
        let mut rows = 0;
        for case in 0..20_000_u32 {
            let (scale_x, scale_y) = if case % 2 == 0 {
                (400.0, 3.0)
            } else {
                (120.0, 90.0)
            };
            let mut point = || pos2(unit() * scale_x, unit() * scale_y);
            let (a, b, c) = (point(), point(), point());
            let area = edge(a, b, c);
            if area == 0.0 || !area.is_finite() {
                continue;
            }
            let coverage = TriangleCoverage {
                inv_area: 1.0 / area,
                includes_boundary: TriangleBoundaryIncludes {
                    edge0: unit() < 0.5,
                    edge1: unit() < 0.5,
                    edge2: unit() < 0.5,
                },
            };
            // Some rows start past u16::MAX, where offsets clamp.
            let start_x = if case % 11 == 0 { 65_530 } else { 0 };
            let end_x = start_x + 1 + usize::try_from(case % 61).unwrap_or(0);
            for y in 0..4 {
                let pixel = pos2(0.5, usize_to_f32(y) + 0.5);
                let search = |collect_stats| TriangleRowStateSearch {
                    coverage,
                    row_start_edges: (edge(b, c, pixel), edge(c, a, pixel), edge(a, b, pixel)),
                    x_steps: (edge_step_x(b, c), edge_step_x(c, a), edge_step_x(a, b)),
                    candidate_start_x: start_x,
                    candidate_end_x: end_x,
                    collect_stats,
                };
                assert_eq!(
                    triangle_row_state_first_covered_x(search(false)).0,
                    triangle_row_state_first_covered_x(search(true)).0,
                    "first a={a:?} b={b:?} c={c:?} y={y} start={start_x} end={end_x}"
                );
                assert_eq!(
                    triangle_row_state_last_covered_x(search(false)).0,
                    triangle_row_state_last_covered_x(search(true)).0,
                    "last a={a:?} b={b:?} c={c:?} y={y} start={start_x} end={end_x}"
                );
                rows += 1;
            }
        }
        assert!(rows > 50_000);
    }
}
