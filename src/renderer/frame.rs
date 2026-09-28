// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{
    MeshRasterContext, SOLID_FAN_POLYGON_SCRATCH_CAPACITY, SoftwareRenderer,
    rasterize_mesh_contents,
};
use crate::geometry::RasterMesh;
use crate::raster::ClipBounds;
use crate::surface::SoftwareSurface;
use crate::texture::{EMPTY_TEXTURE, TextureId, TextureKey};
use crate::{ClipRect, Color, Error, Rect, Vertex};

/// Triangles to draw with [`Frame::mesh`].
///
/// Every three `indices` name one triangle's vertices. The rasterizer reads the slices where they
/// lie for the duration of the call: nothing is copied, and nothing is kept afterwards.
#[derive(Clone, Copy, Debug)]
pub struct Mesh<'a> {
    /// The vertices.
    pub vertices: &'a [Vertex],
    /// Vertex indices, three per triangle.
    pub indices: &'a [u32],
    /// The texture sampled at each vertex's `uv`, or `None` to use the vertex colors alone.
    pub texture: Option<TextureId>,
}

impl SoftwareRenderer {
    /// Stores a texture from premultiplied RGBA8 `pixels`, `width * height * 4` bytes, top
    /// row first.
    ///
    /// # Errors
    ///
    /// [`Error::TextureSize`] for a zero or overflowing size, [`Error::PixelDataLength`] if
    /// `pixels` is the wrong length, [`Error::TextureBudget`] if the renderer's texture
    /// storage would exceed [`MAX_TEXTURE_BYTES`](crate::MAX_TEXTURE_BYTES), and
    /// [`Error::TextureIdsExhausted`] once the process has issued every possible handle.
    pub fn create_texture(
        &mut self,
        width: usize,
        height: usize,
        pixels: &[u8],
    ) -> Result<TextureId, Error> {
        let id = TextureId::issue().ok_or(Error::TextureIdsExhausted)?;
        self.textures.insert_native(id, width, height, pixels)?;
        self.previous_frame_valid = false;
        Ok(id)
    }

    /// Replaces the `width`x`height` region of `texture` whose top-left texel is (`x`, `y`)
    /// with premultiplied RGBA8 `pixels`, top row first.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownTexture`] for a freed or foreign handle, [`Error::PixelDataLength`] if
    /// `pixels` is the wrong length, and [`Error::TextureUpdateOutOfBounds`] if the region does
    /// not fit inside the texture.
    pub fn update_texture(
        &mut self,
        texture: TextureId,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        pixels: &[u8],
    ) -> Result<(), Error> {
        self.textures
            .update_native(texture, [x, y], [width, height], pixels)?;
        self.previous_frame_valid = false;
        Ok(())
    }

    /// Releases a texture's storage. The handle is invalid afterwards.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownTexture`] if the handle was already freed or never belonged to this
    /// renderer.
    pub fn free_texture(&mut self, texture: TextureId) -> Result<(), Error> {
        self.textures.free_native(texture)?;
        self.previous_frame_valid = false;
        Ok(())
    }

    /// Starts drawing into a `width`x`height` surface.
    ///
    /// Nothing is cleared implicitly: the surface keeps the previous frame's pixels, unless its
    /// size changed, in which case it starts transparent black. Call [`Frame::clear`] to set a
    /// background.
    ///
    /// # Errors
    ///
    /// [`Error::SurfaceSize`] if the pixel count overflows or exceeds
    /// [`MAX_SURFACE_PIXELS`](crate::MAX_SURFACE_PIXELS).
    pub fn begin_frame(&mut self, width: usize, height: usize) -> Result<Frame<'_>, Error> {
        let resized = self
            .surface
            .resize(width, height)
            .map_err(|_| Error::SurfaceSize { width, height })?;
        if resized {
            self.surface.clear(Color::TRANSPARENT.to_array());
        }
        // Drawing outside egui invalidates the unchanged-egui-frame shortcut.
        self.previous_frame_valid = false;
        Ok(Frame { renderer: self })
    }

    fn rasterize_frame_mesh(
        &mut self,
        mesh: RasterMesh<'_>,
        texture: Option<TextureId>,
        clip: ClipBounds,
    ) -> Result<(), Error> {
        let texture = match texture {
            None => &EMPTY_TEXTURE,
            Some(id) => self
                .textures
                .get(&TextureKey::Native(id))
                .ok_or(Error::UnknownTexture(id))?,
        };
        if clip.is_empty() {
            return Ok(());
        }
        self.solid_fan_polygon_scratch.clear();
        self.solid_fan_seen_boundary_scratch.clear();
        let mut context = MeshRasterContext {
            surface: &mut self.surface,
            fan_polygon_scratch: &mut self.solid_fan_polygon_scratch,
            fan_seen_boundary_scratch: &mut self.solid_fan_seen_boundary_scratch,
            fan_span_cache: &mut self.solid_fan_span_cache,
            mesh,
            texture,
            clip,
            primitive_index: 0,
            solid_fan_polygon_scratch_budget: SOLID_FAN_POLYGON_SCRATCH_CAPACITY,
        };
        rasterize_mesh_contents(&mut context, &mut None, &mut None, &mut None)?;
        Ok(())
    }
}

/// One frame of drawing into a [`SoftwareRenderer`]'s surface, from
/// [`SoftwareRenderer::begin_frame`].
///
/// Draw calls rasterize immediately, in call order, each blended over what is already there.
/// See the crate documentation for coordinates, sampling, and blending.
#[derive(Debug)]
pub struct Frame<'a> {
    renderer: &'a mut SoftwareRenderer,
}

impl Frame<'_> {
    /// Surface width in pixels.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.renderer.surface.width
    }

    /// Surface height in pixels.
    #[must_use]
    pub const fn height(&self) -> usize {
        self.renderer.surface.height
    }

    /// The pixels drawn so far.
    #[must_use]
    pub const fn surface(&self) -> &SoftwareSurface {
        &self.renderer.surface
    }

    /// Overwrites every pixel with `color`, without blending.
    pub fn clear(&mut self, color: Color) {
        self.renderer.surface.clear(color.to_array());
    }

    /// Blends a solid rectangle over the surface.
    ///
    /// A pixel is covered when its center lies in `rect`: `min < center <= max` on each axis.
    /// A rectangle with `max <= min` on either axis covers nothing.
    ///
    /// # Errors
    ///
    /// [`Error::NonFiniteRect`] if a coordinate is NaN or infinite.
    pub fn fill_rect(&mut self, rect: Rect, color: Color, clip: ClipRect) -> Result<(), Error> {
        self.rect(rect, Rect::FULL_UV, None, color, clip)
    }

    /// Blends `texture` over `rect`, mapping `uv.min` to the rectangle's top-left corner and
    /// `uv.max` to its bottom-right, each texel multiplied by the premultiplied `tint`
    /// ([`Color::WHITE`] leaves it unchanged). Coverage is as in [`Frame::fill_rect`].
    ///
    /// # Errors
    ///
    /// [`Error::NonFiniteRect`] for a NaN or infinite coordinate in `rect` or `uv`, and
    /// [`Error::UnknownTexture`] for a freed or foreign texture.
    pub fn textured_rect(
        &mut self,
        rect: Rect,
        uv: Rect,
        texture: TextureId,
        tint: Color,
        clip: ClipRect,
    ) -> Result<(), Error> {
        self.rect(rect, uv, Some(texture), tint, clip)
    }

    // Rectangles are drawn as egui lays them out (`tl, tr, bl, br` as `(0, 1, 2), (2, 1, 3)`),
    // which the rasterizer recognizes as a rectangle before any per-triangle work, so a
    // rectangle drawn here and the same rectangle from egui produce identical pixels.
    fn rect(
        &mut self,
        rect: Rect,
        uv: Rect,
        texture: Option<TextureId>,
        color: Color,
        clip: ClipRect,
    ) -> Result<(), Error> {
        if !rect.is_finite() || !uv.is_finite() {
            return Err(Error::NonFiniteRect);
        }
        if rect.max[0] <= rect.min[0] || rect.max[1] <= rect.min[1] {
            if let Some(texture) = texture {
                self.check_texture(texture)?;
            }
            return Ok(());
        }
        let corner = |x: usize, y: usize| {
            Vertex::new(
                [[rect.min[0], rect.max[0]][x], [rect.min[1], rect.max[1]][y]],
                [[uv.min[0], uv.max[0]][x], [uv.min[1], uv.max[1]][y]],
                color,
            )
        };
        let vertices = [corner(0, 0), corner(1, 0), corner(0, 1), corner(1, 1)];
        self.mesh(
            Mesh {
                vertices: &vertices,
                indices: &[0, 1, 2, 2, 1, 3],
                texture,
            },
            clip,
        )
    }

    /// Blends a triangle mesh over the surface.
    ///
    /// A pixel is covered when its center falls inside a triangle. Centers exactly on an edge
    /// go to one side by a fixed tie-break, so triangles sharing an edge never both blend a
    /// pixel on it. Colors interpolate across each triangle; texels are sampled
    /// nearest-neighbor.
    ///
    /// # Errors
    ///
    /// [`Error::IndexCount`] if the index count is not a multiple of three,
    /// [`Error::IndexOutOfRange`] for an index past the vertices,
    /// [`Error::NonFiniteVertex`] for a NaN or infinite position or texture coordinate on any
    /// vertex, referenced or not, and [`Error::UnknownTexture`] for a freed or foreign texture.
    pub fn mesh(&mut self, mesh: Mesh<'_>, clip: ClipRect) -> Result<(), Error> {
        if !mesh.indices.len().is_multiple_of(3) {
            return Err(Error::IndexCount(mesh.indices.len()));
        }
        let vertex_count = mesh.vertices.len();
        if let Some(&index) = mesh
            .indices
            .iter()
            .find(|&&index| usize::try_from(index).is_ok_and(|index| index >= vertex_count))
        {
            return Err(Error::IndexOutOfRange {
                index,
                vertex_count,
            });
        }
        if let Some((index, field)) = mesh
            .vertices
            .iter()
            .enumerate()
            .find_map(|(index, vertex)| Some((index, vertex.non_finite_field()?)))
        {
            return Err(Error::NonFiniteVertex { index, field });
        }
        if let Some(texture) = mesh.texture {
            self.check_texture(texture)?;
        }
        let renderer = &mut *self.renderer;
        let clip = clip.to_bounds(renderer.surface.width, renderer.surface.height);
        renderer.rasterize_frame_mesh(
            RasterMesh {
                vertices: mesh.vertices,
                indices: mesh.indices,
            },
            mesh.texture,
            clip,
        )
    }

    fn check_texture(&self, texture: TextureId) -> Result<(), Error> {
        if self
            .renderer
            .textures
            .get(&TextureKey::Native(texture))
            .is_some()
        {
            Ok(())
        } else {
            Err(Error::UnknownTexture(texture))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient_vertex(x: f32, y: f32, u: f32, v: f32, alpha: u8) -> Vertex {
        Vertex::new(
            [x, y],
            [u, v],
            Color::from_rgba_unmultiplied(200, 120, 40, alpha),
        )
    }

    #[test]
    fn frame_meshes_match_the_egui_mesh_path() {
        let vertices = [
            gradient_vertex(3.25, 2.5, 0.0, 0.0, 255),
            gradient_vertex(40.0, 4.0, 1.0, 0.0, 180),
            gradient_vertex(6.0, 28.75, 0.0, 1.0, 90),
            gradient_vertex(44.5, 30.0, 1.0, 1.0, 20),
            gradient_vertex(10.0, 10.0, 0.0, 0.0, 255),
            gradient_vertex(30.0, 10.0, 1.0, 0.0, 255),
            gradient_vertex(10.0, 20.0, 0.0, 1.0, 255),
            gradient_vertex(30.0, 20.0, 1.0, 1.0, 255),
        ];
        let indices = [0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7];
        let texels: Vec<u8> = (0..8 * 8)
            .flat_map(|texel: u8| {
                Color::from_rgba_unmultiplied(texel * 4, 255 - texel * 4, 90, texel * 4).to_array()
            })
            .collect();

        for texture in [false, true] {
            let mut generic = SoftwareRenderer::default();
            let mut via_egui = SoftwareRenderer::default();
            let generic_texture = generic.create_texture(8, 8, &texels).expect("texture");
            let egui_texture = via_egui.create_texture(8, 8, &texels).expect("texture");
            let clip = ClipRect::new(2, 1, 40, 29);

            let mut frame = generic.begin_frame(48, 32).expect("frame");
            frame.clear(Color::from_rgb(17, 20, 28));
            frame
                .mesh(
                    Mesh {
                        vertices: &vertices,
                        indices: &indices,
                        texture: texture.then_some(generic_texture),
                    },
                    clip,
                )
                .expect("mesh");

            via_egui
                .begin_frame(48, 32)
                .expect("frame")
                .clear(Color::from_rgb(17, 20, 28));
            let mesh = egui::Mesh {
                indices: indices.to_vec(),
                vertices: vertices.iter().map(|vertex| vertex.to_egui()).collect(),
                texture_id: if texture {
                    egui_texture.to_egui()
                } else {
                    // The egui path samples egui's font atlas white texel for untextured shapes;
                    // an opaque white 1x1 texture stands in for it.
                    let white = via_egui.create_texture(1, 1, &[255; 4]).expect("white");
                    white.to_egui()
                },
            };
            via_egui
                .rasterize(
                    &[egui::ClippedPrimitive {
                        clip_rect: egui::Rect::from_min_max(
                            egui::pos2(2.0, 1.0),
                            egui::pos2(40.0, 29.0),
                        ),
                        primitive: egui::epaint::Primitive::Mesh(mesh),
                    }],
                    None,
                    None,
                    None,
                )
                .expect("egui mesh");

            assert_eq!(
                generic.surface.pixels, via_egui.surface.pixels,
                "textured={texture}"
            );
        }
    }
}
