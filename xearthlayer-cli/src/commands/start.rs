//! Start command - mount a single scenery pack with FUSE passthrough.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use xearthlayer::config::{derive_mountpoint, DownloadConfig, TextureConfig};
use xearthlayer::service::ServiceConfig;

use super::common::{resolve_dds_format, resolve_provider, DdsCompression, ProviderType};
use crate::error::CliError;
use crate::runner::CliRunner;

/// Arguments for the start command.
pub struct StartArgs {
    pub source: String,
    pub mountpoint: Option<String>,
    pub provider: Option<ProviderType>,
    pub google_api_key: Option<String>,
    pub dds_format: Option<DdsCompression>,
    pub timeout: Option<u64>,
    pub parallel: Option<usize>,
    pub no_cache: bool,
    pub fuse3: bool,
}

/// Run the start command.
#[allow(clippy::too_many_arguments)]
pub fn run(args: StartArgs) -> Result<(), CliError> {
    let runner = CliRunner::new()?;
    runner.log_startup("start");
    let config = runner.config();

    // Determine mountpoint: CLI > config > auto-detect
    let mountpoint = match args.mountpoint {
        Some(mp) => mp,
        None => {
            // Try config scenery_dir first
            if let Some(ref scenery_dir) = config.xplane.scenery_dir {
                let source_path = Path::new(&args.source);
                let pack_name = source_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "scenery".to_string());
                scenery_dir.join(&pack_name).to_string_lossy().to_string()
            } else {
                // Fall back to auto-detection
                let source_path = Path::new(&args.source);
                match derive_mountpoint(source_path) {
                    Ok(mp) => {
                        println!("Auto-detected X-Plane 12 Custom Scenery directory");
                        mp.to_string_lossy().to_string()
                    }
                    Err(e) => {
                        return Err(CliError::Config(format!(
                            "Could not determine mountpoint: {}. \
                             Set scenery_dir in config.ini or use --mountpoint.",
                            e
                        )));
                    }
                }
            }
        }
    };

    // Resolve settings from CLI and config
    let provider_config = resolve_provider(args.provider, args.google_api_key, config)?;
    let format = resolve_dds_format(args.dds_format, config);
    let timeout_secs = args.timeout.unwrap_or(config.download.timeout);
    let parallel_downloads = args.parallel.unwrap_or(config.download.parallel);

    // Build configurations
    let texture_config = TextureConfig::new(format).with_mipmap_count(5);

    let download_config = DownloadConfig::new()
        .with_timeout_secs(timeout_secs)
        .with_max_retries(3)
        .with_parallel_downloads(parallel_downloads);

    let service_config = ServiceConfig::builder()
        .texture(texture_config)
        .download(download_config)
        .cache_enabled(!args.no_cache)
        .cache_directory(config.cache.directory.clone())
        .cache_memory_size(config.cache.memory_size)
        .cache_disk_size(config.cache.disk_size)
        .generation_threads(config.generation.threads)
        .generation_timeout(config.generation.timeout)
        .build();

    // Print banner
    println!("XEarthLayer Passthrough Mount v{}", xearthlayer::VERSION);
    println!("================================");
    println!();
    println!("Source:     {}", args.source);
    println!("Mountpoint: {}", mountpoint);
    println!("DDS Format: {:?}", texture_config.format());
    println!();

    let service = runner.create_service(service_config, &provider_config)?;

    // Print service info
    if service.cache_enabled() {
        println!(
            "Cache: Enabled ({} memory, {} disk)",
            xearthlayer::config::format_size(config.cache.memory_size),
            xearthlayer::config::format_size(config.cache.disk_size)
        );
    } else {
        println!("Cache: Disabled (all tiles generated fresh)");
    }
    println!("Provider: {}", service.provider_name());
    println!();

    let fuse_backend = if args.fuse3 {
        "fuse3 (async multi-threaded)"
    } else {
        "fuser (legacy single-threaded)"
    };
    println!("Mounting passthrough filesystem...");
    println!("  Backend:    {}", fuse_backend);
    println!("  Real files: Passed through from source");
    println!("  DDS files:  Generated on-demand");
    println!();
    println!("Press Ctrl+C to unmount and exit");
    println!();

    // Set up signal handler for graceful shutdown
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    ctrlc::set_handler(move || {
        println!();
        println!("Received shutdown signal, unmounting...");
        shutdown_clone.store(true, Ordering::SeqCst);
    })
    .map_err(|e| CliError::Config(format!("Failed to set signal handler: {}", e)))?;

    // Start serving with passthrough
    if args.fuse3 {
        // Use fuse3 async multi-threaded backend
        // This runs synchronously but all FUSE operations are async internally
        service
            .serve_passthrough_fuse3_blocking(&args.source, &mountpoint)
            .map_err(CliError::Serve)?;

        // fuse3 blocks until unmounted, so we get here after unmount
        println!();
        println!("Filesystem unmounted.");
        return Ok(());
    }

    // Start serving with passthrough in background (legacy fuser backend)
    // The BackgroundSession auto-unmounts when dropped
    let _session = service
        .serve_passthrough_background(&args.source, &mountpoint)
        .map_err(CliError::Serve)?;

    // Wait for shutdown signal, displaying telemetry periodically
    let mut last_telemetry = std::time::Instant::now();
    let telemetry_interval = std::time::Duration::from_secs(30);

    while !shutdown.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Display telemetry every 30 seconds
        if last_telemetry.elapsed() >= telemetry_interval {
            let snapshot = service.telemetry_snapshot();
            if snapshot.jobs_completed > 0 || snapshot.jobs_active > 0 {
                println!(
                    "[{}] Tiles: {} completed, {} active | Throughput: {} | Coalescing: {:.0}%",
                    snapshot.uptime_human(),
                    snapshot.jobs_completed,
                    snapshot.jobs_active,
                    snapshot.throughput_human(),
                    snapshot.coalescing_rate() * 100.0
                );
            }
            last_telemetry = std::time::Instant::now();
        }
    }

    // Print final telemetry summary
    let final_snapshot = service.telemetry_snapshot();
    if final_snapshot.jobs_completed > 0 {
        println!();
        println!("Session Summary");
        println!("───────────────");
        println!(
            "  Tiles generated: {} ({} failed)",
            final_snapshot.jobs_completed, final_snapshot.jobs_failed
        );
        println!(
            "  Tiles coalesced: {} ({:.0}% savings)",
            final_snapshot.jobs_coalesced,
            final_snapshot.coalescing_rate() * 100.0
        );
        println!(
            "  Data downloaded: {}",
            final_snapshot.bytes_downloaded_human()
        );
        println!("  Avg throughput:  {}", final_snapshot.throughput_human());
        println!("  Uptime: {}", final_snapshot.uptime_human());
    }

    // Session drops here, triggering unmount
    println!();
    println!("Filesystem unmounted.");
    Ok(())
}
