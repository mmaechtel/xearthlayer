//! Offline benchmark: does treating level 0 (deblock, dering, sharpen)
//! bring ZL16 ortho imagery measurably closer to the ZL18 truth?
//!
//! A research tool for the ideas in samsoir/xearthlayer#458. It changes
//! nothing in XEL: the filters live here, and the runtime does not link
//! this crate. See `README.md` beside this crate's manifest.

mod bc1;
mod dataset;
mod filters;
mod jpeg_info;
mod metrics;
mod report;
mod synth;
mod variants;

use clap::{Parser, Subcommand};
use rayon::prelude::*;
use report::{Crop, Row};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use variants::VARIANTS;

#[derive(Parser)]
#[command(about = "Level 0 sharpening benchmark (research tool)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Try one chunk from every provider and report which answer.
    Probe,
    /// Download ZL16 and ZL18 chunks of the benchmark locations.
    Fetch {
        /// Dataset directory (gitignored).
        #[arg(long, default_value = "level0-bench/data")]
        data: PathBuf,
        /// Providers to fetch.
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "bing,go2,arcgis,apple,usgs"
        )]
        providers: Vec<String>,
        /// ZL16 chunks per side of a benchmark tile (4 is 1024 pixels).
        #[arg(long, default_value_t = 4)]
        side: u32,
    },
    /// Write the synthetic dataset (no network).
    Synth {
        #[arg(long, default_value = "level0-bench/data")]
        data: PathBuf,
        /// JPEG qualities, one synthetic provider each.
        #[arg(long, value_delimiter = ',', default_value = "60,75,90")]
        qualities: Vec<u8>,
        /// JPEG qualities of extra providers whose ZL16 is softened.
        #[arg(long, value_delimiter = ',', default_value = "75")]
        soft: Vec<u8>,
        #[arg(long, default_value_t = 4)]
        side: u32,
    },
    /// Run every variant over the dataset and write the results.
    Run {
        #[arg(long, default_value = "level0-bench/data")]
        data: PathBuf,
        #[arg(long, default_value = "level0-bench/results")]
        out: PathBuf,
        /// Hand-written Markdown appended to summary.md.
        #[arg(long)]
        notes: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Probe => {
            let reachable = dataset::probe();
            println!(
                "{} of {} providers reachable",
                reachable.len(),
                dataset::PROVIDERS.len()
            );
            Ok(())
        }
        Command::Fetch {
            data,
            providers,
            side,
        } => dataset::fetch(&data, &providers, side),
        Command::Synth {
            data,
            qualities,
            soft,
            side,
        } => {
            let sharp = qualities.iter().map(|&q| (q, false));
            let profiles: Vec<(u8, bool)> = sharp.chain(soft.iter().map(|&q| (q, true))).collect();
            dataset::write_synthetic(&data, &profiles, side)
        }
        Command::Run { data, out, notes } => run(&data, &out, notes.as_deref()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Milliseconds scaled from an image of `pixels` to a 4096 x 4096 tile.
fn ms_per_tile(elapsed: std::time::Duration, pixels: u64) -> f64 {
    elapsed.as_secs_f64() * 1000.0 * (4096.0 * 4096.0) / pixels as f64
}

fn run(data: &Path, out: &Path, notes: Option<&Path>) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    let entries = dataset::read_manifests(data)?;
    if entries.is_empty() {
        return Err(format!(
            "no manifest under {}; run `synth` or `fetch` first",
            data.display()
        ));
    }
    let mut rows = Vec::new();
    for entry in &entries {
        let tile = dataset::load(data, entry)?;
        let truth = metrics::Truth::new(&tile.truth);
        let pixels = u64::from(tile.source.width()) * u64::from(tile.source.height());
        // Filters and encodes run one at a time so their timings are clean;
        // only the scoring runs in parallel.
        let outputs: Vec<_> = VARIANTS
            .iter()
            .map(|v| {
                let (filtered, filter_time) = v.apply(&tile.source, &tile.chunk_info);
                let (decoded, bc1_time) = bc1::round_trip(&filtered);
                (filtered, decoded, filter_time, bc1_time)
            })
            .collect();
        let summary = entry.source_jpeg.as_ref();
        let scored: Vec<Row> = outputs
            .par_iter()
            .enumerate()
            .map(
                |(variant, (filtered, decoded, filter_time, bc1_time))| Row {
                    provider: entry.provider.clone(),
                    location: entry.location.clone(),
                    content: entry.content.clone(),
                    synthetic: entry.synthetic,
                    quality: summary.map(|s| s.first_chunk.quality_estimate),
                    subsampling: summary.map(|s| s.first_chunk.subsampling.clone()),
                    variant,
                    pre: truth.score(filtered),
                    post: truth.score(decoded),
                    filter_ms: ms_per_tile(*filter_time, pixels),
                    bc1_ms: ms_per_tile(*bc1_time, pixels),
                },
            )
            .collect();
        println!(
            "{} {}: baseline SSIM {:.4} after BC1",
            entry.provider, entry.location, scored[0].post.ssim
        );
        rows.extend(scored);
    }

    fs::create_dir_all(out.join("crops")).map_err(io)?;
    report::write_csv(&rows, &out.join("results.csv")).map_err(io)?;
    let notes = match notes {
        Some(path) => fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?,
        None => String::new(),
    };
    fs::write(out.join("summary.md"), report::summary(&rows, &notes)).map_err(io)?;

    let mut crops = Vec::new();
    for entry in &entries {
        let best = report::best_variant(&rows, &entry.provider);
        let tile = dataset::load(data, entry)?;
        let (x, y) = report::busiest_window(&tile.truth);
        let (baseline, _) = bc1::round_trip(&tile.source);
        let (filtered, _) = VARIANTS[best].apply(&tile.source, &tile.chunk_info);
        let (treated, _) = bc1::round_trip(&filtered);
        let file = format!("{}__{}.png", entry.provider, entry.location);
        report::triptych([&baseline, &treated, &tile.truth], x, y)
            .save(out.join("crops").join(&file))
            .map_err(|e| e.to_string())?;
        let ssim = |variant: usize| {
            rows.iter()
                .find(|r| {
                    r.provider == entry.provider
                        && r.location == entry.location
                        && r.variant == variant
                })
                .map_or(f64::NAN, |r| r.post.ssim)
        };
        crops.push(Crop {
            provider: entry.provider.clone(),
            location: entry.location.clone(),
            content: entry.content.clone(),
            file,
            variant: best,
            ssim_base: ssim(0),
            ssim_best: ssim(best),
        });
    }
    fs::write(out.join("index.html"), report::contact_sheet(&crops)).map_err(io)?;
    println!("wrote {}", out.join("summary.md").display());
    Ok(())
}
