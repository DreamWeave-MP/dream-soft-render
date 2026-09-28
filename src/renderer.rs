// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::Vertex;
use crate::geometry::RasterMesh;
use std::io;
use std::time::Instant;

#[cfg(feature = "egui")]
mod egui_frame;
mod fan;
mod frame;
mod quad;
mod stats;

#[cfg(feature = "egui")]
pub use egui_frame::{RenderFrame, RenderOutcome, RenderTimings, TextureEvidence};
pub use frame::{Frame, Mesh};

use super::raster::{
    ClipBounds, RasterStats, SolidFanRasterParams, SolidFanSpanCache, classify_triangle,
    polygon_raster_bounds, rasterize_solid_fan_with_cache, rasterize_triangle,
};
use super::surface::SoftwareSurface;
use super::texture::{TextureImage, TextureStore};
use fan::{
    FanBoundaryKey, SOLID_FAN_MIN_TRIANGLES, SolidFanPolygonScratch, SolidFanRun, solid_fan_run,
};
use quad::try_rasterize_quad_window;
use stats::{
    PrimitiveStats, RasterTimings, SolidFanRasterRecord, SolidFanRasterWork, TriangleSource,
};

/// A CPU rasterizer that owns an RGBA8 [`SoftwareSurface`] and a texture store.
///
/// Draw with [`SoftwareRenderer::begin_frame`], or, with the `egui` feature, run egui frames
/// through `SoftwareRenderer::render_egui`. Both write the same surface.
#[derive(Debug)]
pub struct SoftwareRenderer {
    surface: SoftwareSurface,
    textures: TextureStore,
    solid_fan_polygon_scratch: Vec<usize>,
    solid_fan_seen_boundary_scratch: Vec<FanBoundaryKey>,
    solid_fan_span_cache: SolidFanSpanCache,
    #[cfg(feature = "egui")]
    skip_unchanged_frames: bool,
    // Primitives last rasterized into `surface`, valid only while `previous_frame_valid`.
    #[cfg(feature = "egui")]
    previous_primitives: Vec<egui::ClippedPrimitive>,
    #[cfg(feature = "egui")]
    previous_frame_valid: bool,
}

const SOLID_FAN_POLYGON_SCRATCH_CAPACITY: usize = 4096;

impl Default for SoftwareRenderer {
    fn default() -> Self {
        Self {
            surface: SoftwareSurface::default(),
            textures: TextureStore::default(),
            solid_fan_polygon_scratch: Vec::with_capacity(SOLID_FAN_POLYGON_SCRATCH_CAPACITY),
            solid_fan_seen_boundary_scratch: Vec::with_capacity(SOLID_FAN_POLYGON_SCRATCH_CAPACITY),
            solid_fan_span_cache: SolidFanSpanCache::default(),
            #[cfg(feature = "egui")]
            skip_unchanged_frames: true,
            #[cfg(feature = "egui")]
            previous_primitives: Vec::new(),
            #[cfg(feature = "egui")]
            previous_frame_valid: false,
        }
    }
}

impl SoftwareRenderer {
    /// The surface holding the most recently rendered frame.
    #[must_use]
    pub const fn surface(&self) -> &SoftwareSurface {
        &self.surface
    }

    /// Drawing or changing textures outside egui leaves the last egui frame's pixels stale, so
    /// the next egui frame must rasterize even if egui's output did not change.
    #[cfg_attr(not(feature = "egui"), allow(clippy::unused_self))]
    const fn forget_last_egui_frame(&mut self) {
        #[cfg(feature = "egui")]
        {
            self.previous_frame_valid = false;
        }
    }
}

fn rasterize_mesh_contents(
    context: &mut MeshRasterContext<'_>,
    stats: &mut Option<&mut PrimitiveStats>,
    raster_stats: &mut Option<&mut RasterStats>,
    raster_timings: &mut Option<&mut RasterTimings>,
) -> io::Result<()> {
    let mut index_offset = 0;
    while index_offset + 2 < context.mesh.indices.len() {
        if context.try_rasterize_quad_at(index_offset, stats, raster_stats, raster_timings)? {
            index_offset += 6;
            continue;
        }

        if let Some(fan_triangle_count) =
            context.try_rasterize_solid_fan_at(index_offset, stats, raster_stats, raster_timings)?
        {
            index_offset += fan_triangle_count * 3;
            continue;
        }

        context.rasterize_generic_triangle_at(index_offset, stats, raster_stats, raster_timings)?;
        index_offset += 3;
    }
    Ok(())
}

struct MeshRasterContext<'a> {
    surface: &'a mut SoftwareSurface,
    fan_polygon_scratch: &'a mut Vec<usize>,
    fan_seen_boundary_scratch: &'a mut Vec<FanBoundaryKey>,
    fan_span_cache: &'a mut SolidFanSpanCache,
    mesh: RasterMesh<'a>,
    texture: &'a TextureImage,
    clip: ClipBounds,
    primitive_index: usize,
    solid_fan_polygon_scratch_budget: usize,
}

#[cfg(feature = "egui")]
fn mesh_quad_vertices<'a>(
    mesh: RasterMesh<'a>,
    quad: &[u32],
) -> io::Result<Option<[&'a Vertex; 6]>> {
    let i0 = mesh_index_to_usize(quad[0])?;
    let i1 = mesh_index_to_usize(quad[1])?;
    let i2 = mesh_index_to_usize(quad[2])?;
    let i3 = mesh_index_to_usize(quad[3])?;
    let i4 = mesh_index_to_usize(quad[4])?;
    let i5 = mesh_index_to_usize(quad[5])?;
    let (Some(v0), Some(v1), Some(v2), Some(v3), Some(v4), Some(v5)) = (
        mesh.vertices.get(i0),
        mesh.vertices.get(i1),
        mesh.vertices.get(i2),
        mesh.vertices.get(i3),
        mesh.vertices.get(i4),
        mesh.vertices.get(i5),
    ) else {
        return Ok(None);
    };
    Ok(Some([v0, v1, v2, v3, v4, v5]))
}

impl MeshRasterContext<'_> {
    fn try_rasterize_quad_at(
        &mut self,
        index_offset: usize,
        stats: &mut Option<&mut PrimitiveStats>,
        raster_stats: &mut Option<&mut RasterStats>,
        raster_timings: &mut Option<&mut RasterTimings>,
    ) -> io::Result<bool> {
        if index_offset + 5 >= self.mesh.indices.len() {
            return Ok(false);
        }
        let quad = &self.mesh.indices[index_offset..index_offset + 6];
        let mut instrumentation = RasterInstrumentation {
            primitive_stats: borrow_optional_mut(stats),
            raster_stats: borrow_optional_mut(raster_stats),
            timings: borrow_optional_mut(raster_timings),
        };
        try_rasterize_quad_window(
            self.surface,
            self.mesh,
            self.texture,
            self.clip,
            quad,
            self.source(index_offset),
            &mut instrumentation,
        )
    }

    fn first_triangle_has_mixed_vertex_colors(&self, index_offset: usize) -> bool {
        let color_at = |slot: usize| {
            usize::try_from(self.mesh.indices[index_offset + slot])
                .ok()
                .and_then(|index| self.mesh.vertices.get(index))
                .map(|vertex| vertex.color)
        };
        match (color_at(0), color_at(1), color_at(2)) {
            (Some(c0), Some(c1), Some(c2)) => c0 != c1 || c0 != c2,
            // Leave malformed indices to the full probe, which reports them.
            _ => false,
        }
    }

    fn try_rasterize_solid_fan_at(
        &mut self,
        index_offset: usize,
        stats: &mut Option<&mut PrimitiveStats>,
        raster_stats: &mut Option<&mut RasterStats>,
        raster_timings: &mut Option<&mut RasterTimings>,
    ) -> io::Result<Option<usize>> {
        if self.remaining_triangles_at(index_offset) < SOLID_FAN_MIN_TRIANGLES {
            if let Some(stats) = borrow_optional_mut(stats) {
                stats.solid_fan_probe.preflight_reject_too_few_triangles += 1;
            }
            return Ok(None);
        }
        // Every triangle of a solid fan has one vertex color, so a first triangle with mixed
        // colors (feathered edges, gradients) rejects before any geometry is examined.
        if self.first_triangle_has_mixed_vertex_colors(index_offset) {
            if let Some(stats) = borrow_optional_mut(stats) {
                stats.solid_fan_probe.preflight_reject_mixed_vertex_colors += 1;
            }
            return Ok(None);
        }

        let fan_probe_start = raster_timings.as_ref().map(|_| Instant::now());
        let fan = solid_fan_run(
            self.mesh,
            self.texture,
            self.clip,
            index_offset,
            SolidFanPolygonScratch {
                polygon: &mut *self.fan_polygon_scratch,
                seen_boundaries: &mut *self.fan_seen_boundary_scratch,
                budget: self.solid_fan_polygon_scratch_budget,
            },
            stats.as_deref_mut().map(|stats| &mut stats.solid_fan_probe),
        )?;
        let fan_accepted = fan.is_some();
        if let (Some(start), Some(timings)) = (fan_probe_start, borrow_optional_mut(raster_timings))
        {
            timings.record_solid_fan_probe(fan_accepted, start.elapsed().as_micros());
        }
        let Some(fan) = fan else {
            self.fan_polygon_scratch.clear();
            return Ok(None);
        };
        let mut instrumentation = RasterInstrumentation {
            primitive_stats: borrow_optional_mut(stats),
            raster_stats: borrow_optional_mut(raster_stats),
            timings: borrow_optional_mut(raster_timings),
        };
        let source = self.source(index_offset);
        rasterize_solid_fan_run(
            SolidFanRasterRunContext {
                surface: self.surface,
                mesh: self.mesh,
                fan_span_cache: self.fan_span_cache,
                clip: self.clip,
                source,
            },
            &fan,
            self.fan_polygon_scratch,
            &mut instrumentation,
        );
        let fan_triangle_count = fan.triangle_count;
        self.fan_polygon_scratch.clear();
        Ok(Some(fan_triangle_count))
    }

    fn remaining_triangles_at(&self, index_offset: usize) -> usize {
        self.mesh.indices[index_offset..].len() / 3
    }

    fn rasterize_generic_triangle_at(
        &mut self,
        index_offset: usize,
        stats: &mut Option<&mut PrimitiveStats>,
        raster_stats: &mut Option<&mut RasterStats>,
        raster_timings: &mut Option<&mut RasterTimings>,
    ) -> io::Result<()> {
        let Some([v0, v1, v2]) = mesh_triangle_vertices(self.mesh, index_offset)? else {
            return Ok(());
        };
        let source = self.source(index_offset);
        if let Some(stats) = borrow_optional_mut(stats) {
            stats.record_generic_triangle(v0, v1, v2, self.texture, self.clip, source);
        }
        rasterize_generic_triangle(
            self.surface,
            [v0, v1, v2],
            self.texture,
            self.clip,
            raster_stats,
            raster_timings,
        );
        Ok(())
    }

    const fn source(&self, index_offset: usize) -> TriangleSource {
        TriangleSource {
            primitive_index: self.primitive_index,
            mesh_index_offset: index_offset,
        }
    }
}

fn mesh_triangle_vertices(
    mesh: RasterMesh<'_>,
    index_offset: usize,
) -> io::Result<Option<[&Vertex; 3]>> {
    let triangle = &mesh.indices[index_offset..index_offset + 3];
    let i0 = mesh_index_to_usize(triangle[0])?;
    let i1 = mesh_index_to_usize(triangle[1])?;
    let i2 = mesh_index_to_usize(triangle[2])?;
    let Some(v0) = mesh.vertices.get(i0) else {
        return Ok(None);
    };
    let Some(v1) = mesh.vertices.get(i1) else {
        return Ok(None);
    };
    let Some(v2) = mesh.vertices.get(i2) else {
        return Ok(None);
    };
    Ok(Some([v0, v1, v2]))
}

fn rasterize_generic_triangle(
    surface: &mut SoftwareSurface,
    vertices: [&Vertex; 3],
    texture: &TextureImage,
    clip: ClipBounds,
    raster_stats: &mut Option<&mut RasterStats>,
    raster_timings: &mut Option<&mut RasterTimings>,
) {
    let [v0, v1, v2] = vertices;
    let triangle_classification = raster_timings
        .as_ref()
        .map(|_| classify_triangle(v0, v1, v2, texture));
    let triangle_start = raster_timings.as_ref().map(|_| Instant::now());
    rasterize_triangle(
        surface,
        v0,
        v1,
        v2,
        texture,
        clip,
        borrow_optional_mut(raster_stats),
    );
    if let (Some(start), Some(classification), Some(timings)) = (
        triangle_start,
        triangle_classification,
        borrow_optional_mut(raster_timings),
    ) {
        timings.record_generic_triangle(classification, start.elapsed().as_micros());
    }
}

fn rasterize_solid_fan_run(
    context: SolidFanRasterRunContext<'_>,
    fan: &SolidFanRun,
    fan_polygon: &[usize],
    instrumentation: &mut RasterInstrumentation<'_>,
) {
    let SolidFanRasterRunContext {
        surface,
        mesh,
        fan_span_cache,
        clip,
        source,
    } = context;
    let fan_work_before = instrumentation
        .raster_stats
        .as_deref()
        .map(SolidFanRasterWork::from_stats);
    let fan_bounds = instrumentation
        .primitive_stats
        .as_ref()
        .and_then(|_| polygon_raster_bounds(mesh.vertices, &fan_polygon[..fan.polygon_len], clip));
    let fan_start = instrumentation.timing_start();
    rasterize_solid_fan_with_cache(
        surface,
        SolidFanRasterParams {
            vertices: mesh.vertices,
            polygon: &fan_polygon[..fan.polygon_len],
            triangle_count: fan.triangle_count,
            color: fan.color,
            clip,
        },
        &mut instrumentation.raster_stats(),
        Some(fan_span_cache),
    );
    let elapsed_us = fan_start.map_or(0, |start| start.elapsed().as_micros());
    let fan_work_after = instrumentation
        .raster_stats
        .as_deref()
        .map(SolidFanRasterWork::from_stats);
    let fan_work = fan_work_before
        .zip(fan_work_after)
        .map_or_else(SolidFanRasterWork::default, |(before, after)| {
            after - before
        });
    if let Some(stats) = instrumentation.primitive_stats() {
        stats.solid_fan_runs += 1;
        stats.solid_fan_triangles += fan.triangle_count;
        stats.record_solid_fan(SolidFanRasterRecord {
            source,
            triangle_count: fan.triangle_count,
            polygon_vertices: fan.polygon_len,
            alpha: fan.color[3],
            bounds: fan_bounds,
            work: fan_work,
            elapsed_us,
        });
    }
    if let Some(timings) = instrumentation.timings() {
        timings.solid_fan_raster += elapsed_us;
    }
}

struct SolidFanRasterRunContext<'a> {
    surface: &'a mut SoftwareSurface,
    mesh: RasterMesh<'a>,
    fan_span_cache: &'a mut SolidFanSpanCache,
    clip: ClipBounds,
    source: TriangleSource,
}

struct RasterInstrumentation<'a> {
    primitive_stats: Option<&'a mut PrimitiveStats>,
    raster_stats: Option<&'a mut RasterStats>,
    timings: Option<&'a mut RasterTimings>,
}

impl RasterInstrumentation<'_> {
    fn primitive_stats(&mut self) -> Option<&mut PrimitiveStats> {
        borrow_optional_mut(&mut self.primitive_stats)
    }

    fn raster_stats(&mut self) -> Option<&mut RasterStats> {
        borrow_optional_mut(&mut self.raster_stats)
    }

    fn timings(&mut self) -> Option<&mut RasterTimings> {
        borrow_optional_mut(&mut self.timings)
    }

    const fn collects_stats(&self) -> bool {
        self.primitive_stats.is_some() || self.raster_stats.is_some() || self.timings.is_some()
    }

    fn timing_start(&self) -> Option<Instant> {
        self.timings.as_ref().map(|_| Instant::now())
    }
}

fn mesh_index_to_usize(index: u32) -> io::Result<usize> {
    usize::try_from(index).map_err(|_| io::Error::other("mesh index does not fit usize"))
}

fn borrow_optional_mut<'a, T>(option: &'a mut Option<&mut T>) -> Option<&'a mut T> {
    option.as_mut().map(|value| &mut **value)
}
