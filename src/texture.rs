// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::Error;

#[cfg(feature = "egui")]
mod egui_textures;

#[cfg(feature = "egui")]
pub(crate) use egui_textures::TextureDeltaStats;

/// Total bytes of texture pixels one renderer may hold (8 MiB): egui's font atlas and
/// textures together with those made by [`SoftwareRenderer::create_texture`](crate::SoftwareRenderer::create_texture).
pub const MAX_TEXTURE_BYTES: usize = 8 * 1024 * 1024;

/// A texture owned by a [`SoftwareRenderer`](crate::SoftwareRenderer), returned by
/// [`create_texture`](crate::SoftwareRenderer::create_texture).
///
/// Handles are unique across the whole process and never reused. A freed handle stays invalid,
/// and a handle from one renderer is [`Error::UnknownTexture`] to every other renderer instead
/// of quietly naming whatever texture that renderer happens to hold under the same number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(pub(crate) u64);

// One counter for every renderer in the process, so handles cannot alias across renderers.
static NEXT_TEXTURE_ID: AtomicU64 = AtomicU64::new(0);

/// How the store names a texture: one egui uploaded, or one made through
/// [`SoftwareRenderer::create_texture`](crate::SoftwareRenderer::create_texture).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TextureKey {
    #[cfg(feature = "egui")]
    Egui(egui::TextureId),
    Native(TextureId),
}

impl TextureId {
    /// Issues the next process-wide handle, or `None` once all `u64` values are spent. It never
    /// wraps, because a wrapped counter would hand out a handle that is already in use.
    pub(crate) fn issue() -> Option<Self> {
        issue_from(&NEXT_TEXTURE_ID).map(Self)
    }
}

impl fmt::Display for TextureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

fn issue_from(counter: &AtomicU64) -> Option<u64> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .ok()
}

/// Stands in for "no texture": zero-sized textures sample as opaque white.
pub(crate) static EMPTY_TEXTURE: TextureImage = TextureImage {
    width: 0,
    height: 0,
    pixels: Vec::new(),
};

#[derive(Debug, Default)]
pub(crate) struct TextureStore {
    textures: HashMap<TextureKey, TextureImage>,
    bytes_used: usize,
}

impl TextureStore {
    pub(crate) fn insert_native(
        &mut self,
        id: TextureId,
        width: usize,
        height: usize,
        pixels: &[u8],
    ) -> Result<(), Error> {
        if width == 0 || height == 0 {
            return Err(Error::TextureSize { width, height });
        }
        let expected = rgba8_byte_len(width, height).ok_or(Error::TextureSize { width, height })?;
        check_pixel_data_length(expected, pixels)?;
        let requested = self.bytes_used.saturating_add(expected);
        if requested > MAX_TEXTURE_BYTES {
            return Err(Error::TextureBudget {
                requested,
                budget: MAX_TEXTURE_BYTES,
            });
        }
        self.textures.insert(
            TextureKey::Native(id),
            TextureImage {
                width,
                height,
                pixels: pixels.to_vec(),
            },
        );
        self.bytes_used = requested;
        Ok(())
    }

    pub(crate) fn update_native(
        &mut self,
        id: TextureId,
        pos: [usize; 2],
        size: [usize; 2],
        pixels: &[u8],
    ) -> Result<(), Error> {
        let [width, height] = size;
        let expected = rgba8_byte_len(width, height).ok_or(Error::TextureUpdateOutOfBounds(id))?;
        check_pixel_data_length(expected, pixels)?;
        let texture = self
            .textures
            .get_mut(&TextureKey::Native(id))
            .ok_or(Error::UnknownTexture(id))?;
        texture
            .validate_update_bounds(pos, width, height)
            .map_err(|_| Error::TextureUpdateOutOfBounds(id))?;
        let row_bytes = width * 4;
        for (row, source) in pixels
            .chunks_exact(row_bytes.max(1))
            .enumerate()
            .take(height)
        {
            let destination = ((pos[1] + row) * texture.width + pos[0]) * 4;
            texture.pixels[destination..destination + row_bytes].copy_from_slice(source);
        }
        Ok(())
    }

    pub(crate) fn free_native(&mut self, id: TextureId) -> Result<(), Error> {
        if self.textures.contains_key(&TextureKey::Native(id)) {
            self.free(TextureKey::Native(id));
            Ok(())
        } else {
            Err(Error::UnknownTexture(id))
        }
    }

    fn free(&mut self, id: TextureKey) {
        if let Some(texture) = self.textures.remove(&id) {
            self.bytes_used = self
                .bytes_used
                .checked_sub(texture.pixels.len())
                .expect("texture byte accounting underflow");
        }
    }

    pub(crate) fn get(&self, id: TextureKey) -> Option<&TextureImage> {
        self.textures.get(&id)
    }

    #[cfg(feature = "egui")]
    pub(crate) fn len(&self) -> usize {
        self.textures.len()
    }

    #[cfg(feature = "egui")]
    pub(crate) fn bytes_used(&self) -> usize {
        self.bytes_used
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct TextureImage {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) pixels: Vec<u8>,
}

impl TextureImage {
    fn validate_update_bounds(
        &self,
        pos: [usize; 2],
        width: usize,
        height: usize,
    ) -> io::Result<()> {
        let x_end = pos[0]
            .checked_add(width)
            .ok_or_else(|| io::Error::other("partial texture x range overflow"))?;
        let y_end = pos[1]
            .checked_add(height)
            .ok_or_else(|| io::Error::other("partial texture y range overflow"))?;
        if x_end > self.width || y_end > self.height {
            return Err(io::Error::other(
                "partial texture update exceeds texture bounds",
            ));
        }
        Ok(())
    }
}

fn rgba8_byte_len(width: usize, height: usize) -> Option<usize> {
    width.checked_mul(height)?.checked_mul(4)
}

fn check_pixel_data_length(expected: usize, pixels: &[u8]) -> Result<(), Error> {
    if pixels.len() == expected {
        Ok(())
    } else {
        Err(Error::PixelDataLength {
            expected,
            actual: pixels.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_id_issuance_stops_instead_of_wrapping() {
        let counter = AtomicU64::new(u64::MAX - 2);
        assert_eq!(issue_from(&counter), Some(u64::MAX - 2));
        assert_eq!(issue_from(&counter), Some(u64::MAX - 1));
        assert_eq!(issue_from(&counter), None);
        assert_eq!(issue_from(&counter), None);
    }
}
