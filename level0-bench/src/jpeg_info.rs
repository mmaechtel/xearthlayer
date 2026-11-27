//! JPEG header facts the benchmark needs: quantisation tables, chroma
//! subsampling and an estimate of the encoder quality.
//!
//! Only the header is read. The pixels are decoded by the `image` crate,
//! which is what XEL's own `chunk_image::decode` runs.

use serde::{Deserialize, Serialize};

/// Zigzag scan position to natural (row-major) position in an 8x8 block.
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// The luminance table of ITU-T T.81 Annex K, in natural order. Quality 50
/// in libjpeg's scale.
const STD_LUMA: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];

/// What a JPEG header says about how the image was compressed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JpegInfo {
    /// Luminance quantisation table, natural (row-major) order.
    pub luma_q: Vec<u16>,
    /// Chrominance quantisation table, natural order, when present.
    pub chroma_q: Option<Vec<u16>>,
    /// libjpeg quality (1 to 100) whose scaled Annex K table is closest to
    /// `luma_q`. Exact for libjpeg-style encoders, approximate otherwise.
    pub quality_estimate: u8,
    /// Chroma subsampling, for example `4:2:0`.
    pub subsampling: String,
    pub progressive: bool,
}

impl JpegInfo {
    /// Largest step between two neighbouring blocks that quantisation alone
    /// can plausibly cause, in 8-bit levels.
    ///
    /// The DC coefficient is 8 times the block mean, so its rounding moves a
    /// block by up to `Q00 / 16` and a boundary by `Q00 / 8`. A first order
    /// AC basis function reaches about 0.17 of its coefficient at the block
    /// edge, a second order one about 0.1 after the signs partly cancel.
    pub fn block_step(&self) -> f32 {
        let q = |i: usize| f32::from(self.luma_q[i]);
        q(0) / 8.0 + 0.17 * (q(1) + q(8)) + 0.1 * (q(2) + q(16) + q(9))
    }

    /// Typical ringing amplitude next to a strong edge, in 8-bit levels: a
    /// quarter of the mean low and middle frequency step.
    pub fn ring_amplitude(&self) -> f32 {
        let mut sum = 0.0;
        let mut count = 0.0;
        for v in 0..8 {
            for u in 0..8 {
                if (1..=4).contains(&(u + v)) {
                    sum += f32::from(self.luma_q[v * 8 + u]);
                    count += 1.0;
                }
            }
        }
        sum / count / 4.0
    }
}

/// One frame component: horizontal and vertical sampling factor and the
/// index of its quantisation table.
type Component = (u8, u8, u8);

/// Reads the header of `bytes`, or `None` when it is not a baseline or
/// progressive JPEG with a luminance table.
pub fn parse(bytes: &[u8]) -> Option<JpegInfo> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut tables: [Option<Vec<u16>>; 4] = [None, None, None, None];
    let mut frame: Option<(bool, Vec<Component>)> = None;
    let mut pos = 2;
    while pos + 4 <= bytes.len() {
        if bytes[pos] != 0xFF {
            return None;
        }
        let marker = bytes[pos + 1];
        if marker == 0xFF {
            pos += 1;
            continue;
        }
        let len = usize::from(u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]));
        let body = bytes.get(pos + 4..pos + 2 + len)?;
        match marker {
            0xDB => read_tables(body, &mut tables)?,
            0xC0..=0xC2 => {
                let count = usize::from(*body.get(5)?);
                let mut components = Vec::with_capacity(count);
                for c in 0..count {
                    let at = 6 + c * 3;
                    let hv = *body.get(at + 1)?;
                    components.push((hv >> 4, hv & 0x0F, *body.get(at + 2)?));
                }
                frame = Some((marker == 0xC2, components));
            }
            0xDA => break,
            _ => {}
        }
        pos += 2 + len;
    }
    let (progressive, components) = frame?;
    let luma_id = usize::from(components.first()?.2);
    let luma_q = tables.get(luma_id)?.clone()?;
    let chroma_q = components
        .get(1)
        .and_then(|c| tables.get(usize::from(c.2)).cloned().flatten());
    Some(JpegInfo {
        quality_estimate: estimate_quality(&luma_q),
        subsampling: subsampling(&components),
        luma_q,
        chroma_q,
        progressive,
    })
}

fn read_tables(mut body: &[u8], tables: &mut [Option<Vec<u16>>; 4]) -> Option<()> {
    while !body.is_empty() {
        let wide = body[0] >> 4 != 0;
        let id = usize::from(body[0] & 0x0F);
        let size = if wide { 128 } else { 64 };
        let data = body.get(1..1 + size)?;
        let mut table = vec![0u16; 64];
        for (k, &natural) in ZIGZAG.iter().enumerate() {
            table[natural] = if wide {
                u16::from_be_bytes([data[k * 2], data[k * 2 + 1]])
            } else {
                u16::from(data[k])
            };
        }
        *tables.get_mut(id)? = Some(table);
        body = &body[1 + size..];
    }
    Some(())
}

fn subsampling(components: &[Component]) -> String {
    match components {
        [_] => "gray".to_string(),
        [y, c, ..] if c.0 == 1 && c.1 == 1 => match (y.0, y.1) {
            (1, 1) => "4:4:4".to_string(),
            (2, 1) => "4:2:2".to_string(),
            (2, 2) => "4:2:0".to_string(),
            (1, 2) => "4:4:0".to_string(),
            (4, 1) => "4:1:1".to_string(),
            (h, v) => format!("{h}x{v}"),
        },
        _ => "other".to_string(),
    }
}

/// The libjpeg quality whose table is closest to `luma_q` in summed absolute
/// difference.
fn estimate_quality(luma_q: &[u16]) -> u8 {
    (1..=100u8)
        .min_by_key(|&quality| {
            let scale = if quality < 50 {
                5000 / u32::from(quality)
            } else {
                200 - 2 * u32::from(quality)
            };
            STD_LUMA
                .iter()
                .zip(luma_q)
                .map(|(&std, &q)| {
                    let scaled = ((u32::from(std) * scale + 50) / 100).clamp(1, 255);
                    scaled.abs_diff(u32::from(q))
                })
                .sum::<u32>()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/../xearthlayer/tests/fixtures/chunks/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(path).unwrap()
    }

    #[test]
    fn reads_quality_and_subsampling_of_a_420_chunk() {
        let info = parse(&fixture("ycc420_q75.jpg")).unwrap();
        assert_eq!(info.subsampling, "4:2:0");
        assert_eq!(info.quality_estimate, 75);
        assert_eq!(info.luma_q[0], 8);
        assert!(info.chroma_q.is_some());
        assert!(!info.progressive);
    }

    #[test]
    fn reads_progressive_444() {
        let info = parse(&fixture("ycc444_q70_progressive.jpg")).unwrap();
        assert_eq!(info.subsampling, "4:4:4");
        assert!(info.progressive);
        assert_eq!(info.quality_estimate, 70);
    }

    #[test]
    fn rejects_what_is_not_a_jpeg() {
        assert_eq!(parse(b"<html>not an image</html>"), None);
    }

    #[test]
    fn a_coarser_table_expects_larger_steps() {
        let fine = parse(&fixture("ycc420_q75.jpg")).unwrap();
        let coarse = parse(&fixture("ycc420_q60_restart.jpg")).unwrap();
        assert!(coarse.block_step() > fine.block_step());
        assert!(coarse.ring_amplitude() > fine.ring_amplitude());
    }
}
