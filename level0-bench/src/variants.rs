//! The level 0 variants under test and how each is applied to a tile.

use crate::dataset::CHUNK_PX;
use crate::filters::{self, ChunkThresholds, ThresholdMap};
use crate::jpeg_info::JpegInfo;
use image::RgbaImage;
use std::collections::BTreeSet;
use std::sync::OnceLock;
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
    /// Scale the unsharp amount of each chunk by how cleanly it was
    /// compressed ([`Variant::chunk_unsharp`]).
    pub unsharp_by_quant: bool,
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
        unsharp_by_quant: false,
    }
}

/// The F variants: table-driven deblocking, then unsharp masking whose
/// amount follows each chunk's quantisation table.
const fn quant_sharpened(name: &'static str, unsharp: i32) -> Variant {
    Variant {
        name,
        deblock: Deblock::Quant(QUANT_SCALE),
        dering: 0.0,
        unsharp,
        unsharp_by_quant: true,
    }
}

/// libjpeg quality whose table gets exactly the nominal unsharp amount.
const REFERENCE_QUALITY: u8 = 75;
/// Lowest and highest multiple of the nominal amount a chunk can get.
const QUANT_USM_RANGE: (f32, f32) = (0.25, 1.5);

/// The header a libjpeg-style encoder writes at `quality`: the tables of
/// a flat chunk encoded the way the synthetic providers encode.
fn info_at_quality(quality: u8) -> JpegInfo {
    let flat = image::RgbImage::from_pixel(CHUNK_PX, CHUNK_PX, image::Rgb([128, 128, 128]));
    crate::jpeg_info::parse(&crate::synth::encode_chunk(&flat, 0, 0, quality))
        .expect("an encoded chunk has a JPEG header")
}

/// [`JpegInfo::block_step`] of the [`REFERENCE_QUALITY`] table (about 4.84
/// levels), computed once from an encoded chunk rather than written down.
fn reference_block_step() -> f32 {
    static STEP: OnceLock<f32> = OnceLock::new();
    *STEP.get_or_init(|| info_at_quality(REFERENCE_QUALITY).block_step())
}

/// Unsharp amounts 0.3, 0.6 and 1.0 in 256ths.
const USM: [i32; 3] = [77, 154, 256];

/// Unsharp amount 0.1 in 256ths. Added after the first run on real tiles:
/// their ZL16 has 0.92 to 0.97 of the truth's gradient energy and 0.3
/// already overshoots to about 1.1.
const WEAK_USM: i32 = 26;

/// Every variant, baseline first.
pub const VARIANTS: [Variant; 17] = [
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
    quant_sharpened("F q-deblock + q-usm 0.3", USM[0]),
    quant_sharpened("F q-deblock + q-usm 0.6", USM[1]),
    variant("G usm 0.1", Deblock::Off, 0.0, WEAK_USM),
    variant(
        "G q-deblock + usm 0.1",
        Deblock::Quant(QUANT_SCALE),
        0.0,
        WEAK_USM,
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

    /// Unsharp amount of one chunk, in 256ths.
    ///
    /// Without `unsharp_by_quant`, or for a chunk without a JPEG header,
    /// the nominal amount. Otherwise
    ///
    /// ```text
    /// amount * clamp(reference_step / block_step(chunk), 0.25, 1.5)
    /// ```
    ///
    /// rounded to a 256th, where `reference_step` is the block step of
    /// libjpeg quality 75. A chunk at quality 75 gets the nominal amount, a
    /// heavily quantised one down to a quarter of it (sharpening would
    /// amplify its artifacts), a clean one up to one and a half times.
    fn chunk_unsharp(&self, info: Option<&JpegInfo>) -> i32 {
        match info {
            Some(info) if self.unsharp_by_quant => {
                let (lo, hi) = QUANT_USM_RANGE;
                let factor = (reference_block_step() / info.block_step()).clamp(lo, hi);
                (self.unsharp as f32 * factor).round() as i32
            }
            _ => self.unsharp,
        }
    }

    /// Sharpens `image` with every chunk's own amount.
    ///
    /// The whole tile is sharpened once per distinct amount and each chunk
    /// takes its pixels from the result for its amount, so the blur behind
    /// the mask always reads across chunk seams, exactly as it does when
    /// one amount is used everywhere.
    fn sharpen(&self, image: &mut RgbaImage, chunk_info: &[Option<JpegInfo>]) {
        let amounts: Vec<i32> = chunk_info
            .iter()
            .map(|i| self.chunk_unsharp(i.as_ref()))
            .collect();
        let distinct: BTreeSet<i32> = amounts.iter().copied().collect();
        if distinct.len() <= 1 {
            filters::unsharp(image, distinct.first().copied().unwrap_or(self.unsharp));
            return;
        }
        let source = image.clone();
        let chunks_wide = (source.width() / CHUNK_PX) as usize;
        let row_bytes = source.width() as usize * 4;
        let chunk_bytes = CHUNK_PX as usize * 4;
        for amount in distinct {
            let mut sharpened = source.clone();
            filters::unsharp(&mut sharpened, amount);
            for (index, _) in amounts.iter().enumerate().filter(|(_, a)| **a == amount) {
                let x0 = (index % chunks_wide) * chunk_bytes;
                let y0 = (index / chunks_wide) * CHUNK_PX as usize;
                for y in y0..y0 + CHUNK_PX as usize {
                    let at = y * row_bytes + x0;
                    image.as_mut()[at..at + chunk_bytes]
                        .copy_from_slice(&sharpened.as_raw()[at..at + chunk_bytes]);
                }
            }
        }
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
        self.sharpen(&mut image, chunk_info);
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

    /// Two chunks side by side with a pattern that crosses the seam.
    fn two_chunks() -> RgbaImage {
        RgbaImage::from_fn(512, 256, |x, y| {
            let v = (((x / 3 + y / 5) % 2) * 60 + 70 + x % 7) as u8;
            Rgba([v, 255 - v, v / 2, 255])
        })
    }

    fn half(image: &RgbaImage, right: bool) -> RgbaImage {
        image::imageops::crop_imm(image, if right { 256 } else { 0 }, 0, 256, 256).to_image()
    }

    /// The F variant's sharpening stage alone.
    fn q_usm(amount: i32) -> Variant {
        Variant {
            deblock: Deblock::Off,
            ..quant_sharpened("q-usm only", amount)
        }
    }

    #[test]
    fn the_reference_step_is_that_of_quality_75() {
        assert!((reference_block_step() - 4.84).abs() < 0.005);
    }

    #[test]
    fn q_usm_is_plain_usm_when_every_chunk_has_the_reference_table() {
        let source = two_chunks();
        let reference = Some(info_at_quality(REFERENCE_QUALITY));
        let infos = [reference.clone(), reference];
        let (scaled, _) = q_usm(USM[1]).apply(&source, &infos);
        let (plain, _) = variant("usm", Deblock::Off, 0.0, USM[1]).apply(&source, &infos);
        assert_eq!(scaled, plain);
    }

    #[test]
    fn q_usm_gives_a_clean_chunk_more_and_a_coarse_chunk_less() {
        let v = q_usm(USM[1]);
        let at = |quality| v.chunk_unsharp(Some(&info_at_quality(quality)));
        assert_eq!(at(REFERENCE_QUALITY), USM[1]);
        assert!(at(55) < USM[1]);
        assert!(at(90) > USM[1]);
        // The clamp: never below a quarter, never above one and a half.
        assert_eq!(at(5), (USM[1] as f32 * 0.25).round() as i32);
        assert_eq!(at(100), (USM[1] as f32 * 1.5).round() as i32);
        // No header, or a variant without the option: the nominal amount.
        assert_eq!(v.chunk_unsharp(None), USM[1]);
        let plain = variant("usm", Deblock::Off, 0.0, USM[1]);
        assert_eq!(plain.chunk_unsharp(Some(&info_at_quality(55))), USM[1]);
    }

    #[test]
    fn q_usm_sharpens_the_clean_chunk_of_a_tile_more_than_the_coarse_one() {
        let source = two_chunks();
        let infos = [Some(info_at_quality(55)), Some(info_at_quality(90))];
        let (out, _) = q_usm(USM[1]).apply(&source, &infos);
        let change = |right: bool| -> u64 {
            half(&out, right)
                .iter()
                .zip(half(&source, right).iter())
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum()
        };
        // Both halves hold the same pattern up to its 7 pixel phase.
        assert!(
            change(true) > change(false) * 3 / 2,
            "{} {}",
            change(true),
            change(false)
        );
        assert!(change(false) > 0);
    }

    #[test]
    fn q_usm_stays_continuous_across_a_chunk_seam() {
        // A step exactly on the seam between the two chunks.
        let source = RgbaImage::from_fn(512, 256, |x, _| {
            let v = if x < 256 { 60 } else { 180 };
            Rgba([v, v, v, 255])
        });
        let v = q_usm(USM[1]);
        let infos = [Some(info_at_quality(55)), Some(info_at_quality(90))];
        let (out, _) = v.apply(&source, &infos);
        // Each chunk is the whole tile sharpened with that chunk's amount.
        for (right, info) in [(false, &infos[0]), (true, &infos[1])] {
            let mut whole = source.clone();
            filters::unsharp(&mut whole, v.chunk_unsharp(info.as_ref()));
            assert_eq!(half(&out, right), half(&whole, right));
        }
        // So the mask saw the step: both sides overshoot, the clean chunk
        // more. A chunk sharpened on its own would see a flat image.
        let (left, right) = (out.get_pixel(255, 100)[0], out.get_pixel(256, 100)[0]);
        assert!(left < 60 && right > 180, "{left} {right}");
        assert!(right - 180 > 60 - left);
        let mut alone = half(&source, false);
        filters::unsharp(&mut alone, v.chunk_unsharp(infos[0].as_ref()));
        assert_eq!(alone.get_pixel(255, 100)[0], 60);
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
