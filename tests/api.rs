// SPDX-License-Identifier: MIT OR Apache-2.0

//! The generic drawing API: a golden scene that exercises every draw call, and the validation
//! contract for bad input.

use dream_soft_render::{
    ClipRect, Color, Error, MAX_SURFACE_PIXELS, MAX_TEXTURE_BYTES, Mesh, Rect, SoftwareRenderer,
    Vertex, VertexField,
};

const GOLDEN_SCENE_HASH: u64 = 0xdcb5_803b_00db_a499;

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn gradient_texture(width: usize, height: usize) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let alpha = u8::try_from((x * 255) / (width - 1)).unwrap_or(u8::MAX);
            let shade = u8::try_from((y * 255) / (height - 1)).unwrap_or(u8::MAX);
            pixels.extend_from_slice(
                &Color::from_rgba_unmultiplied(shade, 96, 255 - shade, alpha).to_array(),
            );
        }
    }
    pixels
}

fn draw_golden_scene(renderer: &mut SoftwareRenderer) -> Result<Vec<u8>, Error> {
    let badge = renderer.create_texture(16, 8, &gradient_texture(16, 8))?;
    let mut frame = renderer.begin_frame(160, 120)?;
    frame.clear(Color::from_rgb(17, 20, 28));
    frame.fill_rect(
        Rect::from_min_size([6.25, 5.5], [70.0, 30.0]),
        Color::from_rgb(40, 44, 58),
        ClipRect::ALL,
    )?;
    frame.fill_rect(
        Rect::from_min_size([40.0, 20.0], [90.0, 50.0]),
        Color::from_rgba_unmultiplied(255, 140, 0, 128),
        ClipRect::new(0, 0, 110, 60),
    )?;
    frame.textured_rect(
        Rect::from_min_size([10.0, 60.0], [64.0, 32.0]),
        Rect::FULL_UV,
        badge,
        Color::WHITE,
        ClipRect::ALL,
    )?;
    frame.textured_rect(
        Rect::from_min_size([90.0, 80.0], [48.0, 24.0]),
        Rect::from_min_max([1.0, 0.0], [0.0, 1.0]),
        badge,
        Color::from_rgba_unmultiplied(200, 255, 200, 200),
        ClipRect::ALL,
    )?;
    let vertices = [
        Vertex::new(
            [80.0, 8.0],
            [0.0, 0.0],
            Color::from_rgba_unmultiplied(255, 0, 0, 255),
        ),
        Vertex::new(
            [150.0, 30.0],
            [1.0, 0.0],
            Color::from_rgba_unmultiplied(0, 255, 0, 160),
        ),
        Vertex::new(
            [100.5, 70.25],
            [0.5, 1.0],
            Color::from_rgba_unmultiplied(0, 0, 255, 64),
        ),
        Vertex::new([155.0, 75.0], [1.0, 1.0], Color::WHITE),
    ];
    frame.mesh(
        Mesh {
            vertices: &vertices,
            indices: &[0, 1, 2, 2, 1, 3],
            texture: None,
        },
        ClipRect::ALL,
    )?;
    frame.mesh(
        Mesh {
            vertices: &vertices,
            indices: &[2, 1, 3],
            texture: Some(badge),
        },
        ClipRect::new(100, 40, 160, 120),
    )?;
    Ok(frame.surface().pixels.clone())
}

#[test]
fn generic_scene_matches_golden_hash() {
    let pixels = draw_golden_scene(&mut SoftwareRenderer::default()).expect("scene draws");
    assert_eq!(
        fnv1a(&pixels),
        GOLDEN_SCENE_HASH,
        "hash=0x{:016x}",
        fnv1a(&pixels)
    );
}

#[test]
fn surface_size_is_validated() {
    let mut renderer = SoftwareRenderer::default();
    assert!(matches!(
        renderer.begin_frame(MAX_SURFACE_PIXELS + 1, 1),
        Err(Error::SurfaceSize { .. })
    ));
    assert!(matches!(
        renderer.begin_frame(usize::MAX, 2),
        Err(Error::SurfaceSize { .. })
    ));
    assert!(renderer.begin_frame(1280, 720).is_ok());
}

#[test]
fn texture_creation_is_validated() {
    let mut renderer = SoftwareRenderer::default();
    assert_eq!(
        renderer.create_texture(0, 4, &[]),
        Err(Error::TextureSize {
            width: 0,
            height: 4
        })
    );
    assert_eq!(
        renderer.create_texture(2, 2, &[0; 15]),
        Err(Error::PixelDataLength {
            expected: 16,
            actual: 15
        })
    );
    let side = 1024;
    let full = vec![0; side * side * 4];
    let first = renderer
        .create_texture(side, side, &full)
        .expect("4 MiB fits");
    let second = renderer
        .create_texture(side, side, &full)
        .expect("8 MiB fits");
    assert_eq!(
        renderer.create_texture(1, 1, &[0; 4]),
        Err(Error::TextureBudget {
            requested: MAX_TEXTURE_BYTES + 4,
            budget: MAX_TEXTURE_BYTES
        })
    );
    renderer.free_texture(first).expect("free");
    assert!(renderer.create_texture(1, 1, &[0; 4]).is_ok());
    assert_ne!(first, second);
}

#[test]
fn texture_updates_and_frees_are_validated() {
    let mut renderer = SoftwareRenderer::default();
    let texture = renderer.create_texture(4, 4, &[0; 64]).expect("texture");
    assert!(
        renderer
            .update_texture(texture, 2, 2, 2, 2, &[255; 16])
            .is_ok()
    );
    assert_eq!(
        renderer.update_texture(texture, 3, 0, 2, 1, &[0; 8]),
        Err(Error::TextureUpdateOutOfBounds(texture))
    );
    assert_eq!(
        renderer.update_texture(texture, 0, 0, 2, 2, &[0; 8]),
        Err(Error::PixelDataLength {
            expected: 16,
            actual: 8
        })
    );
    renderer.free_texture(texture).expect("free");
    assert_eq!(
        renderer.free_texture(texture),
        Err(Error::UnknownTexture(texture))
    );
    assert_eq!(
        renderer.update_texture(texture, 0, 0, 1, 1, &[0; 4]),
        Err(Error::UnknownTexture(texture))
    );
    let mut frame = renderer.begin_frame(8, 8).expect("frame");
    assert_eq!(
        frame.textured_rect(
            Rect::from_min_size([0.0, 0.0], [4.0, 4.0]),
            Rect::FULL_UV,
            texture,
            Color::WHITE,
            ClipRect::ALL
        ),
        Err(Error::UnknownTexture(texture))
    );
}

#[test]
fn meshes_and_rects_are_validated() {
    let mut renderer = SoftwareRenderer::default();
    let mut frame = renderer.begin_frame(8, 8).expect("frame");
    let vertices = [Vertex::default(); 3];
    assert_eq!(
        frame.mesh(
            Mesh {
                vertices: &vertices,
                indices: &[0, 1],
                texture: None
            },
            ClipRect::ALL
        ),
        Err(Error::IndexCount(2))
    );
    assert_eq!(
        frame.mesh(
            Mesh {
                vertices: &vertices,
                indices: &[0, 1, 3],
                texture: None
            },
            ClipRect::ALL
        ),
        Err(Error::IndexOutOfRange {
            index: 3,
            vertex_count: 3
        })
    );
    assert_eq!(
        frame.fill_rect(
            Rect::from_min_max([0.0, f32::NAN], [4.0, 4.0]),
            Color::WHITE,
            ClipRect::ALL
        ),
        Err(Error::NonFiniteRect)
    );
}

#[test]
fn empty_rects_and_clips_draw_nothing() {
    let mut renderer = SoftwareRenderer::default();
    let mut frame = renderer.begin_frame(8, 8).expect("frame");
    frame.clear(Color::BLACK);
    let before = frame.surface().pixels.clone();
    let inverted = Rect::from_min_max([6.0, 6.0], [2.0, 2.0]);
    frame
        .fill_rect(inverted, Color::WHITE, ClipRect::ALL)
        .expect("empty rect");
    let rect = Rect::from_min_size([0.0, 0.0], [8.0, 8.0]);
    frame
        .fill_rect(rect, Color::WHITE, ClipRect::new(5, 5, 5, 8))
        .expect("empty clip");
    frame
        .fill_rect(rect, Color::WHITE, ClipRect::new(20, 0, 30, 8))
        .expect("clip past surface");
    assert_eq!(frame.surface().pixels, before);
}

#[test]
fn fill_rect_covers_pixel_centers_inside_the_rect() {
    let mut renderer = SoftwareRenderer::default();
    let mut frame = renderer.begin_frame(6, 1).expect("frame");
    frame.clear(Color::BLACK);
    // Centers at 0.5 .. 5.5: 1.5 and 2.5 lie in (0.5, 2.5]; 0.5 does not.
    frame
        .fill_rect(
            Rect::from_min_max([0.5, 0.0], [2.5, 1.0]),
            Color::WHITE,
            ClipRect::ALL,
        )
        .expect("rect");
    let lit: Vec<bool> = frame
        .surface()
        .pixels
        .chunks(4)
        .map(|pixel| pixel[0] == 255)
        .collect();
    assert_eq!(lit, [false, true, true, false, false, false]);
}

#[test]
fn clear_overwrites_and_blending_writes_opaque_alpha() {
    let mut renderer = SoftwareRenderer::default();
    let mut frame = renderer.begin_frame(2, 1).expect("frame");
    frame.clear(Color::TRANSPARENT);
    assert_eq!(frame.surface().pixels, [0; 8]);
    frame
        .fill_rect(
            Rect::from_min_max([0.0, 0.0], [1.0, 1.0]),
            Color::from_rgba_premultiplied(100, 50, 0, 128),
            ClipRect::ALL,
        )
        .expect("rect");
    assert_eq!(&frame.surface().pixels[..4], &[100, 50, 0, 255]);
    assert_eq!(&frame.surface().pixels[4..], &[0, 0, 0, 0]);
}

#[test]
fn texture_handles_do_not_alias_across_renderers() {
    let mut first = SoftwareRenderer::default();
    let mut second = SoftwareRenderer::default();
    let first_texture = first.create_texture(1, 1, &[255; 4]).expect("texture");
    let second_texture = second.create_texture(1, 1, &[255; 4]).expect("texture");
    assert_ne!(first_texture, second_texture);

    assert_eq!(
        second.free_texture(first_texture),
        Err(Error::UnknownTexture(first_texture))
    );
    assert_eq!(
        second.update_texture(first_texture, 0, 0, 1, 1, &[0; 4]),
        Err(Error::UnknownTexture(first_texture))
    );
    let mut frame = second.begin_frame(4, 4).expect("frame");
    assert_eq!(
        frame.textured_rect(
            Rect::from_min_size([0.0, 0.0], [4.0, 4.0]),
            Rect::FULL_UV,
            first_texture,
            Color::WHITE,
            ClipRect::ALL
        ),
        Err(Error::UnknownTexture(first_texture))
    );
    second
        .free_texture(second_texture)
        .expect("own texture frees");
    first
        .free_texture(first_texture)
        .expect("own texture frees");
}

#[test]
fn non_finite_vertices_are_rejected() {
    let mut renderer = SoftwareRenderer::default();
    let mut frame = renderer.begin_frame(8, 8).expect("frame");
    frame.clear(Color::BLACK);
    let before = frame.surface().pixels.clone();
    let good = Vertex::new([1.0, 1.0], [0.0, 0.0], Color::WHITE);
    let cases = [
        (
            Vertex::new([f32::NAN, 1.0], [0.0, 0.0], Color::WHITE),
            VertexField::Position,
        ),
        (
            Vertex::new([1.0, f32::INFINITY], [0.0, 0.0], Color::WHITE),
            VertexField::Position,
        ),
        (
            Vertex::new([1.0, 1.0], [f32::NEG_INFINITY, 0.0], Color::WHITE),
            VertexField::Uv,
        ),
        (
            Vertex::new([1.0, 1.0], [0.0, f32::NAN], Color::WHITE),
            VertexField::Uv,
        ),
    ];
    for (bad, field) in cases {
        // The bad vertex is not referenced by any triangle; it is still malformed input.
        let vertices = [
            good,
            Vertex::new([7.0, 1.0], [0.0, 0.0], Color::WHITE),
            good,
            bad,
        ];
        assert_eq!(
            frame.mesh(
                Mesh {
                    vertices: &vertices,
                    indices: &[0, 1, 2],
                    texture: None,
                },
                ClipRect::ALL,
            ),
            Err(Error::NonFiniteVertex { index: 3, field })
        );
    }
    assert_eq!(frame.surface().pixels, before);
}
