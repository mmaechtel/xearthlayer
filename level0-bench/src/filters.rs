//! Level 0 candidates: deblocking, deringing and unsharp masking.
//!
//! Every filter works in place on the RGBA bytes of an assembled tile, in
//! stored sRGB, on R, G and B independently; alpha is never touched. The
//! JPEG block grid is the 8 pixel grid from the tile origin, which chunk
//! borders (every 256 pixels) are part of, so the deblocker treats a chunk
//! seam as one more block edge. All arithmetic is integer, so the output is
//! deterministic on every machine.
//!
//! The inner loops are written so that the compiler vectorises them with
//! the baseline instruction set (no `target-cpu` flags, no `unsafe`): the
//! same arithmetic for every lane, outcomes chosen with masks instead of
//! branches, 16-bit lanes. The tests keep the first, straightforward
//! implementations and require the same bytes from these.

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

/// Deblocking thresholds per byte of a line of pixels, in the 16-bit lanes
/// the filter runs in. Alpha lanes keep a zero threshold, which leaves them
/// alone.
struct LaneThresholds {
    step: Vec<i16>,
    flat: Vec<i16>,
    half: Vec<i16>,
}

impl LaneThresholds {
    fn new(pixels: usize) -> Self {
        Self {
            step: vec![0; pixels * 4],
            flat: vec![0; pixels * 4],
            half: vec![0; pixels * 4],
        }
    }

    /// Sets the threshold of one pixel's colour lanes. Values are capped at
    /// 256, above every difference of two bytes, so the cap changes nothing.
    fn set(&mut self, pixel: usize, t: i32) {
        let cap = |v: i32| v.clamp(0, 256) as i16;
        let at = pixel * 4..pixel * 4 + 3;
        self.step[at.clone()].fill(cap(t));
        self.flat[at.clone()].fill(cap((t / 4).max(1)));
        self.half[at].fill(cap(t / 2));
    }

    fn any(&self) -> bool {
        self.step.iter().any(|&t| t > 0)
    }
}

/// Smooths block-edge steps that are below the chunk's `deblock` threshold.
///
/// Where both sides of the edge are flat the step is replaced by a ramp over
/// three pixels on each side, which is what removes visible blocking in
/// fields and water. Elsewhere only the two edge pixels move, by at most
/// half the threshold (the normal-mode filter of H.264).
///
/// The image must have at least three pixels after its last block edge in
/// each direction, which every multiple of 8 does.
pub fn deblock(image: &mut RgbaImage, map: &ThresholdMap) {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let row_bytes = w * 4;
    let data: &mut [u8] = image;

    // Vertical edges: the line of pixels runs along x. The six pixels
    // around every edge of a row are gathered into six lane buffers, one
    // per position in the line, filtered together and written back. The
    // thresholds of a row's edges change only where a row of chunks starts.
    let edges: Vec<usize> = (BLOCK..w).step_by(BLOCK).collect();
    let mut thresholds = LaneThresholds::new(edges.len());
    let n = edges.len() * 4;
    let mut moves = Moves::new(n);
    let mut lines = vec![0u8; n * 6];
    let mut chunk_row = usize::MAX;
    let mut active = false;
    for (y, row) in data.chunks_exact_mut(row_bytes.max(1)).take(h).enumerate() {
        if edges.is_empty() {
            break;
        }
        if y / map.chunk_px != chunk_row {
            chunk_row = y / map.chunk_px;
            for (e, &x) in edges.iter().enumerate() {
                thresholds.set(e, map.at(x - 1, y).deblock.max(map.at(x, y).deblock));
            }
            active = thresholds.any();
        }
        if !active {
            continue;
        }
        for (e, &x) in edges.iter().enumerate() {
            let around: &[u8; 24] = row[(x - 3) * 4..(x + 3) * 4]
                .try_into()
                .expect("six pixels");
            for (k, pixel) in around.chunks_exact(4).enumerate() {
                lines[k * n + e * 4..][..4].copy_from_slice(pixel);
            }
        }
        filter_lines(&mut lines, &thresholds, &mut moves);
        for (e, &x) in edges.iter().enumerate() {
            let around: &mut [u8; 24] = (&mut row[(x - 3) * 4..(x + 3) * 4])
                .try_into()
                .expect("six pixels");
            for (k, pixel) in around.chunks_exact_mut(4).enumerate() {
                pixel.copy_from_slice(&lines[k * n + e * 4..][..4]);
            }
        }
    }

    // Horizontal edges: the line of pixels runs along y, so the six rows
    // around an edge already are the six lanes.
    let mut thresholds = LaneThresholds::new(w);
    let mut moves = Moves::new(row_bytes);
    let mut chunk_rows = (usize::MAX, usize::MAX);
    for y in (BLOCK..h).step_by(BLOCK) {
        let rows = ((y - 1) / map.chunk_px, y / map.chunk_px);
        if rows != chunk_rows {
            chunk_rows = rows;
            for x in 0..w {
                thresholds.set(x, map.at(x, y - 1).deblock.max(map.at(x, y).deblock));
            }
            active = thresholds.any();
        }
        if active {
            let band = &mut data[(y - 3) * row_bytes..(y + 3) * row_bytes];
            filter_lines(band, &thresholds, &mut moves);
        }
    }
}

/// `x / 7` for the small sums the ramp forms (`x` up to `3 * 255 + 3`), in
/// 16-bit arithmetic only: `73 / 512` is just under a seventh, so the first
/// estimate is the quotient or one below it, and the remainder says which.
#[inline(always)]
fn seventh(x: i16) -> i16 {
    // The product needs all 16 bits, so it is formed unsigned.
    let estimate = ((x as u16 * 73) >> 9) as i16;
    estimate + i16::from(x - 7 * estimate >= 7)
}

/// Filters many lines across block edges at once. `lines` is six equal
/// lanes: the values three, two and one pixel before the edge, then one,
/// two and three after it, each lane holding that position of every line.
fn filter_lines(lines: &mut [u8], thresholds: &LaneThresholds, moves: &mut Moves) {
    let n = thresholds.step.len();
    let (p2, rest) = lines.split_at_mut(n);
    let (p1, rest) = rest.split_at_mut(n);
    let (p0, rest) = rest.split_at_mut(n);
    let (q0, rest) = rest.split_at_mut(n);
    let (q1, q2) = rest.split_at_mut(n);
    let q2 = &mut q2[..n];
    plan_moves([p2, p1, p0, q0, q1, q2], thresholds, moves);
    apply_move(p2, q2, &moves.outer);
    apply_move(p1, q1, &moves.middle);
    apply_move(p0, q0, &moves.inner);
}

/// How far each line's values move toward the other side of the edge:
/// `outer` for the pixels three from the edge, `middle` two, `inner` one.
/// Zero where a line is left alone.
struct Moves {
    outer: Vec<i16>,
    middle: Vec<i16>,
    inner: Vec<i16>,
}

impl Moves {
    fn new(lanes: usize) -> Self {
        Self {
            outer: vec![0; lanes],
            middle: vec![0; lanes],
            inner: vec![0; lanes],
        }
    }
}

/// Decides the moves of every line. Every line takes the same arithmetic,
/// with the outcome chosen at the end, so the loop has no data-dependent
/// branch and the compiler can vectorise it.
#[inline(never)]
fn plan_moves(lines: [&[u8]; 6], thresholds: &LaneThresholds, moves: &mut Moves) {
    let n = moves.outer.len();
    let [p2, p1, p0, q0, q1, q2] = lines.map(|lane| &lane[..n]);
    let step = &thresholds.step[..n];
    let flat = &thresholds.flat[..n];
    let half = &thresholds.half[..n];
    let (outer, middle, inner) = (
        &mut moves.outer[..n],
        &mut moves.middle[..n],
        &mut moves.inner[..n],
    );
    // All ones where the condition holds, so that outcomes are chosen with
    // `&` and `|` rather than a branch.
    let mask = |holds: bool| -i16::from(holds);
    for i in 0..n {
        let (a2, a1, a0) = (i16::from(p2[i]), i16::from(p1[i]), i16::from(p0[i]));
        let (b0, b1, b2) = (i16::from(q0[i]), i16::from(q1[i]), i16::from(q2[i]));
        let d = b0 - a0;
        let size = d.abs();
        let small = mask(d != 0) & mask(size < step[i]);
        let strong = small & mask((a2 - a0).abs() <= flat[i]) & mask((b2 - b0).abs() <= flat[i]);
        let normal =
            small & !strong & mask((a1 - a0).abs() < step[i]) & mask((b1 - b0).abs() < step[i]);
        // A ramp in sevenths of the step: p2, p1, p0 | q0, q1, q2. The
        // sevenths carry the sign of the step: `negative` is all ones for
        // a falling step, and `(x ^ ones) - ones` is `-x`.
        let negative = d >> 15;
        let signed = |x: i16| (x ^ negative) - negative;
        let s1 = signed(seventh(size + 3));
        let s2 = signed(seventh(2 * size + 3));
        let s3 = signed(seventh(3 * size + 3));
        // Division by 8 that rounds toward zero, then `min` and `max`
        // rather than `clamp`, whose check of its bounds is a branch.
        let sum = 4 * d + (a1 - b1);
        let delta = ((sum + ((sum >> 15) & 7)) >> 3).max(-half[i]).min(half[i]);
        outer[i] = s1 & strong;
        middle[i] = s2 & strong;
        inner[i] = (s3 & strong) | (delta & normal);
    }
}

/// Moves the values before the edge up and those after it down by `by`.
#[inline(never)]
fn apply_move(before: &mut [u8], after: &mut [u8], by: &[i16]) {
    for ((p, q), &by) in before.iter_mut().zip(after.iter_mut()).zip(by) {
        *p = (i16::from(*p) + by).clamp(0, 255) as u8;
        *q = (i16::from(*q) - by).clamp(0, 255) as u8;
    }
}

/// `RECIPROCAL[c]` is `ceil(65536 / c)`, so that `(n * RECIPROCAL[c]) >> 16`
/// is exactly `n / c` for every sum the deringing filter can form (`c` up
/// to 9 neighbours, `n` up to `9 * 255 + 4`).
const RECIPROCAL: [u32; 10] = [
    0, 65536, 32768, 21846, 16384, 13108, 10923, 9363, 8192, 7282,
];

/// The mean of `sum` over `count` values, as `(sum + count / 2) / count`.
#[inline(always)]
fn rounded_mean(sum: u32, count: u32) -> u8 {
    (((sum + count / 2) * RECIPROCAL[count as usize]) >> 16) as u8
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
    if w == 0 || h == 0 {
        return;
    }
    let row_bytes = w * 4;
    let blocks = w.div_ceil(BLOCK);
    let data: &mut [u8] = image;
    // The filter reads unfiltered pixels, also across block borders. Only
    // the rows a row of blocks can read are kept: slot `k` of `orig` is the
    // unfiltered image row `by - 1 + k`. It is filled when the first block
    // of the row qualifies, so a tile without strong edges copies nothing.
    let mut orig = vec![0u8; row_bytes * (BLOCK + 2)];
    // The unfiltered last row of the previous row of blocks, when that row
    // was filtered.
    let mut above = vec![0u8; row_bytes];
    let mut above_filtered = false;
    let mut luma = vec![0u8; w];
    let (mut lo, mut hi) = (vec![0u8; blocks], vec![0u8; blocks]);
    for by in (0..h).step_by(BLOCK) {
        let y1 = (by + BLOCK).min(h);
        // Luma range of every block of this row of blocks.
        lo.fill(255);
        hi.fill(0);
        for row in data[by * row_bytes..y1 * row_bytes].chunks_exact(row_bytes) {
            for (l, p) in luma.iter_mut().zip(row.chunks_exact(4)) {
                let sum = 77 * u16::from(p[0]) + 150 * u16::from(p[1]) + 29 * u16::from(p[2]);
                *l = (sum >> 8) as u8;
            }
            let whole = luma.chunks_exact(BLOCK);
            let rest = whole.remainder();
            for ((lo, hi), block) in lo.iter_mut().zip(hi.iter_mut()).zip(whole) {
                let block: &[u8; BLOCK] = block.try_into().expect("a whole block");
                *lo = block.iter().fold(*lo, |m, &l| m.min(l));
                *hi = block.iter().fold(*hi, |m, &l| m.max(l));
            }
            if let (Some(lo), Some(hi)) = (lo.last_mut(), hi.last_mut()) {
                *lo = rest.iter().fold(*lo, |m, &l| m.min(l));
                *hi = rest.iter().fold(*hi, |m, &l| m.max(l));
            }
        }
        let mut saved = false;
        for (block, bx) in (0..w).step_by(BLOCK).enumerate() {
            let t = map.at(bx, by).dering;
            if t <= 0 || i32::from(hi[block]) - i32::from(lo[block]) < edge_range {
                continue;
            }
            let x1 = (bx + BLOCK).min(w);
            if !saved {
                saved = true;
                if by > 0 {
                    let row = if above_filtered {
                        &above[..]
                    } else {
                        &data[(by - 1) * row_bytes..by * row_bytes]
                    };
                    orig[..row_bytes].copy_from_slice(row);
                }
                let last = (y1 + 1).min(h);
                orig[row_bytes..(last - by + 1) * row_bytes]
                    .copy_from_slice(&data[by * row_bytes..last * row_bytes]);
            }
            // Differences of bytes never exceed 255.
            let t = t.min(255) as u8;
            for y in by..y1 {
                // Slots of the rows above, at and below `y`, clamped to the
                // image as the neighbourhood is.
                let slot = |row: usize| (row + 1 - by) * row_bytes;
                let up = &orig[slot(y.saturating_sub(1))..][..row_bytes];
                let mid = &orig[slot(y)..][..row_bytes];
                let down = &orig[slot((y + 1).min(h - 1))..][..row_bytes];
                let out = &mut data[y * row_bytes..(y + 1) * row_bytes];
                if y > 0 && y + 1 < h && bx > 0 && x1 < w && x1 - bx == BLOCK {
                    sigma_block([up, mid, down], out, bx * 4, t);
                    continue;
                }
                let rows: &[&[u8]] = match (y > 0, y + 1 < h) {
                    (true, true) => &[up, mid, down],
                    (true, false) => &[up, mid],
                    (false, true) => &[mid, down],
                    (false, false) => &[mid],
                };
                for x in bx..x1 {
                    let (n0, n1) = (x.saturating_sub(1) * 4, ((x + 1).min(w - 1) + 1) * 4);
                    for c in 0..3 {
                        let centre = mid[x * 4 + c];
                        let (mut sum, mut count) = (0u32, 0u32);
                        for row in rows {
                            for p in row[n0..n1].chunks_exact(4) {
                                let near = u32::from(p[c].abs_diff(centre) <= t);
                                sum += u32::from(p[c]) * near;
                                count += near;
                            }
                        }
                        out[x * 4 + c] = rounded_mean(sum, count);
                    }
                }
            }
        }
        above_filtered = saved;
        if saved && y1 - by == BLOCK {
            above.copy_from_slice(&orig[BLOCK * row_bytes..(BLOCK + 1) * row_bytes]);
        }
    }
}

/// Bytes of one row of a block.
const LANES: usize = BLOCK * 4;

/// The sigma filter for one row of a whole block that starts at byte
/// `from` and whose 3x3 neighbourhoods all lie inside the image. Each of
/// the nine neighbours is one pass over the block's 32 lanes, without a
/// data-dependent branch.
#[inline(always)]
fn sigma_block(rows: [&[u8]; 3], out: &mut [u8], from: usize, t: u8) {
    let lanes = |row: &[u8], at: usize| -> [u8; LANES] {
        row[at..at + LANES].try_into().expect("a whole block")
    };
    let centre = lanes(rows[1], from);
    let (mut sum, mut count) = ([0u16; LANES], [0u16; LANES]);
    for row in rows {
        for at in [from - 4, from, from + 4] {
            let neighbour = lanes(row, at);
            for i in 0..LANES {
                let near = u16::from(neighbour[i].abs_diff(centre[i]) <= t);
                sum[i] += u16::from(neighbour[i]) * near;
                count[i] += near;
            }
        }
    }
    for (pixel, (sum, count)) in out[from..from + LANES]
        .chunks_exact_mut(4)
        .zip(sum.chunks_exact(4).zip(count.chunks_exact(4)))
    {
        for c in 0..3 {
            pixel[c] = rounded_mean(u32::from(sum[c]), u32::from(count[c]));
        }
    }
}

/// The 1-4-6-4-1 blur of one RGBA row along x, at 16 times scale, with the
/// row's ends repeated. Every lane is blurred, alpha included; the caller
/// ignores that one.
fn blur_row(src: &[u8], out: &mut [u16]) {
    let w = src.len() / 4;
    let edge = |x: usize, out: &mut [u16]| {
        for c in 0..4 {
            let at = |i: usize| u16::from(src[(x + i).saturating_sub(2).min(w - 1) * 4 + c]);
            out[x * 4 + c] = at(0) + 4 * at(1) + 6 * at(2) + 4 * at(3) + at(4);
        }
    };
    if w < 5 {
        (0..w).for_each(|x| edge(x, out));
        return;
    }
    let n = (w - 4) * 4;
    let lanes = out[8..8 + n].iter_mut();
    let taps = src[..n]
        .iter()
        .zip(&src[4..4 + n])
        .zip(&src[8..8 + n])
        .zip(&src[12..12 + n])
        .zip(&src[16..16 + n]);
    for (o, ((((&a, &b), &c), &d), &e)) in lanes.zip(taps) {
        *o = u16::from(a) + 4 * u16::from(b) + 6 * u16::from(c) + 4 * u16::from(d) + u16::from(e);
    }
    for x in [0, 1, w - 2, w - 1] {
        edge(x, out);
    }
}

/// Unsharp mask: adds `amount_256 / 256` of the difference between the
/// image and its 5x5 binomial blur (sigma about 1 pixel).
pub fn unsharp(image: &mut RgbaImage, amount_256: i32) {
    let (w, h) = (image.width() as usize, image.height() as usize);
    if amount_256 == 0 || w == 0 || h == 0 {
        return;
    }
    let row_bytes = w * 4;
    let data: &mut [u8] = image;
    // The blur along x of the five rows around the current one, row `r` in
    // slot `r % 5`. A row is blurred before the pass reaches it, so it is
    // still unsharpened when it is read.
    let mut ring = vec![0u16; row_bytes * 5];
    let mut blurred = 0;
    let mut sharp = vec![0u8; row_bytes];
    for y in 0..h {
        while blurred <= (y + 2).min(h - 1) {
            let src = &data[blurred * row_bytes..(blurred + 1) * row_bytes];
            blur_row(src, &mut ring[(blurred % 5) * row_bytes..][..row_bytes]);
            blurred += 1;
        }
        let tap = |i: usize| -> &[u16] {
            let row = (y + i).saturating_sub(2).min(h - 1);
            &ring[(row % 5) * row_bytes..][..row_bytes]
        };
        let (r0, r1, r2, r3, r4) = (tap(0), tap(1), tap(2), tap(3), tap(4));
        let row = &mut data[y * row_bytes..(y + 1) * row_bytes];
        for i in 0..row_bytes {
            // `acc` is the blur at 256x scale.
            let acc = i32::from(r0[i])
                + 4 * i32::from(r1[i])
                + 6 * i32::from(r2[i])
                + 4 * i32::from(r3[i])
                + i32::from(r4[i]);
            let v256 = i32::from(row[i]) * 256;
            let out = v256 + (amount_256 * (v256 - acc)) / 256;
            sharp[i] = ((out + 128) >> 8).clamp(0, 255) as u8;
        }
        for (pixel, new) in row.chunks_exact_mut(4).zip(sharp.chunks_exact(4)) {
            pixel[..3].copy_from_slice(&new[..3]);
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

    fn hash(x: u32, y: u32, seed: u32) -> u32 {
        let mut h = x
            .wrapping_mul(0x9E37_79B1)
            .wrapping_add(y.wrapping_mul(0x85EB_CA77))
            .wrapping_add(seed.wrapping_mul(0xC2B2_AE3D));
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        h ^= h >> 12;
        h = h.wrapping_mul(0x297A_2D39);
        h ^ (h >> 15)
    }

    /// A deterministic image with every regime the filters distinguish:
    /// smooth ramps, small steps on the block grid, strong edges, and
    /// quiet to heavy noise in different quarters. Alpha varies too.
    fn scene(w: u32, h: u32, seed: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| {
            let quarter = (x * 2 / w.max(1) + 2 * (y * 2 / h.max(1))) as usize;
            let amplitude = [0, 2, 6, 40][quarter % 4];
            let base = 90 + ((x / 7 + y / 11) % 60) as i32;
            let block = (hash(x / 8, y / 8, seed) % 13) as i32 - 6;
            let edge = if hash(x / 24, y / 24, seed + 1).is_multiple_of(5) {
                95
            } else {
                0
            };
            let channel = |c: u32| {
                let noise =
                    (hash(x, y, seed + 2 + c) % (2 * amplitude + 1)) as i32 - amplitude as i32;
                (base + block + edge + noise + 20 * c as i32).clamp(0, 255) as u8
            };
            Rgba([
                channel(0),
                channel(1),
                channel(2),
                (hash(x, y, seed + 9) % 256) as u8,
            ])
        })
    }

    /// Thresholds that differ from chunk to chunk and are zero in some.
    fn varied(image: &RgbaImage, chunk_px: usize) -> ThresholdMap {
        let wide = (image.width() as usize).div_ceil(chunk_px);
        let high = (image.height() as usize).div_ceil(chunk_px);
        let chunks = (0..wide * high)
            .map(|i| ChunkThresholds {
                deblock: [0, 3, 7, 12, 8, 0, 20, 1, 255, 90, 300, 5000, -4][i % 13],
                dering: [4, 0, 1, 3, 9, 2, 0, 60, 255, 700, -1][i % 11],
            })
            .collect();
        ThresholdMap::new(chunk_px, wide, chunks)
    }

    #[test]
    fn deblock_gives_the_reference_bytes() {
        for (w, h, chunk) in [
            (512, 512, 128),
            (256, 512, 64),
            (512, 256, 32),
            (16, 16, 8),
            (24, 40, 16),
        ] {
            let original = scene(w, h, 3);
            let map = varied(&original, chunk);
            let (mut fast, mut slow) = (original.clone(), original.clone());
            deblock(&mut fast, &map);
            reference::deblock(&mut slow, &map);
            assert_eq!(fast, slow, "{w}x{h}");
        }
    }

    #[test]
    fn the_reference_comparison_exercises_the_deblocker() {
        let original = scene(512, 512, 3);
        let mut filtered = original.clone();
        deblock(&mut filtered, &varied(&original, 128));
        assert_ne!(filtered, original);
    }

    #[test]
    fn dering_gives_the_reference_bytes() {
        let sizes = [
            (512, 512, 128),
            (300, 200, 64),
            (20, 13, 8),
            (9, 9, 256),
            (1, 1, 8),
            (3, 17, 8),
        ];
        for (w, h, chunk) in sizes {
            for edge_range in [0, 32, 200] {
                let original = scene(w, h, 5);
                let map = varied(&original, chunk);
                let (mut fast, mut slow) = (original.clone(), original.clone());
                dering(&mut fast, &map, edge_range);
                reference::dering(&mut slow, &map, edge_range);
                assert_eq!(fast, slow, "{w}x{h} range {edge_range}");
            }
        }
        let original = scene(512, 512, 5);
        let mut filtered = original.clone();
        dering(&mut filtered, &varied(&original, 128), 32);
        assert_ne!(filtered, original);
    }

    #[test]
    fn unsharp_gives_the_reference_bytes() {
        let sizes = [
            (512, 512),
            (301, 77),
            (7, 5),
            (5, 7),
            (4, 4),
            (3, 3),
            (2, 9),
            (1, 1),
        ];
        for (w, h) in sizes {
            for amount in [77, 154, 256, -128, 1000] {
                let original = scene(w, h, 7);
                let (mut fast, mut slow) = (original.clone(), original.clone());
                unsharp(&mut fast, amount);
                reference::unsharp(&mut slow, amount);
                assert_eq!(fast, slow, "{w}x{h} amount {amount}");
            }
        }
    }

    #[test]
    fn a_seventh_is_exact_for_every_ramp_sum() {
        for x in 0..=(3 * 255 + 3i16) {
            assert_eq!(seventh(x), x / 7, "{x}");
        }
    }

    #[test]
    fn the_reciprocal_table_divides_exactly() {
        for count in 1..=9u32 {
            for n in 0..=(9 * 255 + 4) {
                assert_eq!(
                    (n * RECIPROCAL[count as usize]) >> 16,
                    n / count,
                    "{n} / {count}"
                );
            }
        }
    }

    /// Prints the time of each filter and of its reference on a full tile.
    /// `cargo test --release -p level0-bench filters::tests::timing -- --ignored --nocapture`
    #[test]
    #[ignore = "a timing, not a check"]
    fn timing() {
        let original = scene(4096, 4096, 11);
        let map = varied(&original, 256);
        let best = |f: &dyn Fn(&mut RgbaImage)| {
            (0..5)
                .map(|_| {
                    let mut image = original.clone();
                    let start = std::time::Instant::now();
                    f(&mut image);
                    start.elapsed().as_secs_f64() * 1000.0
                })
                .fold(f64::MAX, f64::min)
        };
        let report = |name: &str, new: &dyn Fn(&mut RgbaImage), old: &dyn Fn(&mut RgbaImage)| {
            println!(
                "{name:8} reference {:7.1} ms   now {:7.1} ms",
                best(old),
                best(new)
            );
        };
        report("deblock", &|i| deblock(i, &map), &|i| {
            reference::deblock(i, &map)
        });
        report("dering", &|i| dering(i, &map, 32), &|i| {
            reference::dering(i, &map, 32)
        });
        report("dering*", &|i| dering(i, &map, 0), &|i| {
            reference::dering(i, &map, 0)
        });
        report("unsharp", &|i| unsharp(i, 154), &|i| {
            reference::unsharp(i, 154)
        });
    }

    /// The first, straightforward implementations. The filters above must
    /// give exactly their bytes.
    mod reference {
        use super::super::BLOCK;
        use image::RgbaImage;

        /// Smooths block-edge steps that are below the chunk's `deblock` threshold.
        ///
        /// Where both sides of the edge are flat the step is replaced by a ramp over
        /// three pixels on each side, which is what removes visible blocking in
        /// fields and water. Elsewhere only the two edge pixels move, by at most
        /// half the threshold (the normal-mode filter of H.264).
        pub fn deblock(image: &mut RgbaImage, map: &super::super::ThresholdMap) {
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
        pub fn dering(image: &mut RgbaImage, map: &super::super::ThresholdMap, edge_range: i32) {
            let (w, h) = (image.width() as usize, image.height() as usize);
            let src = image.as_raw().clone();
            let data: &mut [u8] = image;
            let luma = |i: usize| -> i32 {
                (77 * i32::from(src[i]) + 150 * i32::from(src[i + 1]) + 29 * i32::from(src[i + 2]))
                    >> 8
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
    }
}
