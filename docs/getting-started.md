# Getting Started with XEarthLayer

This guide will help you get XEarthLayer up and running with X-Plane. By the end, you'll have satellite imagery scenery installed and streaming in your simulator.

## Prerequisites

- **X-Plane 12** (X-Plane 11 may work but is untested)
- **Linux** (Windows and macOS support planned)
- Basic familiarity with the command line

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/youruser/xearthlayer.git
cd xearthlayer

# Build the release binary
make release

# The binary is at target/release/xearthlayer
```

### From Binary Release

Download the latest release from the releases page and extract to a location of your choice.

## Initial Setup

### 1. Create Configuration File

Run the init command to create your configuration file:

```bash
xearthlayer init
```

This creates `~/.xearthlayer/config.ini` with sensible defaults. The command will attempt to auto-detect your X-Plane installation.

### 2. Configure Your Settings

Edit `~/.xearthlayer/config.ini` to set your preferences:

```ini
[xplane]
# Path to your X-Plane Custom Scenery folder
scenery_dir = /path/to/X-Plane 12/Custom Scenery

[packages]
# URL to a package library (get this from your scenery provider)
library_url = https://example.com/xearthlayer_package_library.txt
```

See the [Configuration Guide](configuration.md) for all available options.

## Installing Scenery Packages

XEarthLayer uses scenery packages - pre-built collections of satellite imagery for specific regions.

### 1. Check Available Packages

```bash
xearthlayer packages check
```

This connects to your configured library and shows available packages:

```
Checking for package updates...

  EU-PARIS (ortho) v1.0.0 - Not installed
  NA-SOCAL (ortho) v2.1.0 - Not installed

2 package(s) available for installation.
```

### 2. Install a Package

```bash
xearthlayer packages install eu-paris
```

The package manager will:
1. Download the package archive
2. Verify checksums
3. Extract to your Custom Scenery folder

```
Installing eu-paris (ortho)...

Fetching library index...
Fetching package metadata...
Package: EU-PARIS v1.0.0
Parts: 1 (zzXEL_eu-paris_ortho-1.0.0.tar.gz)

Success: Installed EU-PARIS (ortho) v1.0.0
Downloaded 1.2 GB, extracted 4521 files
```

### 3. Verify Installation

```bash
xearthlayer packages list
```

```
Installed Packages (1)
======================

  EU-PARIS (ortho) v1.0.0 - 1.2 GB
```

## Running the Streaming Service

For regions without pre-built packages, or if you prefer on-demand imagery, XEarthLayer can stream satellite tiles in real-time.

### 1. Start the Service

```bash
xearthlayer start --source /path/to/scenery/folder
```

This creates a virtual filesystem that overlays your existing scenery, generating DDS textures on-demand as X-Plane requests them.

### 2. Configure X-Plane

Add the mount point to your `scenery_packs.ini` or use X-Plane's scenery settings to enable the virtual scenery folder.

### 3. Fly!

Load a flight in the covered region. The first time you visit an area, there may be a brief delay as tiles are downloaded and encoded. Subsequent visits will be instant due to caching.

## Updating Packages

Check for and install updates:

```bash
# Check what's available
xearthlayer packages check

# Update a specific package
xearthlayer packages update eu-paris

# Update all packages
xearthlayer packages update --all
```

## Next Steps

- [Configuration Guide](configuration.md) - Customize cache sizes, providers, and more
- [Package Management](package-management.md) - Detailed package operations
- [Running the Service](running-service.md) - Advanced streaming options
- [Content Publishing](content-publishing.md) - Create and share your own packages
