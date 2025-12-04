# XEarthLayer

High-quality satellite imagery for X-Plane, on demand.

XEarthLayer provides two ways to get satellite scenery into X-Plane:

1. **Scenery Packages** - Download pre-built regional packages (easiest)
2. **Live Streaming** - Stream imagery in real-time as you fly

## Features

- Install scenery packages with a single command
- Stream satellite imagery on-demand from Bing Maps or Google Maps
- Two-tier caching (memory + disk) for instant subsequent loads
- High-quality DDS textures with mipmap chains
- Works with Ortho4XP-generated scenery
- Linux support (Windows and macOS planned)

## Quick Start

### Install from Source

```bash
git clone https://github.com/youruser/xearthlayer.git
cd xearthlayer
make release
```

### Initialize Configuration

```bash
xearthlayer init
```

This creates `~/.xearthlayer/config.ini` with sensible defaults.

### Install a Scenery Package

```bash
# Configure your package library
# Edit ~/.xearthlayer/config.ini and set:
# [packages]
# library_url = https://example.com/xearthlayer_package_library.txt

# Check available packages
xearthlayer packages check

# Install a package
xearthlayer packages install eu-paris
```

### Or Stream Live

```bash
xearthlayer start --source /path/to/scenery
```

## Documentation

### User Guides

| Guide | Description |
|-------|-------------|
| [Getting Started](docs/getting-started.md) | First-time setup and basic usage |
| [Configuration](docs/configuration.md) | All configuration options |
| [Package Management](docs/package-management.md) | Installing, updating, removing packages |
| [Running the Service](docs/running-service.md) | Live streaming mode |
| [Content Publishing](docs/content-publishing.md) | Create packages from Ortho4XP |

### Developer Documentation

| Document | Description |
|----------|-------------|
| [Design Principles](docs/dev/design-principles.md) | SOLID principles and TDD guidelines |
| [Architecture](docs/dev/architecture.md) | System overview and module dependencies |
| [Coordinate System](docs/dev/coordinate-system.md) | Web Mercator projection and tile math |
| [DDS Implementation](docs/dev/dds-implementation.md) | Texture compression details |
| [FUSE Filesystem](docs/dev/fuse-filesystem.md) | Virtual filesystem implementation |
| [Cache Design](docs/dev/cache-design.md) | Two-tier caching strategy |
| [Parallel Processing](docs/dev/parallel-processing.md) | Thread pool and request coalescing |

## CLI Reference

```bash
# Configuration
xearthlayer init                    # Create config file

# Package Management
xearthlayer packages check          # Check for available/updates
xearthlayer packages list           # List installed packages
xearthlayer packages install <region>   # Install a package
xearthlayer packages update [region]    # Update packages
xearthlayer packages remove <region>    # Remove a package
xearthlayer packages info <region>      # Package details

# Streaming Service
xearthlayer start --source <path>   # Start streaming service
xearthlayer cache stats             # View cache statistics
xearthlayer cache clear             # Clear cache

# Content Publishing
xearthlayer publish init            # Initialize package repository
xearthlayer publish scan --source <path>    # Scan Ortho4XP tiles
xearthlayer publish add --source <path> --region <code>  # Create package
xearthlayer publish build --region <code>   # Build archives
xearthlayer publish release --region <code> # Release to library
```

Run `xearthlayer --help` for all options.

## Requirements

- **X-Plane 12** (X-Plane 11 may work but is untested)
- **Linux** with FUSE support
- **Internet connection** for streaming or package downloads

## Contributing

### Development Setup

```bash
# Install Rust via rustup.rs
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/youruser/xearthlayer.git
cd xearthlayer
make init
make verify
```

### Code Guidelines

- Follow TDD (Test-Driven Development)
- Follow SOLID principles
- Run `make verify` before committing
- Maintain test coverage above 80%

See [Developer Documentation](docs/dev/) for architecture details.

## Credits

Architecturally influenced by [AutoOrtho](https://github.com/kubilus1/autoortho) by [kubilus1](https://github.com/kubilus1). XEarthLayer is an independent Rust implementation focused on performance and memory safety.

Developed with assistance from [Claude](https://claude.ai) by Anthropic.

## License

Licensed under the MIT License. See [LICENSE](LICENSE) for details.
