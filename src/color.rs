// SPDX-License-Identifier: MIT OR Apache-2.0

/// An RGBA8 color with **premultiplied** alpha.
///
/// Every color the renderer accepts or stores is premultiplied: `r`, `g`, and `b` have already
/// been scaled by `a / 255`. Opaque colors are the same either way; for translucent ones, use
/// [`Color::from_rgba_unmultiplied`] to convert straight-alpha input. A channel above `a` is
/// allowed and blends additively, as in egui.
///
/// It is four bytes with four-byte alignment, so the rasterizer compares and moves a color as
/// one 32-bit word.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C, align(4))]
pub struct Color {
    /// Red, premultiplied.
    pub r: u8,
    /// Green, premultiplied.
    pub g: u8,
    /// Blue, premultiplied.
    pub b: u8,
    /// Alpha: 0 is transparent, 255 is opaque.
    pub a: u8,
}

impl Color {
    /// Fully transparent (all channels 0).
    pub const TRANSPARENT: Self = Self::from_rgba_premultiplied(0, 0, 0, 0);
    /// Opaque black.
    pub const BLACK: Self = Self::from_rgba_premultiplied(0, 0, 0, 255);
    /// Opaque white.
    pub const WHITE: Self = Self::from_rgba_premultiplied(255, 255, 255, 255);

    /// A color from channels that are already premultiplied.
    #[must_use]
    pub const fn from_rgba_premultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// A color from straight (unmultiplied) channels, premultiplying each of `r`, `g`, `b` by
    /// `a` with round-to-nearest: `(channel * a + 127) / 255`.
    #[must_use]
    pub const fn from_rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: premultiply(r, a),
            g: premultiply(g, a),
            b: premultiply(b, a),
            a,
        }
    }

    /// An opaque color.
    #[must_use]
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self::from_rgba_premultiplied(r, g, b, 255)
    }

    /// The channels as `[r, g, b, a]`, the same order as surface and texture bytes.
    #[must_use]
    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Packs the color into a `u32` with a fixed layout that does not depend on host byte
    /// order: bits 0-7 red, 8-15 green, 16-23 blue, 24-31 alpha.
    #[must_use]
    pub const fn to_packed(self) -> u32 {
        (self.r as u32) | (self.g as u32) << 8 | (self.b as u32) << 16 | (self.a as u32) << 24
    }

    /// Unpacks a color from the [`Color::to_packed`] layout. Every `u32` is a valid color.
    #[must_use]
    pub const fn from_packed(packed: u32) -> Self {
        let [r, g, b, a] = packed.to_le_bytes();
        Self { r, g, b, a }
    }

    #[cfg(all(test, feature = "egui"))]
    pub(crate) const fn to_egui(self) -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(self.r, self.g, self.b, self.a)
    }
}

// Whole-word comparison: the uniform-color checks that pick every fast path run this per
// vertex, and field-by-field comparison costs four compares where one will do.
impl PartialEq for Color {
    fn eq(&self, other: &Self) -> bool {
        u32::from_ne_bytes(self.to_array()) == u32::from_ne_bytes(other.to_array())
    }
}

impl Eq for Color {}

impl std::hash::Hash for Color {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.to_packed().hash(state);
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "(channel * alpha + 127) / 255 is at most 255"
)]
const fn premultiply(channel: u8, alpha: u8) -> u8 {
    ((channel as u16 * alpha as u16 + 127) / 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_layout_is_red_in_the_low_byte() {
        let color = Color::from_rgba_premultiplied(0x11, 0x22, 0x33, 0x44);
        assert_eq!(color.to_packed(), 0x4433_2211);
        assert_eq!(Color::from_packed(0x4433_2211), color);
    }

    #[test]
    fn unmultiplied_colors_round_to_nearest() {
        assert_eq!(
            Color::from_rgba_unmultiplied(255, 128, 1, 128),
            Color::from_rgba_premultiplied(128, 64, 1, 128)
        );
        assert_eq!(
            Color::from_rgba_unmultiplied(200, 100, 50, 255),
            Color::from_rgb(200, 100, 50)
        );
        assert_eq!(
            Color::from_rgba_unmultiplied(200, 100, 50, 0),
            Color::TRANSPARENT
        );
    }
}
