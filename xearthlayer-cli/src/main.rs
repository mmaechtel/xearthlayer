//! XEarthLayer CLI - Command-line interface
//!
//! This binary provides a command-line interface to the XEarthLayer library.

use clap::{Parser, ValueEnum};
use std::process;
use xearthlayer::coord::to_tile_coords;
use xearthlayer::orchestrator::TileOrchestrator;
use xearthlayer::provider::{BingMapsProvider, GoogleMapsProvider, Provider, ReqwestClient};

#[derive(Debug, Clone, ValueEnum)]
enum ProviderType {
    /// Bing Maps aerial imagery (no API key required)
    Bing,
    /// Google Maps satellite imagery (requires API key)
    Google,
}

#[derive(Parser)]
#[command(name = "xearthlayer")]
#[command(about = "Download satellite imagery tiles for X-Plane", long_about = None)]
struct Args {
    /// Latitude in decimal degrees
    #[arg(long)]
    lat: f64,

    /// Longitude in decimal degrees
    #[arg(long)]
    lon: f64,

    /// Zoom level (max: 15 for Bing, 18 for Google)
    #[arg(long, default_value = "15")]
    zoom: u8,

    /// Output file path (JPEG format)
    #[arg(long)]
    output: String,

    /// Imagery provider to use
    #[arg(long, value_enum, default_value = "bing")]
    provider: ProviderType,

    /// Google Maps API key (required when using --provider google)
    #[arg(long, required_if_eq("provider", "google"))]
    google_api_key: Option<String>,
}

fn main() {
    let args = Args::parse();

    // Validate zoom level
    if args.zoom < 1 || args.zoom > 19 {
        eprintln!("Error: Zoom level must be between 1 and 19");
        process::exit(1);
    }

    // Convert coordinates to tile
    let tile = match to_tile_coords(args.lat, args.lon, args.zoom) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error converting coordinates: {}", e);
            process::exit(1);
        }
    };

    println!("Downloading tile for:");
    println!("  Location: {}, {}", args.lat, args.lon);
    println!("  Zoom: {}", args.zoom);
    println!(
        "  Tile: row={}, col={}, zoom={}",
        tile.row, tile.col, tile.zoom
    );
    println!();

    // Create HTTP client
    let http_client = match ReqwestClient::new() {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Error creating HTTP client: {}", e);
            process::exit(1);
        }
    };

    // Create provider based on selection
    match args.provider {
        ProviderType::Bing => {
            let provider = BingMapsProvider::new(http_client);
            let name = provider.name().to_string();
            let max = provider.max_zoom();

            // Validate zoom level - chunks are downloaded at zoom+4
            if args.zoom + 4 > max {
                eprintln!(
                    "Error: Zoom level {} requires chunks at zoom {}, but {} only supports up to zoom {}",
                    args.zoom,
                    args.zoom + 4,
                    name,
                    max
                );
                eprintln!("Maximum usable zoom level for {} is {}", name, max - 4);
                process::exit(1);
            }

            println!("Using provider: {} (no API key required)", name);

            // Create orchestrator (30s timeout, 3 retries per chunk, 32 parallel downloads)
            let orchestrator = TileOrchestrator::new(provider, 30, 3, 32);
            download_and_save(orchestrator, &tile, &args.output);
        }
        ProviderType::Google => {
            let api_key = args.google_api_key.clone().unwrap(); // Safe: required_if_eq
            let provider = GoogleMapsProvider::new(http_client, api_key);
            let name = provider.name().to_string();
            let max = provider.max_zoom();

            // Validate zoom level
            if args.zoom + 4 > max {
                eprintln!(
                    "Error: Zoom level {} requires chunks at zoom {}, but {} only supports up to zoom {}",
                    args.zoom,
                    args.zoom + 4,
                    name,
                    max
                );
                eprintln!("Maximum usable zoom level for {} is {}", name, max - 4);
                process::exit(1);
            }

            println!("Using provider: {} (authenticated)", name);

            // Create orchestrator
            let orchestrator = TileOrchestrator::new(provider, 30, 3, 32);
            download_and_save(orchestrator, &tile, &args.output);
        }
    }
}

fn download_and_save<P: Provider + 'static>(
    orchestrator: TileOrchestrator<P>,
    tile: &xearthlayer::coord::TileCoord,
    output_path: &str,
) {
    // Download tile
    println!("Downloading 256 chunks in parallel...");
    let start = std::time::Instant::now();

    let image = match orchestrator.download_tile(tile) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("Error downloading tile: {}", e);
            process::exit(1);
        }
    };

    let elapsed = start.elapsed();

    println!("Downloaded successfully in {:.2}s", elapsed.as_secs_f64());
    println!("Image size: {}x{}", image.width(), image.height());
    println!();

    // Save as JPEG with 90% quality
    println!("Saving to {}...", output_path);

    // Convert RGBA to RGB for JPEG
    let rgb_image = image::DynamicImage::ImageRgba8(image).to_rgb8();

    // Use JPEG encoder with quality setting
    let file = match std::fs::File::create(output_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error creating output file: {}", e);
            process::exit(1);
        }
    };

    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, 90);

    match encoder.encode(
        rgb_image.as_raw(),
        rgb_image.width(),
        rgb_image.height(),
        image::ExtendedColorType::Rgb8,
    ) {
        Ok(_) => {
            println!("Saved successfully with 90% quality!");
        }
        Err(e) => {
            eprintln!("Error encoding JPEG: {}", e);
            process::exit(1);
        }
    }
}
