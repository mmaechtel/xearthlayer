//! Image quality metrics against a ground truth: SSIM and PSNR on luma, the
//! same on linear-light luminance, and butteraugli.

use butteraugli::{ButteraugliParams, Img, RGB8};
use image::RgbaImage;

/// Scores of one candidate image against the truth.
#[derive(Debug, Clone, Copy, Default)]
pub struct Scores {
    /// SSIM of BT.601 luma computed from the stored sRGB bytes.
    pub ssim: f64,
    /// PSNR of the same luma, in dB.
    pub psnr: f64,
    /// SSIM of linear-light luminance (BT.709 weights), scaled to 0..255.
    pub ssim_linear: f64,
    /// Butteraugli 3-norm (lower is better).
    pub butteraugli: f64,
}

/// A truth image with everything precomputed that every comparison reuses.
pub struct Truth {
    width: usize,
    height: usize,
    luma: Vec<f32>,
    linear: Vec<f32>,
    rgb: Vec<RGB8>,
}

impl Truth {
    pub fn new(image: &RgbaImage) -> Self {
        Self {
            width: image.width() as usize,
            height: image.height() as usize,
            luma: luma(image),
            linear: linear_luminance(image),
            rgb: rgb8(image),
        }
    }

    /// Scores `candidate`, which must have the truth's dimensions.
    pub fn score(&self, candidate: &RgbaImage) -> Scores {
        let y = luma(candidate);
        let lin = linear_luminance(candidate);
        let params = ButteraugliParams::default();
        let butteraugli = butteraugli::butteraugli(
            Img::new(rgb8(candidate), self.width, self.height).as_ref(),
            Img::new(self.rgb.clone(), self.width, self.height).as_ref(),
            &params,
        )
        .map(|r| r.pnorm_3)
        .unwrap_or(f64::NAN);
        Scores {
            ssim: ssim(&y, &self.luma, self.width, self.height),
            psnr: psnr(&y, &self.luma),
            ssim_linear: ssim(&lin, &self.linear, self.width, self.height),
            butteraugli,
        }
    }
}

fn rgb8(image: &RgbaImage) -> Vec<RGB8> {
    image
        .pixels()
        .map(|p| RGB8::new(p[0], p[1], p[2]))
        .collect()
}

/// BT.601 luma of the stored bytes, 0..255.
pub fn luma(image: &RgbaImage) -> Vec<f32> {
    image
        .pixels()
        .map(|p| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
        .collect()
}

/// Linear-light luminance (BT.709 weights) scaled to 0..255.
fn linear_luminance(image: &RgbaImage) -> Vec<f32> {
    let table: Vec<f32> = (0..=255u8).map(butteraugli::srgb_to_linear).collect();
    image
        .pixels()
        .map(|p| {
            255.0
                * (0.2126 * table[usize::from(p[0])]
                    + 0.7152 * table[usize::from(p[1])]
                    + 0.0722 * table[usize::from(p[2])])
        })
        .collect()
}

fn psnr(a: &[f32], b: &[f32]) -> f64 {
    let mse = a
        .iter()
        .zip(b)
        .map(|(&x, &y)| f64::from(x - y).powi(2))
        .sum::<f64>()
        / a.len() as f64;
    if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    }
}

/// The 11 tap Gaussian window of the SSIM paper (sigma 1.5).
fn window() -> [f32; 11] {
    let mut w = [0.0f32; 11];
    let mut sum = 0.0;
    for (i, v) in w.iter_mut().enumerate() {
        let d = i as f32 - 5.0;
        *v = (-d * d / (2.0 * 1.5 * 1.5)).exp();
        sum += *v;
    }
    w.map(|v| v / sum)
}

/// Gaussian-windowed mean over the valid region: the result is
/// `(w - 10) x (h - 10)`.
fn blur_valid(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let k = window();
    let ow = w - 10;
    let oh = h - 10;
    let mut rows = vec![0.0f32; ow * h];
    for y in 0..h {
        let line = &src[y * w..(y + 1) * w];
        for x in 0..ow {
            let mut acc = 0.0;
            for (i, &kv) in k.iter().enumerate() {
                acc += kv * line[x + i];
            }
            rows[y * ow + x] = acc;
        }
    }
    let mut out = vec![0.0f32; ow * oh];
    for y in 0..oh {
        for x in 0..ow {
            let mut acc = 0.0;
            for (i, &kv) in k.iter().enumerate() {
                acc += kv * rows[(y + i) * ow + x];
            }
            out[y * ow + x] = acc;
        }
    }
    out
}

/// Mean SSIM (Wang et al. 2004) of two single-channel 0..255 images.
pub fn ssim(a: &[f32], b: &[f32], w: usize, h: usize) -> f64 {
    const C1: f32 = (0.01 * 255.0) * (0.01 * 255.0);
    const C2: f32 = (0.03 * 255.0) * (0.03 * 255.0);
    let product =
        |x: &[f32], y: &[f32]| -> Vec<f32> { x.iter().zip(y).map(|(p, q)| p * q).collect() };
    let mu_a = blur_valid(a, w, h);
    let mu_b = blur_valid(b, w, h);
    let aa = blur_valid(&product(a, a), w, h);
    let bb = blur_valid(&product(b, b), w, h);
    let ab = blur_valid(&product(a, b), w, h);
    let mut sum = 0.0f64;
    for i in 0..mu_a.len() {
        let (ma, mb) = (mu_a[i], mu_b[i]);
        let va = aa[i] - ma * ma;
        let vb = bb[i] - mb * mb;
        let cov = ab[i] - ma * mb;
        let value =
            ((2.0 * ma * mb + C1) * (2.0 * cov + C2)) / ((ma * ma + mb * mb + C1) * (va + vb + C2));
        sum += f64::from(value);
    }
    sum / mu_a.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn gradient(offset: u8) -> RgbaImage {
        RgbaImage::from_fn(64, 64, |x, y| {
            let v = ((x * 3 + y * 2) % 200) as u8 + offset;
            Rgba([v, v / 2 + 20, 255 - v, 255])
        })
    }

    #[test]
    fn an_image_scores_perfectly_against_itself() {
        let image = gradient(0);
        let scores = Truth::new(&image).score(&image);
        assert!((scores.ssim - 1.0).abs() < 1e-6);
        assert!((scores.ssim_linear - 1.0).abs() < 1e-6);
        assert!(scores.psnr.is_infinite());
        assert!(scores.butteraugli < 1e-6);
    }

    #[test]
    fn a_noisier_image_scores_worse() {
        let truth = gradient(0);
        let noisy = |amplitude: u8| {
            let mut image = truth.clone();
            for (i, p) in image.pixels_mut().enumerate() {
                if i % 2 == 0 {
                    p[1] = p[1].saturating_add(amplitude);
                }
            }
            image
        };
        let t = Truth::new(&truth);
        let mild = t.score(&noisy(6));
        let strong = t.score(&noisy(30));
        assert!(mild.ssim > strong.ssim);
        assert!(mild.psnr > strong.psnr);
        assert!(mild.butteraugli < strong.butteraugli);
        assert!(mild.ssim < 1.0);
    }
}
