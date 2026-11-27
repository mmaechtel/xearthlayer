//! The benchmark dataset: where chunks live on disk, the manifest that
//! describes them, fetching them from XEL's providers, and loading a tile.
//!
//! Layout under the data directory (gitignored):
//!
//! ```text
//! <provider>/manifest.json
//! <provider>/<location>/z16/<row>_<col>.jpg
//! <provider>/<location>/z18/<row>_<col>.jpg
//! ```

use crate::jpeg_info::{self, JpegInfo};
use crate::synth::{self, Content};
use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use xearthlayer::dds::MipmapGenerator;
use xearthlayer::provider::{Provider, ProviderConfig, ProviderFactory, ReqwestClient};

/// Width and height of a chunk in pixels.
pub const CHUNK_PX: u32 = 256;
/// Zoom of the chunks XEL downloads for the tiles under test.
pub const SOURCE_ZOOM: u8 = 16;
/// Zoom of the ground truth chunks.
pub const TRUTH_ZOOM: u8 = 18;
/// ZL18 chunks per ZL16 chunk, per side.
pub const TRUTH_SCALE: u32 = 4;

/// A square block of chunks at one zoom.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Grid {
    pub zoom: u8,
    pub row0: u32,
    pub col0: u32,
    pub side: u32,
}

impl Grid {
    fn chunks(self) -> impl Iterator<Item = (u32, u32)> {
        (0..self.side)
            .flat_map(move |r| (0..self.side).map(move |c| (self.row0 + r, self.col0 + c)))
    }
}

/// How the chunks of one grid were compressed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JpegSummary {
    /// Header of the first chunk.
    pub first_chunk: JpegInfo,
    /// Whether every chunk has the first chunk's tables and subsampling.
    pub uniform: bool,
    pub total_bytes: u64,
}

/// One benchmark tile: a provider at a location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileEntry {
    pub provider: String,
    pub location: String,
    pub content: String,
    pub synthetic: bool,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub url_pattern: String,
    pub source: Grid,
    pub truth: Grid,
    pub source_jpeg: Option<JpegSummary>,
    pub truth_jpeg: Option<JpegSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub note: String,
    pub tiles: Vec<TileEntry>,
}

/// A real location to fetch.
pub struct Location {
    pub name: &'static str,
    pub content: Content,
    pub lat: f64,
    pub lon: f64,
    /// Inside the United States, so the US-only USGS provider covers it.
    pub us: bool,
}

const fn loc(name: &'static str, content: Content, lat: f64, lon: f64, us: bool) -> Location {
    Location {
        name,
        content,
        lat,
        lon,
        us,
    }
}

/// Twelve locations over the five content types, half of them in the US.
pub const LOCATIONS: [Location; 12] = [
    loc("town-munich", Content::Town, 48.160, 11.580, false),
    loc("town-denver", Content::Town, 39.700, -104.940, true),
    loc("town-lyon", Content::Town, 45.760, 4.850, false),
    loc("forest-schwarzwald", Content::Forest, 48.550, 8.250, false),
    loc("forest-olympic", Content::Forest, 47.600, -123.600, true),
    loc("farmland-iowa", Content::Farmland, 42.000, -93.500, true),
    loc("farmland-po", Content::Farmland, 45.100, 10.300, false),
    loc("farmland-kansas", Content::Farmland, 37.800, -100.800, true),
    loc("coast-bigsur", Content::Coast, 36.270, -121.810, true),
    loc("coast-bretagne", Content::Coast, 48.640, -4.560, false),
    loc(
        "mountains-zermatt",
        Content::Mountains,
        45.980,
        7.700,
        false,
    ),
    loc(
        "mountains-sawatch",
        Content::Mountains,
        39.110,
        -106.450,
        true,
    ),
];

/// A provider the benchmark can fetch without credentials.
pub struct ProviderSpec {
    pub name: &'static str,
    pub config: fn() -> ProviderConfig,
    /// The URL pattern XEL's provider module builds, for the manifest.
    pub url_pattern: &'static str,
    pub us_only: bool,
}

pub const PROVIDERS: [ProviderSpec; 5] = [
    ProviderSpec {
        name: "bing",
        config: ProviderConfig::bing,
        url_pattern: "https://ecn.t0.tiles.virtualearth.net/tiles/a{quadkey}.jpeg?g=1",
        us_only: false,
    },
    ProviderSpec {
        name: "go2",
        config: ProviderConfig::go2,
        url_pattern: "http://mt{0-3}.google.com/vt/lyrs=s&x={col}&y={row}&z={zoom}",
        us_only: false,
    },
    ProviderSpec {
        name: "arcgis",
        config: ProviderConfig::arcgis,
        url_pattern:
            "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{zoom}/{row}/{col}",
        us_only: false,
    },
    ProviderSpec {
        name: "apple",
        config: ProviderConfig::apple,
        url_pattern:
            "https://sat-cdn.apple-mapkit.com/tile?style=7&size=1&scale=1&z={zoom}&x={col}&y={row}&v={version}&accessKey={token}",
        us_only: false,
    },
    ProviderSpec {
        name: "usgs",
        config: ProviderConfig::usgs,
        url_pattern:
            "https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer/tile/{zoom}/{row}/{col}",
        us_only: true,
    },
];

/// Web Mercator chunk (row, col) containing a coordinate.
pub fn chunk_at(lat: f64, lon: f64, zoom: u8) -> (u32, u32) {
    let n = f64::from(1u32 << zoom);
    let col = ((lon + 180.0) / 360.0 * n).floor();
    let row = ((1.0 - lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0 * n).floor();
    (row as u32, col as u32)
}

/// The source grid centred on a location and the truth grid covering
/// exactly the same ground.
pub fn grids_at(lat: f64, lon: f64, side: u32) -> (Grid, Grid) {
    let (row, col) = chunk_at(lat, lon, SOURCE_ZOOM);
    let source = Grid {
        zoom: SOURCE_ZOOM,
        row0: row - side / 2,
        col0: col - side / 2,
        side,
    };
    (source, truth_grid(source))
}

fn truth_grid(source: Grid) -> Grid {
    Grid {
        zoom: TRUTH_ZOOM,
        row0: source.row0 * TRUTH_SCALE,
        col0: source.col0 * TRUTH_SCALE,
        side: source.side * TRUTH_SCALE,
    }
}

fn chunk_path(
    data: &Path,
    provider: &str,
    location: &str,
    zoom: u8,
    row: u32,
    col: u32,
) -> PathBuf {
    data.join(provider)
        .join(location)
        .join(format!("z{zoom}"))
        .join(format!("{row}_{col}.jpg"))
}

fn write_manifest(data: &Path, provider: &str, manifest: &Manifest) -> Result<(), String> {
    let path = data.join(provider).join("manifest.json");
    let json = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))
}

/// Every tile of every provider manifest under `data`, in a stable order.
pub fn read_manifests(data: &Path) -> Result<Vec<TileEntry>, String> {
    let mut providers: Vec<PathBuf> = fs::read_dir(data)
        .map_err(|e| format!("{}: {e}", data.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path().join("manifest.json")))
        .filter(|path| path.is_file())
        .collect();
    providers.sort();
    let mut tiles = Vec::new();
    for path in providers {
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let manifest: Manifest =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        tiles.extend(manifest.tiles);
    }
    Ok(tiles)
}

fn summarise(data: &Path, provider: &str, location: &str, grid: Grid) -> Option<JpegSummary> {
    let mut first: Option<JpegInfo> = None;
    let mut uniform = true;
    let mut total_bytes = 0;
    for (row, col) in grid.chunks() {
        let bytes = fs::read(chunk_path(data, provider, location, grid.zoom, row, col)).ok()?;
        total_bytes += bytes.len() as u64;
        let info = jpeg_info::parse(&bytes)?;
        match &first {
            None => first = Some(info),
            Some(f) => uniform &= *f == info,
        }
    }
    Some(JpegSummary {
        first_chunk: first?,
        uniform,
        total_bytes,
    })
}

/// Tries one ZL16 chunk from every provider and reports the outcome.
/// Returns the names of the providers that answered.
pub fn probe() -> Vec<&'static str> {
    let mut reachable = Vec::new();
    for spec in &PROVIDERS {
        let location = LOCATIONS.iter().find(|l| l.us).expect("a US location");
        let (row, col) = chunk_at(location.lat, location.lon, SOURCE_ZOOM);
        match connect(spec).and_then(|p| {
            p.download_chunk(row, col, SOURCE_ZOOM)
                .map_err(|e| e.to_string())
        }) {
            Ok(bytes) => match jpeg_info::parse(&bytes) {
                Some(info) => {
                    println!(
                        "{:8} ok   {} bytes, JPEG quality ~{}, {}",
                        spec.name,
                        bytes.len(),
                        info.quality_estimate,
                        info.subsampling
                    );
                    reachable.push(spec.name);
                }
                None => println!(
                    "{:8} FAIL answered {} bytes that are not a JPEG",
                    spec.name,
                    bytes.len()
                ),
            },
            Err(e) => println!("{:8} FAIL {e}", spec.name),
        }
    }
    reachable
}

fn connect(spec: &ProviderSpec) -> Result<Arc<dyn Provider>, String> {
    let client = ReqwestClient::new().map_err(|e| e.to_string())?;
    ProviderFactory::new(client)
        .create(&(spec.config)())
        .map(|(provider, _, _)| provider)
        .map_err(|e| e.to_string())
}

/// Downloads the source and truth chunks of every location from the named
/// providers through XEL's provider modules. Chunks already on disk are
/// kept, so an interrupted fetch resumes. A tile with a chunk that could
/// not be downloaded is left out of the manifest.
pub fn fetch(data: &Path, providers: &[String], side: u32) -> Result<(), String> {
    for spec in PROVIDERS
        .iter()
        .filter(|s| providers.iter().any(|p| p == s.name))
    {
        let provider = match connect(spec) {
            Ok(provider) => provider,
            Err(e) => {
                println!("{}: cannot start provider: {e}", spec.name);
                continue;
            }
        };
        let mut tiles = Vec::new();
        for location in LOCATIONS.iter().filter(|l| l.us || !spec.us_only) {
            let (source, truth) = grids_at(location.lat, location.lon, side);
            let wanted: Vec<(u8, u32, u32)> = [source, truth]
                .iter()
                .flat_map(|g| g.chunks().map(move |(r, c)| (g.zoom, r, c)))
                .collect();
            let failures: Vec<String> = wanted
                .par_iter()
                .filter_map(|&(zoom, row, col)| {
                    let path = chunk_path(data, spec.name, location.name, zoom, row, col);
                    if path.is_file() {
                        return None;
                    }
                    let result = provider
                        .download_chunk(row, col, zoom)
                        .map_err(|e| e.to_string())
                        .and_then(|bytes| {
                            jpeg_info::parse(&bytes).ok_or("not a JPEG".to_string())?;
                            fs::create_dir_all(path.parent().expect("chunk has a folder"))
                                .and_then(|()| fs::write(&path, bytes))
                                .map_err(|e| e.to_string())
                        });
                    result.err().map(|e| format!("z{zoom} {row}/{col}: {e}"))
                })
                .collect();
            if let Some(first) = failures.first() {
                println!(
                    "{} {}: {} of {} chunks failed (first: {first}), tile skipped",
                    spec.name,
                    location.name,
                    failures.len(),
                    wanted.len()
                );
                continue;
            }
            println!("{} {}: {} chunks", spec.name, location.name, wanted.len());
            tiles.push(TileEntry {
                provider: spec.name.to_string(),
                location: location.name.to_string(),
                content: location.content.name().to_string(),
                synthetic: false,
                lat: Some(location.lat),
                lon: Some(location.lon),
                url_pattern: spec.url_pattern.to_string(),
                source,
                truth,
                source_jpeg: summarise(data, spec.name, location.name, source),
                truth_jpeg: summarise(data, spec.name, location.name, truth),
            });
        }
        if !tiles.is_empty() {
            let note = "Fetched through xearthlayer::provider. Row and column are Web Mercator chunk coordinates.";
            write_manifest(
                data,
                spec.name,
                &Manifest {
                    note: note.to_string(),
                    tiles,
                },
            )?;
        }
    }
    Ok(())
}

/// Writes the synthetic dataset: every scene of [`synth::SCENES`] for each
/// profile, as a provider named by [`synth::Profile::name`].
pub fn write_synthetic(data: &Path, profiles: &[synth::Profile], side: u32) -> Result<(), String> {
    write_scenes(data, profiles, side, &synth::SCENES)
}

/// [`write_synthetic`] for a chosen set of scenes.
fn write_scenes(
    data: &Path,
    profiles: &[synth::Profile],
    side: u32,
    scenes: &[(&str, Content, u32)],
) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    for profile in profiles {
        let provider = profile.name();
        let mut tiles = Vec::new();
        for (index, (name, content, seed)) in scenes.iter().enumerate() {
            let source = Grid {
                zoom: SOURCE_ZOOM,
                row0: 1000 + index as u32 * 100,
                col0: 2000,
                side,
            };
            let truth = truth_grid(source);
            let scene = synth::render(*content, *seed, truth.side * CHUNK_PX);
            let small = synth::soften(&synth::provider_downsample_4x(&scene), profile.sigma_tenths);
            for (grid, image) in [(source, &small), (truth, &scene)] {
                let chunks: Vec<(u32, u32)> = grid.chunks().collect();
                chunks.par_iter().try_for_each(|&(row, col)| {
                    let path = chunk_path(data, &provider, name, grid.zoom, row, col);
                    fs::create_dir_all(path.parent().expect("chunk has a folder")).map_err(io)?;
                    let quality = profile.quality_at(row, col);
                    let bytes =
                        synth::encode_chunk(image, col - grid.col0, row - grid.row0, quality);
                    fs::write(path, bytes).map_err(io)
                })?;
            }
            println!("{provider} {name}");
            tiles.push(TileEntry {
                provider: provider.clone(),
                location: (*name).to_string(),
                content: content.name().to_string(),
                synthetic: true,
                lat: None,
                lon: None,
                url_pattern: format!("synthetic scene, seed {seed}, {}", profile.describe()),
                source,
                truth,
                source_jpeg: summarise(data, &provider, name, source),
                truth_jpeg: summarise(data, &provider, name, truth),
            });
        }
        let note =
            "SYNTHETIC. Procedural scenes, not provider imagery. Rows and columns are arbitrary.";
        write_manifest(
            data,
            &provider,
            &Manifest {
                note: note.to_string(),
                tiles,
            },
        )?;
    }
    Ok(())
}

/// A tile ready for the benchmark.
pub struct LoadedTile {
    /// The ZL16 chunks assembled as XEL assembles a tile.
    pub source: RgbaImage,
    /// The ZL18 chunks assembled and reduced 4x in linear light.
    pub truth: RgbaImage,
    /// The JPEG header of every source chunk, row-major.
    pub chunk_info: Vec<Option<JpegInfo>>,
}

fn assemble(
    data: &Path,
    entry: &TileEntry,
    grid: Grid,
) -> Result<(RgbaImage, Vec<Option<JpegInfo>>), String> {
    let mut image = RgbaImage::new(grid.side * CHUNK_PX, grid.side * CHUNK_PX);
    let mut infos = Vec::new();
    for (row, col) in grid.chunks() {
        let path = chunk_path(data, &entry.provider, &entry.location, grid.zoom, row, col);
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        // The `image` decoder is what `tasks::chunk_image::decode` runs, and
        // `decode_into` (the assembly fast path) is tested to give the same
        // bytes, so this is XEL's decode: zune-jpeg with `image`'s options.
        let chunk = image::load_from_memory(&bytes)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .into_rgba8();
        if chunk.dimensions() != (CHUNK_PX, CHUNK_PX) {
            return Err(format!("{}: not a 256 pixel chunk", path.display()));
        }
        let (x, y) = ((col - grid.col0) * CHUNK_PX, (row - grid.row0) * CHUNK_PX);
        image::imageops::replace(&mut image, &chunk, i64::from(x), i64::from(y));
        infos.push(jpeg_info::parse(&bytes));
    }
    Ok((image, infos))
}

/// Loads a tile: decodes and assembles both grids and reduces the truth to
/// the source's resolution with the mipmap filter of #458 (two 2x2
/// linear-light averages).
pub fn load(data: &Path, entry: &TileEntry) -> Result<LoadedTile, String> {
    let (source, chunk_info) = assemble(data, entry, entry.source)?;
    let (truth_full, _) = assemble(data, entry, entry.truth)?;
    let truth = MipmapGenerator::generate_chain_with_count(&truth_full, 3)
        .pop()
        .ok_or("empty mipmap chain")?;
    if truth.dimensions() != source.dimensions() {
        return Err("truth and source cover different areas".to_string());
    }
    Ok(LoadedTile {
        source,
        truth,
        chunk_info,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_coordinates_match_the_slippy_map_definition() {
        // Munich at ZL16.
        assert_eq!(chunk_at(48.137, 11.575, 16), (22743, 34875));
        assert_eq!(chunk_at(0.0, 0.0, 1), (1, 1));
    }

    #[test]
    fn the_truth_grid_covers_the_source_grid() {
        let (source, truth) = grids_at(48.137, 11.575, 4);
        assert_eq!((source.row0, source.col0, source.side), (22741, 34873, 4));
        assert_eq!(
            (truth.zoom, truth.row0, truth.col0, truth.side),
            (18, 90964, 139492, 16)
        );
    }

    #[test]
    fn a_synthetic_tile_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("level0-bench-test-{}", std::process::id()));
        let profile = synth::Profile {
            quality: synth::Quality::Fixed(75),
            sigma_tenths: 0,
        };
        write_synthetic(&dir, &[profile], 1).unwrap();
        let tiles = read_manifests(&dir).unwrap();
        assert_eq!(tiles.len(), synth::SCENES.len());
        let summary = tiles[0].source_jpeg.as_ref().unwrap();
        assert_eq!(summary.first_chunk.quality_estimate, 75);
        let tile = load(&dir, &tiles[0]).unwrap();
        assert_eq!(tile.source.dimensions(), (256, 256));
        assert_eq!(tile.truth.dimensions(), (256, 256));
        assert_eq!(tile.chunk_info.len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_mixed_provider_has_different_tables_inside_one_tile() {
        let dir = std::env::temp_dir().join(format!("level0-bench-mixed-{}", std::process::id()));
        let profile = synth::Profile {
            quality: synth::Quality::Mixed,
            sigma_tenths: 5,
        };
        write_scenes(&dir, &[profile], 2, &synth::SCENES[..1]).unwrap();
        let tiles = read_manifests(&dir).unwrap();
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].provider, "synth-mixed-s05");
        assert!(tiles[0].url_pattern.contains("sigma 0.5 px"));
        assert!(!tiles[0].source_jpeg.as_ref().unwrap().uniform);
        assert!(!tiles[0].truth_jpeg.as_ref().unwrap().uniform);
        let tile = load(&dir, &tiles[0]).unwrap();
        let mut qualities: Vec<u8> = tile
            .chunk_info
            .iter()
            .map(|i| i.as_ref().unwrap().quality_estimate)
            .collect();
        qualities.sort_unstable();
        qualities.dedup();
        assert_eq!(qualities, synth::MIXED_QUALITIES);
        fs::remove_dir_all(dir).unwrap();
    }
}
