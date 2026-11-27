//! Level 0 through XEL's own BC1 path and back to RGBA.
//!
//! The encode is `xearthlayer::dds::DdsEncoder` with its default compressor
//! (ISPC), without mipmaps: exactly the bytes level 0 gets in a served tile.
//! The decode follows the BC1 definition a GPU implements.

use image::RgbaImage;
use std::time::{Duration, Instant};
use xearthlayer::dds::{DdsEncoder, DdsFormat};

/// Size of the DDS header that precedes the block data.
const DDS_HEADER_BYTES: usize = 128;

/// Encodes `image` to BC1 with XEL's encoder and decodes it again. Also
/// returns the time the encode took.
pub fn round_trip(image: &RgbaImage) -> (RgbaImage, Duration) {
    let (width, height) = image.dimensions();
    let encoder = DdsEncoder::new(DdsFormat::BC1).without_mipmaps();
    let start = Instant::now();
    let dds = encoder
        .encode(image.clone())
        .expect("BC1 encode of a multiple-of-4 image");
    let elapsed = start.elapsed();
    (decode(&dds[DDS_HEADER_BYTES..], width, height), elapsed)
}

fn expand565(c: u16) -> [u32; 3] {
    let r = u32::from(c >> 11) & 0x1F;
    let g = u32::from(c >> 5) & 0x3F;
    let b = u32::from(c) & 0x1F;
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

/// Decodes BC1 block data of a `width` x `height` image (multiples of 4).
pub fn decode(blocks: &[u8], width: u32, height: u32) -> RgbaImage {
    let mut out = RgbaImage::new(width, height);
    let blocks_wide = (width / 4) as usize;
    for (index, block) in blocks.chunks_exact(8).enumerate() {
        let c0 = u16::from_le_bytes([block[0], block[1]]);
        let c1 = u16::from_le_bytes([block[2], block[3]]);
        let (a, b) = (expand565(c0), expand565(c1));
        let mix = |wa: u32, wb: u32, div: u32| -> [u32; 3] {
            [0, 1, 2].map(|i| (wa * a[i] + wb * b[i] + div / 2) / div)
        };
        let palette = if c0 > c1 {
            [a, b, mix(2, 1, 3), mix(1, 2, 3)]
        } else {
            [a, b, mix(1, 1, 2), [0, 0, 0]]
        };
        let bits = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
        let bx = (index % blocks_wide) as u32 * 4;
        let by = (index / blocks_wide) as u32 * 4;
        for i in 0..16u32 {
            let c = palette[((bits >> (2 * i)) & 3) as usize];
            out.put_pixel(
                bx + i % 4,
                by + i / 4,
                image::Rgba([c[0] as u8, c[1] as u8, c[2] as u8, 255]),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn a_flat_image_survives_within_565_precision() {
        let image = RgbaImage::from_pixel(16, 16, Rgba([200, 100, 50, 255]));
        let (decoded, _) = round_trip(&image);
        for p in decoded.pixels() {
            assert!(p[0].abs_diff(200) <= 4, "{p:?}");
            assert!(p[1].abs_diff(100) <= 2, "{p:?}");
            assert!(p[2].abs_diff(50) <= 4, "{p:?}");
        }
    }

    #[test]
    fn decodes_the_four_colour_palette() {
        // c0 = white, c1 = black, indices 0,1,2,3 repeated per row.
        let block = [0xFF, 0xFF, 0x00, 0x00, 0xE4, 0xE4, 0xE4, 0xE4];
        let image = decode(&block, 4, 4);
        let row: Vec<u8> = (0..4).map(|x| image.get_pixel(x, 0)[0]).collect();
        assert_eq!(row, vec![255, 0, 170, 85]);
    }
}
