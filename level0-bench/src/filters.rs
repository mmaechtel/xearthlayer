//! Level 0 candidates: deblocking, deringing and unsharp masking.
//!
//! Every filter works in place on the RGBA bytes of an assembled tile, in
//! stored sRGB, on R, G and B independently; alpha is never touched. The
//! JPEG block grid is the 8 pixel grid from the tile origin, which chunk
//! borders (every 256 pixels) are part of, so the deblocker treats a chunk
//! seam as one more block edge. All arithmetic is integer, so the output is
//! deterministic on every machine.

use image::RgbaImage;

/// Side of a JPEG block in pixels.
const BLOCK: usize = 8;

/// Per-chunk filter thresholds, in 8-bit levels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChunkThresholds {
    /// A step across a block edge smaller than this is treated as a
    /// compression artifact and smoothed; a larger one is image content.
    pub deblock: i32,
    /// Neighbours within this distance of a pixel are averaged with it by
    /// the deringing filter. Zero disables deringing in the chunk.
    pub dering: i32,
}

/// Thresholds for every chunk of a tile, row-major.
#[derive(Debug, Clone)]
pub struct ThresholdMap {
    chunk_px: usize,
    chunks_wide: usize,
    chunks: Vec<ChunkThresholds>,
}

impl ThresholdMap {
    pub fn new(chunk_px: usize, chunks_wide: usize, chunks: Vec<ChunkThresholds>) -> Self {
        assert!(chunks_wide > 0 && chunks.len().is_multiple_of(chunks_wide));
        Self {
            chunk_px,
            chunks_wide,
            chunks,
        }
    }

    /// The same thresholds everywhere.
    #[cfg(test)]
    pub fn uniform(image: &RgbaImage, chunk_px: usize, thresholds: ChunkThresholds) -> Self {
        let wide = (image.width() as usize).div_ceil(chunk_px);
        let high = (image.height() as usize).div_ceil(chunk_px);
        Self::new(chunk_px, wide, vec![thresholds; wide * high])
    }

    fn at(&self, x: usize, y: usize) -> ChunkThresholds {
        self.chunks[(y / self.chunk_px) * self.chunks_wide + x / self.chunk_px]
    }
}

/// Smooths block-edge steps that are below the chunk's `deblock` threshold.
///
/// Where both sides of the edge are flat the step is replaced by a ramp over
/// three pixels on each side, which is what removes visible blocking in
/// fields and water. Elsewhere only the two edge pixels move, by at most
/// half the threshold (the normal-mode filter of H.264).
pub fn deblock(image: &mut RgbaImage, map: &ThresholdMap) {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let data: &mut [u8] = image;
    // Vertical edges: the line of pixels runs along x.
    for y in 0..h {
        for x in (BLOCK..w).step_by(BLOCK) {
            let t = map.at(x - 1, y).deblock.max(map.at(x, y).deblock);
            if t > 0 {
                filter_edge(data, (y * w + x) * 4, 4, t);
            }
        }
    }
    // Horizontal edges: the line of pixels runs along y.
    for y in (BLOCK..h).step_by(BLOCK) {
        for x in 0..w {
            let t = map.at(x, y - 1).deblock.max(map.at(x, y).deblock);
            if t > 0 {
                filter_edge(data, (y * w + x) * 4, w * 4, t);
            }
        }
    }
}

/// Filters one line across a block edge. `q0` is the byte offset of the
/// first pixel after the edge and `stride` the byte distance between
/// neighbouring pixels of the line; four pixels exist on each side.
fn filter_edge(data: &mut [u8], q0: usize, stride: usize, t: i32) {
    for c in 0..3 {
        let at = |k: isize| -> usize { (q0 as isize + k * stride as isize) as usize + c };
        let px = |k: isize| -> i32 { i32::from(data[at(k)]) };
        let (p2, p1, p0) = (px(-3), px(-2), px(-1));
        let (q0v, q1, q2) = (px(0), px(1), px(2));
        let d = q0v - p0;
        if d == 0 || d.abs() >= t {
            continue;
        }
        let flat = (t / 4).max(1);
        if (p2 - p0).abs() <= flat && (q2 - q0v).abs() <= flat {
            // A ramp in sevenths of the step: p2, p1, p0 | q0, q1, q2.
            let step = |n: i32| (d * n + 3 * d.signum()) / 7;
            let mut set = |k: isize, v: i32| data[at(k)] = v.clamp(0, 255) as u8;
            set(-3, p2 + step(1));
            set(-2, p1 + step(2));
            set(-1, p0 + step(3));
            set(0, q0v - step(3));
            set(1, q1 - step(2));
            set(2, q2 - step(1));
        } else if (p1 - p0).abs() < t && (q1 - q0v).abs() < t {
            let delta = ((4 * d + (p1 - q1)) / 8).clamp(-t / 2, t / 2);
            data[at(-1)] = (p0 + delta).clamp(0, 255) as u8;
            data[at(0)] = (q0v - delta).clamp(0, 255) as u8;
        }
    }
}

/// Smooths ringing in blocks that contain a strong edge.
///
/// A block qualifies when its luma range is at least `edge_range`. Inside
/// it every pixel becomes the mean of itself and those 3x3 neighbours
/// within the chunk's `dering` threshold of it (a sigma filter), so the
/// small oscillations beside an edge flatten while the edge itself, whose
/// two sides differ by more than the threshold, stays.
pub fn dering(image: &mut RgbaImage, map: &ThresholdMap, edge_range: i32) {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let src = image.as_raw().clone();
    let data: &mut [u8] = image;
    let luma = |i: usize| -> i32 {
        (77 * i32::from(src[i]) + 150 * i32::from(src[i + 1]) + 29 * i32::from(src[i + 2])) >> 8
    };
    for by in (0..h).step_by(BLOCK) {
        for bx in (0..w).step_by(BLOCK) {
            let t = map.at(bx, by).dering;
            if t <= 0 {
                continue;
            }
            let (x1, y1) = ((bx + BLOCK).min(w), (by + BLOCK).min(h));
            let (mut lo, mut hi) = (255, 0);
            for y in by..y1 {
                for x in bx..x1 {
                    let l = luma((y * w + x) * 4);
                    lo = lo.min(l);
                    hi = hi.max(l);
                }
            }
            if hi - lo < edge_range {
                continue;
            }
            for y in by..y1 {
                for x in bx..x1 {
                    for c in 0..3 {
                        let centre = i32::from(src[(y * w + x) * 4 + c]);
                        let (mut sum, mut count) = (0, 0);
                        for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                            for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                                let v = i32::from(src[(ny * w + nx) * 4 + c]);
                                if (v - centre).abs() <= t {
                                    sum += v;
                                    count += 1;
                                }
                            }
                        }
                        data[(y * w + x) * 4 + c] = ((sum + count / 2) / count) as u8;
                    }
                }
            }
        }
    }
}

/// Unsharp mask: adds `amount_256 / 256` of the difference between the
/// image and its 5x5 binomial blur (sigma about 1 pixel).
pub fn unsharp(image: &mut RgbaImage, amount_256: i32) {
    if amount_256 == 0 {
        return;
    }
    let (w, h) = (image.width() as usize, image.height() as usize);
    let src = image.as_raw().clone();
    const K: [i32; 5] = [1, 4, 6, 4, 1];
    // Horizontal pass, kept at 16x scale.
    let mut rows = vec![0i32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                let mut acc = 0;
                for (i, k) in K.iter().enumerate() {
                    let sx = (x + i).saturating_sub(2).min(w - 1);
                    acc += k * i32::from(src[(y * w + sx) * 4 + c]);
                }
                rows[(y * w + x) * 3 + c] = acc;
            }
        }
    }
    let data: &mut [u8] = image;
    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                let mut acc = 0;
                for (i, k) in K.iter().enumerate() {
                    let sy = (y + i).saturating_sub(2).min(h - 1);
                    acc += k * rows[(sy * w + x) * 3 + c];
                }
                // `acc` is the blur at 256x scale.
                let i = (y * w + x) * 4 + c;
                let v256 = i32::from(src[i]) * 256;
                let out = v256 + (amount_256 * (v256 - acc)) / 256;
                data[i] = ((out + 128) >> 8).clamp(0, 255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn two_halves(left: u8, right: u8) -> RgbaImage {
        RgbaImage::from_fn(16, 16, |x, _| {
            let v = if x < 8 { left } else { right };
            Rgba([v, v, v, 255])
        })
    }

    fn uniform(image: &RgbaImage, deblock: i32, dering: i32) -> ThresholdMap {
        ThresholdMap::uniform(image, 256, ChunkThresholds { deblock, dering })
    }

    #[test]
    fn deblock_turns_a_small_step_between_flat_blocks_into_a_ramp() {
        let mut image = two_halves(100, 107);
        let map = uniform(&image, 12, 0);
        deblock(&mut image, &map);
        let row: Vec<u8> = (3..13).map(|x| image.get_pixel(x, 5)[0]).collect();
        assert_eq!(row, vec![100, 100, 101, 102, 103, 104, 105, 106, 107, 107]);
    }

    #[test]
    fn deblock_leaves_a_step_at_or_above_the_threshold() {
        let original = two_halves(100, 140);
        let mut image = original.clone();
        let map = uniform(&image, 12, 0);
        deblock(&mut image, &map);
        assert_eq!(image, original);
    }

    #[test]
    fn deblock_leaves_steps_that_are_not_on_the_block_grid() {
        let original = RgbaImage::from_fn(16, 16, |x, _| {
            let v = if x < 5 { 100 } else { 106 };
            Rgba([v, v, v, 255])
        });
        let mut image = original.clone();
        let map = uniform(&image, 12, 0);
        deblock(&mut image, &map);
        assert_eq!(image, original);
    }

    #[test]
    fn deblock_uses_the_stronger_chunk_at_a_seam() {
        let mut image = two_halves(100, 105);
        let quiet = ChunkThresholds {
            deblock: 0,
            dering: 0,
        };
        let strong = ChunkThresholds {
            deblock: 12,
            dering: 0,
        };
        let map = ThresholdMap::new(8, 2, vec![quiet, strong, quiet, strong]);
        deblock(&mut image, &map);
        assert_ne!(image.get_pixel(7, 3)[0], 100);
    }

    #[test]
    fn dering_flattens_ripple_beside_an_edge_and_keeps_the_edge() {
        let mut image = RgbaImage::from_fn(8, 8, |x, y| {
            let v = if x < 4 {
                40 + ((x + y) % 2) as u8 * 4
            } else {
                200
            };
            Rgba([v, v, v, 255])
        });
        let map = uniform(&image, 0, 6);
        dering(&mut image, &map, 24);
        assert_eq!(image.get_pixel(1, 3)[0], 42);
        assert_eq!(image.get_pixel(3, 3)[0], 42);
        assert_eq!(image.get_pixel(4, 3)[0], 200);
    }

    #[test]
    fn dering_skips_blocks_without_a_strong_edge() {
        let original = RgbaImage::from_fn(8, 8, |x, y| {
            let v = 40 + ((x + y) % 2) as u8 * 4;
            Rgba([v, v, v, 255])
        });
        let mut image = original.clone();
        let map = uniform(&image, 0, 6);
        dering(&mut image, &map, 24);
        assert_eq!(image, original);
    }

    #[test]
    fn unsharp_leaves_a_flat_image_and_alpha_alone() {
        let original = RgbaImage::from_pixel(12, 12, Rgba([90, 120, 30, 77]));
        let mut image = original.clone();
        unsharp(&mut image, 256);
        assert_eq!(image, original);
    }

    #[test]
    fn unsharp_overshoots_on_both_sides_of_an_edge() {
        let mut image = two_halves(100, 140);
        unsharp(&mut image, 154);
        assert!(image.get_pixel(7, 4)[0] < 100);
        assert!(image.get_pixel(8, 4)[0] > 140);
        assert_eq!(image.get_pixel(2, 4)[0], 100);
    }
}
