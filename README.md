# XEarthLayer

A high-performance Rust implementation for streaming satellite imagery to X-Plane flight simulator.

## Overview

XEarthLayer provides on-demand satellite imagery streaming for X-Plane using a FUSE virtual filesystem. Instead of downloading massive scenery packages, it dynamically fetches only the imagery you need as you fly, from multiple satellite providers (Bing Maps, NAIP, EOX Sentinel-2, USGS).

## Current Implementation Status

### Implemented Modules

#### 1. Coordinate System (`xearthlayer/src/coord/`)
- Geographic to tile coordinate conversion (Web Mercator projection)
- Tile to chunk coordinate mapping (16×16 chunks per tile)
- Bing Maps quadkey encoding/decoding
- **48 comprehensive tests** including property-based testing

#### 2. Provider Abstraction (`xearthlayer/src/provider/`)
- `Provider` trait for satellite imagery sources
- `HttpClient` trait for dependency injection and testing
- Bing Maps provider implementation with quadkey-based URLs
- Mock support for testing without network calls
- **10 tests** covering provider behavior

#### 3. Download Orchestrator (`xearthlayer/src/orchestrator/`)
- Parallel downloading of 256 chunks per tile (16×16 grid)
- Batched threading with configurable parallelism (default: 32 concurrent)
- Per-chunk retry logic with overall timeout
- Error tolerance (requires 80% success rate minimum)
- Image assembly into 4096×4096 RGBA images
- **3 integration tests** including real network downloads

#### 4. Command-Line Interface (`xearthlayer-cli/`)
- Download satellite imagery tiles to disk for testing
- JPEG encoding with 90% quality
- Automatic zoom level validation
- Successfully downloads real imagery in ~2.5 seconds

**Total Test Coverage**: 61 passing tests

### Modules Not Yet Implemented

The following modules from the AutoOrtho architecture remain to be implemented:

1. **FUSE Virtual Filesystem**
   - File operation handlers (getattr, read, readdir)
   - DDS/KTX2 filename pattern matching
   - Integration with X-Plane scenery system

2. **Tile Cache Manager**
   - In-memory LRU cache (1-2GB limit)
   - Disk cache for JPEG chunks (30GB limit)
   - Cache eviction and cleanup strategies

3. **DDS Texture Compression**
   - DirectX texture format encoding (BC1/BC3)
   - 5-level mipmap chain generation
   - Progressive loading optimization

4. **Configuration Management**
   - INI-based configuration from `~/.autoortho`
   - X-Plane scenery path auto-detection
   - Provider and cache settings

5. **Additional Imagery Providers**
   - NAIP (National Agriculture Imagery Program)
   - EOX Sentinel-2 satellite imagery
   - USGS imagery sources
   - Provider fallback logic

6. **Scenery Package Management**
   - Download pre-built scenery packages from GitHub
   - Package installation and mounting

7. **Flight Data Integration**
   - UDP listener for X-Plane position data (port 49000)
   - Predictive tile preloading based on flight path

8. **Web UI Dashboard**
   - Flask/SocketIO server (port 5000)
   - Real-time download statistics
   - Cache management interface

## Getting Started

### Development Setup

```bash
# Initialize development environment
make init

# Run all tests
make test

# Format and verify code
make verify

# Build release binary
cargo build --release
```

See `make help` for all available commands.

### Testing the CLI

Download a satellite imagery tile for any location:

```bash
# Build the CLI
cargo build --release

# Download NYC area at zoom 15 (maximum for Bing Maps)
./target/release/xearthlayer \
  --lat 40.7128 \
  --lon -74.0060 \
  --zoom 15 \
  --output nyc_tile.jpg

# Download San Francisco
./target/release/xearthlayer \
  --lat 37.7749 \
  --lon -122.4194 \
  --zoom 15 \
  --output sf_tile.jpg
```

**Note**: The maximum usable zoom level is 15 for Bing Maps because tiles at zoom Z require chunks from zoom Z+4, and Bing's maximum zoom is 19 (15+4=19).

### CLI Options

```
--lat <LAT>        Latitude in decimal degrees (required)
--lon <LON>        Longitude in decimal degrees (required)
--zoom <ZOOM>      Zoom level 1-15 (default: 15)
--output <PATH>    Output JPEG file path (required)
```

### Output Format

The CLI generates 4096×4096 pixel JPEG images at 90% quality by:
1. Converting lat/lon to tile coordinates at the requested zoom level
2. Downloading 256 chunks (16×16 grid) at zoom+4 from Bing Maps
3. Assembling chunks into a single high-resolution image
4. Encoding as JPEG with optimized quality settings

Typical output size: 4-6 MB per tile

## Credits

This project is architecturally influenced by [AutoOrtho](https://github.com/kubilus1/autoortho) by [kubilus1](https://github.com/kubilus1). XEarthLayer is an independent Rust implementation focused on performance, memory safety, and cross-platform compatibility.

## License

Licensed under the MIT License. See [LICENSE](LICENSE) for details.
