//! MESSPROBE (#484, not for merge): quality of whole tiles encoded by the
//! real GPU worker (kernel chosen by `XEL_PROBE_BC1`) against ISPC through
//! `DdsEncoder`, on the synthetic tile and the real one
//! (`XEL_BENCH_CHUNKS`, `XEL_BENCH_TILE`). Per image: the chain's PSNR
//! against the CPU chain for each encoder, level 0's, and the decoded
//! difference between the two encoders.

#[path = "support/gpu_bench.rs"]
#[allow(dead_code)]
mod gpu_bench;

use std::sync::Arc;

use gpu_bench::*;
use image::RgbaImage;
use xearthlayer::dds::gpu_channel::{create_gpu_encoder_channels, CHANNEL_CAPACITY};
use xearthlayer::dds::{DdsEncoder, DdsFormat, IspcCompressor, MipmapCompressor, MipmapGenerator};

const LEVELS: usize = 13;
const HEADER: usize = 128;

/// Sum of squared RGB differences and the channel count.
fn sq(a: &RgbaImage, b: &RgbaImage) -> (f64, f64) {
    let (rmse, _) = rgb_error(a, b);
    let n = f64::from(a.width()) * f64::from(a.height()) * 3.0;
    (rmse * rmse * n, n)
}

fn psnr(sum: f64, n: f64) -> f64 {
    10.0 * (255.0 * 255.0 / (sum / n)).log10()
}

/// The decoded levels of a DDS file of `levels` (padded sources).
fn decode_levels(dds: &[u8], levels: &[RgbaImage]) -> Vec<RgbaImage> {
    let mut at = HEADER;
    let out = levels
        .iter()
        .map(|l| {
            let len = (l.width() / 4 * (l.height() / 4) * 8) as usize;
            let img = decode_bc1(&dds[at..at + len], l.width(), l.height());
            at += len;
            img
        })
        .collect();
    assert_eq!(at, dds.len(), "the file is header plus the chain");
    out
}

fn report(name: &str, levels: &[RgbaImage], decoded: &[RgbaImage]) {
    let (mut s, mut n) = (0.0, 0.0);
    let mut worst = f64::INFINITY;
    for (l, d) in levels.iter().zip(decoded) {
        let (a, b) = sq(l, d);
        s += a;
        n += b;
        if a > 0.0 {
            worst = worst.min(psnr(a, b));
        }
    }
    let (a, b) = sq(&levels[0], &decoded[0]);
    eprintln!(
        "  {name:<22} PSNR chain {:.3} dB, level 0 {:.3} dB, worst level {:.2} dB",
        psnr(s, n),
        psnr(a, b),
        worst
    );
}

#[test]
#[ignore]
fn whole_tile_quality_gpu_against_ispc() {
    eprintln!("Machine: {}", machine_label());
    eprintln!(
        "XEL_PROBE_BC1 = {}",
        std::env::var("XEL_PROBE_BC1").unwrap_or_else(|_| "(unset: block_compression)".into())
    );
    let selector = gpu_selectors().remove(0);
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (channels, mut worker, _sel) =
        create_gpu_encoder_channels(&selector, CHANNEL_CAPACITY, shutdown).expect("the GPU");
    let channel = Arc::new(channels.demand);
    let gpu = DdsEncoder::new(DdsFormat::BC1)
        .with_mipmap_compressor(
            Arc::clone(&channel) as Arc<dyn MipmapCompressor>,
            Arc::new(IspcCompressor),
        )
        .with_mipmap_count(LEVELS);
    let ispc = DdsEncoder::new(DdsFormat::BC1)
        .with_compressor(Arc::new(IspcCompressor))
        .with_mipmap_count(LEVELS);

    for (name, image) in test_images() {
        eprintln!("-- {name} --");
        let levels: Vec<RgbaImage> = MipmapGenerator::generate_chain_with_count(&image, LEVELS)
            .iter()
            .map(pad_to_blocks)
            .collect();
        let g = gpu.encode(image.clone()).expect("GPU encodes");
        let i = ispc.encode(image).expect("ISPC encodes");
        assert_eq!(g.len(), i.len());
        assert_eq!(g[..HEADER], i[..HEADER], "the same header");
        let (dg, di) = (decode_levels(&g, &levels), decode_levels(&i, &levels));
        report("GPU worker", &levels, &dg);
        report("ISPC", &levels, &di);
        let (mut s, mut n) = (0.0, 0.0);
        for (a, b) in dg.iter().zip(&di) {
            let (x, y) = sq(a, b);
            s += x;
            n += y;
        }
        let same = g[HEADER..]
            .chunks_exact(8)
            .zip(i[HEADER..].chunks_exact(8))
            .filter(|(a, b)| a == b)
            .count();
        eprintln!(
            "  GPU against ISPC, decoded: PSNR chain {:.3} dB; blocks identical {:.2}%",
            psnr(s, n),
            100.0 * same as f64 / ((g.len() - HEADER) / 8) as f64
        );
    }
    let stats = channel.stats().take_sample();
    if let Some(s) = stats {
        eprintln!(
            "  tiles={} failed={} fallback={}",
            s.tiles(),
            s.failed,
            s.fallback
        );
    }
    drop(gpu);
    worker.stop_blocking(std::time::Duration::from_secs(10));
}
