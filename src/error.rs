// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt;

use crate::TextureId;

/// Why a renderer call was rejected. Invalid input fails loudly rather than being clipped
/// or skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The surface size overflows or exceeds [`MAX_SURFACE_PIXELS`](crate::MAX_SURFACE_PIXELS).
    SurfaceSize {
        /// Requested width.
        width: usize,
        /// Requested height.
        height: usize,
    },
    /// A texture dimension is zero or the texture's byte size overflows.
    TextureSize {
        /// Requested width.
        width: usize,
        /// Requested height.
        height: usize,
    },
    /// Pixel data is not exactly `width * height * 4` bytes.
    PixelDataLength {
        /// Bytes the dimensions call for.
        expected: usize,
        /// Bytes supplied.
        actual: usize,
    },
    /// Storing the texture would exceed [`MAX_TEXTURE_BYTES`](crate::MAX_TEXTURE_BYTES).
    TextureBudget {
        /// Total texture bytes the call would need.
        requested: usize,
        /// The budget.
        budget: usize,
    },
    /// The texture handle was never created by this renderer, or has been freed.
    UnknownTexture(TextureId),
    /// A texture update region does not fit inside the texture.
    TextureUpdateOutOfBounds(TextureId),
    /// A rectangle coordinate is NaN or infinite.
    NonFiniteRect,
    /// A mesh's index count is not a multiple of three.
    IndexCount(usize),
    /// A mesh index points past the end of its vertices.
    IndexOutOfRange {
        /// The offending index.
        index: u32,
        /// How many vertices the mesh has.
        vertex_count: usize,
    },
    /// The rasterizer rejected the draw; the message says why.
    Raster(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SurfaceSize { width, height } => {
                write!(
                    f,
                    "surface size {width}x{height} overflows or exceeds the pixel budget"
                )
            }
            Self::TextureSize { width, height } => {
                write!(f, "texture size {width}x{height} is empty or overflows")
            }
            Self::PixelDataLength { expected, actual } => {
                write!(f, "pixel data is {actual} bytes, expected {expected}")
            }
            Self::TextureBudget { requested, budget } => {
                write!(
                    f,
                    "texture storage would need {requested} bytes, over the {budget} byte budget"
                )
            }
            Self::UnknownTexture(texture) => write!(f, "unknown or freed texture {texture}"),
            Self::TextureUpdateOutOfBounds(texture) => {
                write!(f, "update region exceeds the bounds of texture {texture}")
            }
            Self::NonFiniteRect => f.write_str("rectangle has a non-finite coordinate"),
            Self::IndexCount(count) => {
                write!(f, "mesh has {count} indices, not a multiple of three")
            }
            Self::IndexOutOfRange {
                index,
                vertex_count,
            } => write!(
                f,
                "mesh index {index} is out of range for {vertex_count} vertices"
            ),
            Self::Raster(message) => write!(f, "rasterizer error: {message}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Raster(error.to_string())
    }
}
