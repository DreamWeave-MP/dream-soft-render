// SPDX-License-Identifier: MIT OR Apache-2.0

//! How egui's texture uploads land in the texture store, and how egui meshes name textures.

use std::io;

use super::{
    MAX_TEXTURE_BYTES, MAX_TEXTURE_SIDE, TextureId, TextureImage, TextureKey, TextureStore,
};

impl TextureKey {
    /// The texture an egui mesh samples. egui only uploads `Managed` textures, so `User(n)` is
    /// free to mean renderer-created texture `n`; that is how egui UIs show them.
    pub(crate) const fn for_egui_mesh(id: egui::TextureId) -> Self {
        match id {
            egui::TextureId::User(native) => Self::Native(TextureId(native)),
            egui::TextureId::Managed(_) => Self::Egui(id),
        }
    }
}

impl TextureId {
    /// The egui id that makes an egui mesh sample this texture; see
    /// [`TextureKey::for_egui_mesh`].
    pub(crate) const fn to_egui(self) -> egui::TextureId {
        egui::TextureId::User(self.0)
    }
}

impl TextureStore {
    pub(crate) fn apply(&mut self, delta: &egui::TexturesDelta) -> io::Result<TextureDeltaStats> {
        let mut stats = TextureDeltaStats::default();
        for (id, image_delta) in &delta.set {
            stats.record_set(self.set(*id, image_delta)?);
        }
        Ok(stats)
    }

    fn set(
        &mut self,
        id: egui::TextureId,
        delta: &egui::epaint::ImageDelta,
    ) -> io::Result<TextureSetStats> {
        let id = TextureKey::Egui(id);
        let metadata = TextureImageMetadata::from_image_data(&delta.image)?;
        if let Some(pos) = delta.pos {
            self.textures
                .get(&id)
                .ok_or_else(|| io::Error::other("partial texture update for missing texture"))?
                .validate_update_bounds(pos, metadata.width, metadata.height)?;
            let image = TextureImage::from_image_data(&delta.image, metadata);
            let texture = self
                .textures
                .get_mut(&id)
                .ok_or_else(|| io::Error::other("partial texture update for missing texture"))?;
            texture.update(pos, &image)?;
            Ok(TextureSetStats {
                byte_len: metadata.byte_len,
                partial: true,
            })
        } else {
            let old_len = self
                .textures
                .get(&id)
                .map_or(0, |texture| texture.pixels.len());
            let bytes_used = check_texture_budget(self.bytes_used, old_len, metadata.byte_len)?;
            if metadata.width > MAX_TEXTURE_SIDE || metadata.height > MAX_TEXTURE_SIDE {
                return Err(io::Error::other(format!(
                    "texture {}x{} is longer than {MAX_TEXTURE_SIDE} texels on a side",
                    metadata.width, metadata.height
                )));
            }
            let image = TextureImage::from_image_data(&delta.image, metadata);
            self.textures.insert(id, image);
            self.bytes_used = bytes_used;
            Ok(TextureSetStats {
                byte_len: metadata.byte_len,
                partial: false,
            })
        }
    }

    /// Frees a texture egui released at the end of a frame.
    pub(crate) fn free_egui(&mut self, id: egui::TextureId) {
        self.free(TextureKey::Egui(id));
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TextureDeltaStats {
    pub(crate) set_count: usize,
    pub(crate) full_upload_count: usize,
    pub(crate) partial_update_count: usize,
    pub(crate) set_bytes: usize,
}

impl TextureDeltaStats {
    fn record_set(&mut self, set: TextureSetStats) {
        self.set_count = self.set_count.saturating_add(1);
        self.set_bytes = self.set_bytes.saturating_add(set.byte_len);
        if set.partial {
            self.partial_update_count = self.partial_update_count.saturating_add(1);
        } else {
            self.full_upload_count = self.full_upload_count.saturating_add(1);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TextureSetStats {
    byte_len: usize,
    partial: bool,
}

#[derive(Debug, Clone, Copy)]
struct TextureImageMetadata {
    width: usize,
    height: usize,
    byte_len: usize,
}

impl TextureImageMetadata {
    fn from_image_data(image: &egui::ImageData) -> io::Result<Self> {
        match image {
            egui::ImageData::Color(color_image) => {
                let [width, height] = color_image.size;
                let pixel_count = width
                    .checked_mul(height)
                    .ok_or_else(|| io::Error::other("texture pixel count overflow"))?;
                if color_image.pixels.len() != pixel_count {
                    return Err(io::Error::other(format!(
                        "texture pixel count mismatch: {} != {pixel_count}",
                        color_image.pixels.len()
                    )));
                }
                let byte_len = pixel_count
                    .checked_mul(4)
                    .ok_or_else(|| io::Error::other("texture byte count overflow"))?;
                Ok(Self {
                    width,
                    height,
                    byte_len,
                })
            }
        }
    }
}

impl TextureImage {
    fn from_image_data(image: &egui::ImageData, metadata: TextureImageMetadata) -> Self {
        match image {
            egui::ImageData::Color(color_image) => {
                let mut pixels = Vec::with_capacity(metadata.byte_len);
                for color in &color_image.pixels {
                    pixels.extend_from_slice(&[color.r(), color.g(), color.b(), color.a()]);
                }
                Self {
                    width: metadata.width,
                    height: metadata.height,
                    pixels,
                }
            }
        }
    }

    fn update(&mut self, pos: [usize; 2], image: &Self) -> io::Result<()> {
        self.validate_update_bounds(pos, image.width, image.height)?;
        for row in 0..image.height {
            let destination = ((pos[1] + row) * self.width + pos[0]) * 4;
            let source = row * image.width * 4;
            let byte_count = image.width * 4;
            self.pixels[destination..destination + byte_count]
                .copy_from_slice(&image.pixels[source..source + byte_count]);
        }
        Ok(())
    }
}

fn check_texture_budget(bytes_used: usize, old_len: usize, new_len: usize) -> io::Result<usize> {
    let used = bytes_used
        .checked_sub(old_len)
        .and_then(|bytes| bytes.checked_add(new_len))
        .ok_or_else(|| io::Error::other("texture byte budget overflow"))?;
    if used > MAX_TEXTURE_BYTES {
        return Err(io::Error::other(format!(
            "texture byte budget exceeded: {used} > {MAX_TEXTURE_BYTES}"
        )));
    }
    Ok(used)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn texture_partial_update_checks_bounds() {
        let mut texture = TextureImage {
            width: 2,
            height: 2,
            pixels: vec![0; 16],
        };
        let patch = TextureImage {
            width: 2,
            height: 1,
            pixels: vec![255; 8],
        };

        let error = texture
            .update([1, 1], &patch)
            .expect_err("patch crosses bounds");

        assert_eq!(
            error.to_string(),
            "partial texture update exceeds texture bounds"
        );
    }

    #[test]
    fn texture_budget_rejects_excess_bytes() {
        let error = check_texture_budget(MAX_TEXTURE_BYTES, 0, 4).expect_err("over budget");

        assert_eq!(
            error.to_string(),
            format!(
                "texture byte budget exceeded: {} > {MAX_TEXTURE_BYTES}",
                MAX_TEXTURE_BYTES + 4
            )
        );
    }

    #[test]
    fn texture_preflight_rejects_mismatched_pixel_count() {
        let image = egui::ImageData::Color(Arc::new(egui::ColorImage {
            size: [2, 2],
            source_size: egui::vec2(2.0, 2.0),
            pixels: vec![egui::Color32::WHITE; 3],
        }));

        let error = TextureImageMetadata::from_image_data(&image).expect_err("mismatched pixels");

        assert_eq!(error.to_string(), "texture pixel count mismatch: 3 != 4");
    }

    #[test]
    fn texture_store_rejects_over_budget_before_storing() {
        let mut store = TextureStore::default();
        let small_image = egui::ColorImage::new([1, 1], vec![egui::Color32::WHITE]);
        let small_delta =
            egui::epaint::ImageDelta::full(small_image, egui::TextureOptions::NEAREST);
        store
            .set(egui::TextureId::Managed(0), &small_delta)
            .expect("small texture fits budget");
        let pixel_count = (MAX_TEXTURE_BYTES / 4) + 1;
        let image =
            egui::ColorImage::new([pixel_count, 1], vec![egui::Color32::WHITE; pixel_count]);
        let delta = egui::epaint::ImageDelta::full(image, egui::TextureOptions::NEAREST);

        let error = store
            .set(egui::TextureId::Managed(1), &delta)
            .expect_err("over-budget texture");

        assert_eq!(
            error.to_string(),
            format!(
                "texture byte budget exceeded: {} > {MAX_TEXTURE_BYTES}",
                MAX_TEXTURE_BYTES + 8
            )
        );
        assert_eq!(store.len(), 1);
        assert_eq!(store.bytes_used(), 4);
    }

    // Texel indices are computed in f32 from the texture's size clamped to u16::MAX. egui only
    // checks a texture against max_texture_side in debug builds.
    #[test]
    fn texture_store_rejects_sides_longer_than_65536_texels() {
        let mut store = TextureStore::default();
        for (index, (size, accepted)) in [
            ([65_536, 1], true),
            ([65_537, 1], false),
            ([1, 65_537], false),
        ]
        .into_iter()
        .enumerate()
        {
            let image = egui::ColorImage::new(size, vec![egui::Color32::WHITE; size[0] * size[1]]);
            let delta = egui::epaint::ImageDelta::full(image, egui::TextureOptions::NEAREST);
            let result = store.set(egui::TextureId::Managed(index as u64), &delta);
            if accepted {
                result.expect("65536 texels fit");
            } else {
                assert_eq!(
                    result.expect_err("too long").to_string(),
                    format!(
                        "texture {}x{} is longer than 65536 texels on a side",
                        size[0], size[1]
                    )
                );
            }
        }
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn texture_store_tracks_bytes_for_set_replace_partial_update_and_free() {
        let mut store = TextureStore::default();
        let id = egui::TextureId::Managed(0);
        let full_image = egui::ColorImage::new([2, 2], vec![egui::Color32::WHITE; 4]);
        let full_delta = egui::epaint::ImageDelta::full(full_image, egui::TextureOptions::NEAREST);
        store.set(id, &full_delta).expect("full texture set");

        assert_eq!(store.len(), 1);
        assert_eq!(store.bytes_used(), 16);

        let replacement_image = egui::ColorImage::new([3, 1], vec![egui::Color32::BLACK; 3]);
        let replacement_delta =
            egui::epaint::ImageDelta::full(replacement_image, egui::TextureOptions::NEAREST);
        store
            .set(id, &replacement_delta)
            .expect("replacement texture set");

        assert_eq!(store.len(), 1);
        assert_eq!(store.bytes_used(), 12);

        let partial_image = egui::ColorImage::new([1, 1], vec![egui::Color32::WHITE]);
        let partial_delta =
            egui::epaint::ImageDelta::partial([1, 0], partial_image, egui::TextureOptions::NEAREST);
        store
            .set(id, &partial_delta)
            .expect("partial texture update");

        assert_eq!(store.len(), 1);
        assert_eq!(store.bytes_used(), 12);

        store.free_egui(id);

        assert_eq!(store.len(), 0);
        assert_eq!(store.bytes_used(), 0);
    }

    #[test]
    fn texture_store_apply_reports_full_and_partial_update_stats() {
        let mut store = TextureStore::default();
        let id = egui::TextureId::Managed(0);
        let full_image = egui::ColorImage::new([2, 2], vec![egui::Color32::WHITE; 4]);
        let partial_image = egui::ColorImage::new([1, 1], vec![egui::Color32::BLACK]);
        let delta = egui::TexturesDelta {
            set: vec![
                (
                    id,
                    egui::epaint::ImageDelta::full(full_image, egui::TextureOptions::NEAREST),
                ),
                (
                    id,
                    egui::epaint::ImageDelta::partial(
                        [1, 1],
                        partial_image,
                        egui::TextureOptions::NEAREST,
                    ),
                ),
            ],
            free: Vec::new(),
        };

        let stats = store.apply(&delta).expect("texture delta applies");

        assert_eq!(stats.set_count, 2);
        assert_eq!(stats.full_upload_count, 1);
        assert_eq!(stats.partial_update_count, 1);
        assert_eq!(stats.set_bytes, 20);
    }
}
