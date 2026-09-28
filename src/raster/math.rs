// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::Color;
use crate::geometry::Pos2;
use std::cmp::Ordering;

pub(crate) fn usize_to_f32(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

pub(super) fn f32_to_usize_floor_clamped(value: f32, max: usize) -> usize {
    f32_to_usize_threshold_clamped(value.floor(), max)
}

pub(super) fn f32_to_usize_ceil_clamped(value: f32, max: usize) -> usize {
    f32_to_usize_threshold_clamped(value.ceil(), max)
}

pub(super) fn f32_to_usize_round_clamped(value: f32, max: usize) -> usize {
    f32_to_usize_threshold_clamped(value.round(), max)
}

fn f32_to_usize_threshold_clamped(value: f32, max: usize) -> usize {
    // For max <= u16::MAX, usize_to_f32(max) is exactly max, and Rust's saturating float to
    // int cast maps NaN and values <= 0 to 0, so the cast plus `min` equals the branches below
    // for every input, in fewer instructions on the hot sampling and bounds paths.
    if u16::try_from(max).is_ok() {
        return f32_to_usize_saturating(value).min(max);
    }
    if value <= 0.0 {
        return 0;
    }
    let max_value = usize_to_f32(max);
    if value >= max_value {
        return max;
    }
    f32_to_usize_bounded(value.clamp(0.0, max_value))
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "value is clamped to a non-negative finite usize range before casting"
)]
fn f32_to_usize_bounded(value: f32) -> usize {
    value as usize
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "float to int casts saturate, mapping NaN and negatives to 0"
)]
fn f32_to_usize_saturating(value: f32) -> usize {
    value as usize
}

pub(super) fn f32_to_u8_round_clamped(value: f32) -> u8 {
    let value = value.round().clamp(0.0, 255.0);
    f32_to_u8_bounded(value)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "value is rounded and clamped to the u8 range before casting"
)]
fn f32_to_u8_bounded(value: f32) -> u8 {
    value as u8
}

pub(super) fn edge(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    (c.x - a.x).mul_add(b.y - a.y, -((c.y - a.y) * (b.x - a.x)))
}

pub(super) fn edge_step_x(a: Pos2, b: Pos2) -> f32 {
    b.y - a.y
}

pub(super) fn edge_step_y(a: Pos2, b: Pos2) -> f32 {
    -(b.x - a.x)
}

fn edge_is_top_left(a: Pos2, b: Pos2) -> bool {
    a.y < b.y || (same_f32(a.y, b.y) && a.x > b.x)
}

pub(super) fn edge_includes_boundary(a: Pos2, b: Pos2, area: f32) -> bool {
    if area < 0.0 {
        edge_is_top_left(a, b)
    } else {
        !edge_is_top_left(a, b)
    }
}

pub(super) fn edge_covers_pixel(weight: f32, includes_boundary: bool) -> bool {
    weight > 0.0 || (same_f32(weight, 0.0) && includes_boundary)
}

pub(super) fn same_f32(left: f32, right: f32) -> bool {
    matches!(left.partial_cmp(&right), Some(Ordering::Equal))
}

pub(super) fn same_pos2(left: Pos2, right: Pos2) -> bool {
    same_f32(left.x, right.x) && same_f32(left.y, right.y)
}

pub(super) fn near_finite_pos2(left: Pos2, right: Pos2, epsilon: f32) -> bool {
    left.x.is_finite()
        && left.y.is_finite()
        && right.x.is_finite()
        && right.y.is_finite()
        && (left.x - right.x).abs() <= epsilon
        && (left.y - right.y).abs() <= epsilon
}

pub(super) fn color_to_array(color: Color) -> [u8; 4] {
    [color.r, color.g, color.b, color.a]
}

pub(super) fn interpolate_color(
    c0: Color,
    c1: Color,
    c2: Color,
    w0: f32,
    w1: f32,
    w2: f32,
) -> [u8; 4] {
    [
        interpolate_channel(c0.r, c1.r, c2.r, w0, w1, w2),
        interpolate_channel(c0.g, c1.g, c2.g, w0, w1, w2),
        interpolate_channel(c0.b, c1.b, c2.b, w0, w1, w2),
        interpolate_channel(c0.a, c1.a, c2.a, w0, w1, w2),
    ]
}

fn interpolate_channel(c0: u8, c1: u8, c2: u8, w0: f32, w1: f32, w2: f32) -> u8 {
    f32_to_u8_round_clamped(interpolate_channel_value(c0, c1, c2, w0, w1, w2))
}

pub(super) fn interpolate_channel_value(c0: u8, c1: u8, c2: u8, w0: f32, w1: f32, w2: f32) -> f32 {
    f32::from(c0).mul_add(w0, f32::from(c1).mul_add(w1, f32::from(c2) * w2))
}

pub(super) fn modulate_color(vertex: [u8; 4], texture: [u8; 4]) -> [u8; 4] {
    [
        multiply_u8(vertex[0], texture[0]),
        multiply_u8(vertex[1], texture[1]),
        multiply_u8(vertex[2], texture[2]),
        multiply_u8(vertex[3], texture[3]),
    ]
}

fn multiply_u8(a: u8, b: u8) -> u8 {
    u8::try_from((u16::from(a) * u16::from(b) + 127) / 255).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn threshold_clamped_reference(value: f32, max: usize) -> usize {
        if value <= 0.0 {
            return 0;
        }
        let max_value = usize_to_f32(max);
        if value >= max_value {
            return max;
        }
        f32_to_usize_bounded(value.clamp(0.0, max_value))
    }

    #[test]
    fn threshold_clamp_fast_path_matches_reference() {
        let mut values = vec![
            f32::NAN,
            -f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            0.0,
            -0.0,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            f32::MAX,
            f32::MIN,
        ];
        for whole in 0..=u16::MAX {
            let whole = f32::from(whole);
            values.extend([whole, whole + 0.25, whole + 0.5, whole - 0.5, -whole]);
        }
        values.extend([65_536.0, 65_536.5, 1.0e6]);
        for max in [0, 1, 2, 63, 255, 1023, 1279, 65_534, 65_535] {
            for &value in &values {
                for value in [value, value.round(), value.floor(), value.ceil()] {
                    assert_eq!(
                        f32_to_usize_threshold_clamped(value, max),
                        threshold_clamped_reference(value, max),
                        "value={value} max={max}"
                    );
                }
            }
        }
    }

    #[test]
    fn usize_conversions_clamp_floor_ceil_and_round() {
        assert_eq!(f32_to_usize_floor_clamped(-1.25, 10), 0);
        assert_eq!(f32_to_usize_floor_clamped(1.75, 10), 1);
        assert_eq!(f32_to_usize_floor_clamped(12.0, 10), 10);

        assert_eq!(f32_to_usize_ceil_clamped(-1.25, 10), 0);
        assert_eq!(f32_to_usize_ceil_clamped(1.25, 10), 2);
        assert_eq!(f32_to_usize_ceil_clamped(12.0, 10), 10);

        assert_eq!(f32_to_usize_round_clamped(-1.25, 10), 0);
        assert_eq!(f32_to_usize_round_clamped(1.49, 10), 1);
        assert_eq!(f32_to_usize_round_clamped(1.5, 10), 2);
        assert_eq!(f32_to_usize_round_clamped(12.0, 10), 10);
    }

    #[test]
    fn u8_conversion_rounds_and_clamps() {
        assert_eq!(f32_to_u8_round_clamped(-1.0), 0);
        assert_eq!(f32_to_u8_round_clamped(1.49), 1);
        assert_eq!(f32_to_u8_round_clamped(1.5), 2);
        assert_eq!(f32_to_u8_round_clamped(300.0), 255);
    }
}
