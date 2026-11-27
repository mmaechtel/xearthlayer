//! The level 0 variants under test and how each is applied to a tile.

use crate::dataset::CHUNK_PX;
use crate::filters::{self, ChunkThresholds, ThresholdMap};
use crate::jpeg_info::JpegInfo;
use image::RgbaImage;
use std::time::{Duration, Instant};

/// How block edges are smoothed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Deblock {
    Off,
    /// The same threshold everywhere, in 8-bit levels.
    Fixed(i32),
    /// The threshold of each chunk is this multiple of the step its own
    /// quantisation table can cause ([`JpegInfo::block_step`]).
    Quant(f32),
}

/// One candidate treatment of level 0.
#[derive(Debug, Clone, Copy)]
pub struct Variant {
    pub name: &'static str,
    pub deblock: Deblock,
    /// Deringing threshold as a multiple of [`JpegInfo::ring_amplitude`];
    /// zero is off.
    pub dering: f32,
    /// Unsharp amount in 256ths; zero is off.
    pub unsharp: i32,
}

/// Fixed deblocking threshold of the C variants.
const FIXED_T: i32 = 8;
/// Multiple of the quantisation step the D and E variants deblock up to.
const QUANT_SCALE: f32 = 1.5;
/// Multiple of the ringing amplitude the E variants smooth up to.
const DERING_SCALE: f32 = 1.5;
/// Luma range a block needs before it is deringed.
const DERING_EDGE_RANGE: i32 = 32;

const fn variant(name: &'static str, deblock: Deblock, dering: f32, unsharp: i32) -> Variant {
    Variant {
        name,
        deblock,
        dering,
        unsharp,
    }
}

/// Unsharp amounts 0.3, 0.6 and 1.0 in 256ths.
const USM: [i32; 3] = [77, 154, 256];

/// Every variant, baseline first.
pub const VARIANTS: [Variant; 13] = [
    variant("A baseline", Deblock::Off, 0.0, 0),
    variant("B usm 0.3", Deblock::Off, 0.0, USM[0]),
    variant("B usm 0.6", Deblock::Off, 0.0, USM[1]),
    variant("B usm 1.0", Deblock::Off, 0.0, USM[2]),
    variant("C deblock", Deblock::Fixed(FIXED_T), 0.0, 0),
    variant("C deblock + usm 0.3", Deblock::Fixed(FIXED_T), 0.0, USM[0]),
    variant("C deblock + usm 0.6", Deblock::Fixed(FIXED_T), 0.0, USM[1]),
    variant("D q-deblock", Deblock::Quant(QUANT_SCALE), 0.0, 0),
    variant(
        "D q-deblock + usm 0.3",
        Deblock::Quant(QUANT_SCALE),
        0.0,
        USM[0],
    ),
    variant(
        "D q-deblock + usm 0.6",
        Deblock::Quant(QUANT_SCALE),
        0.0,
        USM[1],
    ),
    variant(
        "E q-deblock + dering",
        Deblock::Quant(QUANT_SCALE),
        DERING_SCALE,
        0,
    ),
    variant(
        "E q-deblock + dering + usm 0.3",
        Deblock::Quant(QUANT_SCALE),
        DERING_SCALE,
        USM[0],
    ),
    variant(
        "E q-deblock + dering + usm 0.6",
        Deblock::Quant(QUANT_SCALE),
        DERING_SCALE,
        USM[1],
    ),
];

impl Variant {
    /// Thresholds of one chunk. A chunk without a JPEG header (another
    /// format) gets no quantisation-driven filtering.
    fn thresholds(&self, info: Option<&JpegInfo>) -> ChunkThresholds {
        let deblock = match self.deblock {
            Deblock::Off => 0,
            Deblock::Fixed(t) => t,
            Deblock::Quant(scale) => info.map_or(0, |i| (scale * i.block_step()).round() as i32),
        };
        let dering = info.map_or(0, |i| (self.dering * i.ring_amplitude()).round() as i32);
        ChunkThresholds { deblock, dering }
    }

    /// Applies the variant to an assembled tile: deblock, dering, sharpen.
    /// Returns the result and the time the filters took.
    pub fn apply(
        &self,
        source: &RgbaImage,
        chunk_info: &[Option<JpegInfo>],
    ) -> (RgbaImage, Duration) {
        let mut image = source.clone();
        let chunks_wide = (source.width() / CHUNK_PX) as usize;
        let start = Instant::now();
        let map = ThresholdMap::new(
            CHUNK_PX as usize,
            chunks_wide,
            chunk_info
                .iter()
                .map(|i| self.thresholds(i.as_ref()))
                .collect(),
        );
        if self.deblock != Deblock::Off {
            filters::deblock(&mut image, &map);
        }
        if self.dering > 0.0 {
            filters::dering(&mut image, &map, DERING_EDGE_RANGE);
        }
        filters::unsharp(&mut image, self.unsharp);
        (image, start.elapsed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn noisy() -> RgbaImage {
        RgbaImage::from_fn(256, 256, |x, y| {
            let v = ((x * 7 + y * 13) % 61) as u8 + 80;
            Rgba([v, v, 255 - v, 255])
        })
    }

    #[test]
    fn the_baseline_changes_nothing() {
        let source = noisy();
        let (out, _) = VARIANTS[0].apply(&source, &[None]);
        assert_eq!(out, source);
    }

    #[test]
    fn every_variant_is_deterministic_and_keeps_the_size() {
        let source = noisy();
        for v in &VARIANTS {
            let (a, _) = v.apply(&source, &[None]);
            let (b, _) = v.apply(&source, &[None]);
            assert_eq!(a, b, "{}", v.name);
            assert_eq!(a.dimensions(), source.dimensions());
        }
    }

    #[test]
    fn names_are_unique_and_the_baseline_is_first() {
        let mut names: Vec<_> = VARIANTS.iter().map(|v| v.name).collect();
        assert_eq!(names[0], "A baseline");
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), VARIANTS.len());
    }
}
