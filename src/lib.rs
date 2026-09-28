// SPDX-License-Identifier: MIT OR Apache-2.0

//! A CPU rasterizer that turns 2D triangles into RGBA8 bytes, and does it the same way every
//! time.
//!
//! There is no GPU here, no window, no swapchain, and no driver to have opinions about your
//! blend state. You hand `dream-soft-render` rectangles, textured rectangles, and triangle
//! meshes; it writes premultiplied pixels into a buffer you own. What happens to the buffer
//! afterwards (a Linux framebuffer, a PNG, a golden-test hash) is your business.
//!
//! It exists because egui on a handheld with no usable GPU still has to draw something, and
//! the CPU in those handhelds is a small ARM core that notices every wasted instruction. The
//! hot paths recognize what they are drawing (solid rectangles, glyph quads, triangle fans)
//! instead of treating everything as anonymous triangles, and the `AArch64` NEON paths are
//! held byte-for-byte to the portable code by tests.
//!
//! # Drawing a Frame
//!
//! ```
//! use dream_soft_render::{ClipRect, Color, Mesh, Rect, SoftwareRenderer, Vertex};
//!
//! # fn main() -> Result<(), dream_soft_render::Error> {
//! let mut renderer = SoftwareRenderer::default();
//! let checkerboard = renderer.create_texture(2, 2, &[
//!     255, 255, 255, 255, 0, 0, 0, 255,
//!     0, 0, 0, 255, 255, 255, 255, 255,
//! ])?;
//!
//! let mut frame = renderer.begin_frame(64, 48)?;
//! frame.clear(Color::from_rgb(17, 20, 28));
//! frame.fill_rect(
//!     Rect::from_min_size([4.0, 4.0], [24.0, 12.0]),
//!     Color::from_rgba_unmultiplied(80, 160, 255, 192),
//!     ClipRect::ALL,
//! )?;
//! frame.textured_rect(
//!     Rect::from_min_size([32.0, 4.0], [16.0, 16.0]),
//!     Rect::FULL_UV,
//!     checkerboard,
//!     Color::WHITE,
//!     ClipRect::ALL,
//! )?;
//!
//! let warning = Color::from_rgb(220, 40, 40);
//! frame.mesh(
//!     Mesh {
//!         vertices: &[
//!             Vertex::new([8.0, 40.0], [0.0, 0.0], warning),
//!             Vertex::new([24.0, 24.0], [0.0, 0.0], warning),
//!             Vertex::new([40.0, 40.0], [0.0, 0.0], warning),
//!         ],
//!         indices: &[0, 1, 2],
//!         texture: None,
//!     },
//!     ClipRect::ALL,
//! )?;
//!
//! let rgba = &frame.surface().pixels; // 64 * 48 * 4 bytes, top row first
//! # assert_eq!(rgba.len(), 64 * 48 * 4);
//! # Ok(())
//! # }
//! ```
//!
//! Draw calls rasterize immediately, in the order you make them. There is no command list to
//! flush and no deferred state that can disagree with what you asked for.
//!
//! egui applications do not build meshes by hand. They call
//! [`SoftwareRenderer::render_egui`] with a UI closure; see [`egui_adapter`]. Both paths write
//! the same surface, share the same textures, and produce identical pixels for identical
//! geometry.
//!
//! # The Rules
//!
//! Every drawing path follows these, egui frames included. If the code and this list ever
//! disagree, the code is wrong.
//!
//! - **Surface.** Row-major RGBA8, top row first, exactly `width * height * 4` bytes, at most
//!   [`MAX_SURFACE_PIXELS`] pixels. It is an opaque framebuffer: every blended pixel is written
//!   with alpha 255, so the alpha channel does not carry coverage. A new or resized surface
//!   starts transparent black. Otherwise a frame draws over whatever the last one left, until
//!   you call [`Frame::clear`]. Nothing clears behind your back.
//! - **Coordinates.** Surface pixels. Origin at the top-left corner, x to the right, y down.
//!   Pixel `(x, y)` covers `[x, x + 1) x [y, y + 1)` and is sampled at its center,
//!   `(x + 0.5, y + 0.5)`.
//! - **Triangle coverage.** A triangle covers the pixels whose centers it contains. A center
//!   exactly on an edge goes to one side by a fixed tie-break, so two triangles sharing that
//!   edge never both blend the pixel.
//! - **Rectangle coverage.** A rectangle covers the pixels whose centers satisfy
//!   `min < center <= max` on both axes. `max <= min` on either axis covers nothing.
//! - **Clipping.** [`ClipRect`] is integer pixels, half-open, and clamped to the surface.
//! - **Color.** [`Color`] is premultiplied RGBA8, and so are texture pixels. Vertex colors
//!   interpolate linearly across a triangle and multiply the sampled texel per channel as
//!   `round(a * b / 255)`.
//! - **Blending.** Premultiplied source-over, onto an opaque destination:
//!   `dst = src + round(dst * (255 - src_alpha) / 255)`, saturating, alpha written as 255.
//!   Fully transparent source pixels leave the destination alone.
//! - **Sampling.** Nearest texel. UV `(0, 0)` is the texture's top-left and `(1, 1)` its
//!   bottom-right; coordinates clamp to that range, and the texel on each axis is
//!   `round(uv * (size - 1))`. Untextured meshes sample opaque white.
//! - **Determinism.** Output is a function of the draw calls. Same calls, same bytes, on every
//!   platform, with or without NEON.
//!
//! # Limits and Failures
//!
//! A surface holds at most [`MAX_SURFACE_PIXELS`] (1280x720) and one renderer's textures at
//! most [`MAX_TEXTURE_BYTES`] (8 MiB). Both limits exist to catch accidents on small machines,
//! not because the rasterizer falls over past them.
//!
//! Malformed input is an [`Error`]: a mesh index past the end of its vertices, pixel data of
//! the wrong length, a NaN rectangle, a freed texture. The renderer does not clip bad input
//! into something drawable and carry on. Code that feeds it garbage has a bug, and a quietly
//! wrong frame would only hide where.

mod color;
pub mod egui_adapter;
mod error;
mod geometry;
mod raster;
mod render_benchmark;
mod renderer;
mod surface;
mod texture;

pub use color::Color;
pub use error::{Error, VertexField};
pub use geometry::{ClipRect, Rect, Vertex};
pub use renderer::{Frame, Mesh, SoftwareRenderer};
pub use surface::{MAX_SURFACE_PIXELS, SoftwareSurface};
pub use texture::{MAX_TEXTURE_BYTES, TextureId};

// Compiles the README's examples so they cannot drift from the API.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
