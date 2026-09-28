// SPDX-License-Identifier: MIT OR Apache-2.0

//! Benchmark of the drawing API alone, with no egui anywhere.
//!
//! `cargo bench --bench draw` draws each workload a few hundred times into a 640x480 surface and
//! reports microseconds per frame. `--frames N` sets the sample count. Without `--bench` (the
//! way `cargo test --all-targets` runs it) each workload draws once.
//!
//! - `glyphs`: a page of 7x9 textured rectangles cut from a coverage atlas and tinted, the
//!   shape text takes once a UI toolkit has laid it out.
//! - `panels`: stacked opaque and translucent rectangles, the backgrounds and frames of a UI.
//! - `mesh`: a 256-triangle gradient fan, the shape of a filled rounded panel or a chart.

use std::time::Instant;

use dream_soft_render::{ClipRect, Color, Error, Frame, Mesh, Rect, SoftwareRenderer, Vertex};

const WIDTH: usize = 640;
const HEIGHT: usize = 480;
const ATLAS_WIDTH: usize = 256;
const ATLAS_HEIGHT: usize = 64;
const GLYPH_WIDTH: usize = 7;
const GLYPH_HEIGHT: usize = 9;

type DrawFn = fn(&mut Frame<'_>, &Workloads) -> Result<(), Error>;

struct Workloads {
    atlas: dream_soft_render::TextureId,
    fan_vertices: Vec<Vertex>,
    fan_indices: Vec<u32>,
}

fn main() -> Result<(), Error> {
    let mut frames = 300_usize;
    let mut bench_mode = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bench" => bench_mode = true,
            "--frames" => {
                frames = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .expect("--frames N");
            }
            _ => {}
        }
    }
    if !bench_mode {
        frames = 1;
    }

    let mut renderer = SoftwareRenderer::default();
    let workloads = Workloads {
        atlas: renderer.create_texture(ATLAS_WIDTH, ATLAS_HEIGHT, &coverage_atlas())?,
        fan_vertices: fan_vertices(),
        fan_indices: fan_indices(),
    };
    let draws: [(&str, DrawFn); 3] = [
        ("glyphs", draw_glyphs),
        ("panels", draw_panels),
        ("mesh", draw_mesh),
    ];
    for (name, draw) in draws {
        let mut samples = Vec::with_capacity(frames);
        for _ in 0..frames {
            let start = Instant::now();
            let mut frame = renderer.begin_frame(WIDTH, HEIGHT)?;
            frame.clear(Color::from_rgb(17, 20, 28));
            draw(&mut frame, &workloads)?;
            samples.push(start.elapsed().as_micros());
        }
        samples.sort_unstable();
        let at = |tenths: usize| samples[(samples.len() - 1) * tenths / 10];
        println!(
            "{name:<8} frame_us p50={:>7} p10={:>7} p90={:>7}",
            at(5),
            at(1),
            at(9)
        );
    }
    Ok(())
}

/// Premultiplied white with a deterministic coverage pattern, like a font atlas: mostly
/// transparent, some translucent edges, some opaque stems.
fn coverage_atlas() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(ATLAS_WIDTH * ATLAS_HEIGHT * 4);
    for y in 0..ATLAS_HEIGHT {
        for x in 0..ATLAS_WIDTH {
            let coverage = match (x * 7 + y * 13) % 11 {
                0..=4 => 0,
                5..=7 => u8::try_from((x * 37 + y * 11) % 255).unwrap_or(u8::MAX),
                _ => u8::MAX,
            };
            pixels.extend_from_slice(&[coverage; 4]);
        }
    }
    pixels
}

fn usize_to_f32(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

fn draw_glyphs(frame: &mut Frame<'_>, workloads: &Workloads) -> Result<(), Error> {
    let tint = Color::from_rgb(220, 224, 230);
    let columns = ATLAS_WIDTH / GLYPH_WIDTH;
    let rows = ATLAS_HEIGHT / GLYPH_HEIGHT;
    for line in 0..40 {
        for column in 0..80 {
            let glyph = (line * 31 + column * 7) % (columns * rows);
            let (u, v) = (
                (glyph % columns) * GLYPH_WIDTH,
                (glyph / columns) * GLYPH_HEIGHT,
            );
            // Texel centers, the way egui addresses its font atlas.
            let uv = Rect::from_min_max(
                [
                    (usize_to_f32(u) + 0.5) / usize_to_f32(ATLAS_WIDTH - 1),
                    (usize_to_f32(v) + 0.5) / usize_to_f32(ATLAS_HEIGHT - 1),
                ],
                [
                    (usize_to_f32(u + GLYPH_WIDTH) - 0.5) / usize_to_f32(ATLAS_WIDTH - 1),
                    (usize_to_f32(v + GLYPH_HEIGHT) - 0.5) / usize_to_f32(ATLAS_HEIGHT - 1),
                ],
            );
            let rect = Rect::from_min_size(
                [
                    usize_to_f32(4 + column * (GLYPH_WIDTH + 1)),
                    usize_to_f32(4 + line * (GLYPH_HEIGHT + 3)),
                ],
                [usize_to_f32(GLYPH_WIDTH), usize_to_f32(GLYPH_HEIGHT)],
            );
            frame.textured_rect(rect, uv, workloads.atlas, tint, ClipRect::ALL)?;
        }
    }
    Ok(())
}

fn draw_panels(frame: &mut Frame<'_>, _: &Workloads) -> Result<(), Error> {
    for panel in 0..48 {
        let x = usize_to_f32((panel % 8) * 80 + 4);
        let y = usize_to_f32((panel / 8) * 80 + 4);
        let color = if panel % 3 == 0 {
            Color::from_rgb(40, 44, 58)
        } else {
            Color::from_rgba_unmultiplied(90, 140, 220, 96)
        };
        frame.fill_rect(
            Rect::from_min_size([x, y], [120.0, 60.0]),
            color,
            ClipRect::ALL,
        )?;
    }
    Ok(())
}

fn fan_vertices() -> Vec<Vertex> {
    let center = [320.0, 240.0];
    let mut vertices = vec![Vertex::new(center, [0.0, 0.0], Color::WHITE)];
    for step in 0..=256_u16 {
        let angle = f32::from(step) / 256.0 * std::f32::consts::TAU;
        let shade = u8::try_from(step % 256).unwrap_or(u8::MAX);
        vertices.push(Vertex::new(
            [
                center[0] + 200.0 * angle.cos(),
                center[1] + 180.0 * angle.sin(),
            ],
            [0.0, 0.0],
            Color::from_rgba_unmultiplied(shade, 255 - shade, 128, 220),
        ));
    }
    vertices
}

fn fan_indices() -> Vec<u32> {
    (1..=256).flat_map(|edge| [0, edge, edge + 1]).collect()
}

fn draw_mesh(frame: &mut Frame<'_>, workloads: &Workloads) -> Result<(), Error> {
    frame.mesh(
        Mesh {
            vertices: &workloads.fan_vertices,
            indices: &workloads.fan_indices,
            texture: None,
        },
        ClipRect::ALL,
    )
}
