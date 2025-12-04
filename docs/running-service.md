# Running the Streaming Service

XEarthLayer can stream satellite imagery on-demand, generating DDS textures in real-time as X-Plane requests them. This is useful for:

- Regions without pre-built packages
- Testing new areas before committing to a full download
- Situations where disk space is limited

## How It Works

XEarthLayer creates a virtual filesystem (using FUSE) that overlays your existing scenery folder. When X-Plane requests a texture file:

1. XEarthLayer intercepts the request
2. Downloads satellite imagery tiles from the configured provider
3. Encodes them into DDS format
4. Returns the texture to X-Plane

The result is cached so subsequent requests are instant.

## Prerequisites

- **Linux** with FUSE support (most distributions have this built-in)
- **Existing scenery** with terrain definitions (DSF files) but missing textures

## Basic Usage

Start the streaming service:

```bash
xearthlayer start --source /path/to/scenery
```

This creates a mount point at `/path/to/scenery_xel` that X-Plane should use instead of the original folder.

### Options

| Option | Description |
|--------|-------------|
| `--source <PATH>` | Source scenery folder to overlay |
| `--mount <PATH>` | Custom mount point (default: `<source>_xel`) |
| `--provider <TYPE>` | Imagery provider: `bing`, `go2`, `google` |
| `--dds-format <FMT>` | Texture format: `bc1` or `bc3` |
| `--no-cache` | Disable caching (not recommended) |

### Example

```bash
xearthlayer start \
  --source "/home/user/X-Plane 12/Custom Scenery/Ortho4XP_Europe" \
  --provider bing
```

Output:
```
XEarthLayer Streaming Service
=============================

Source:    /home/user/X-Plane 12/Custom Scenery/Ortho4XP_Europe
Mount:     /home/user/X-Plane 12/Custom Scenery/Ortho4XP_Europe_xel
Provider:  Bing Maps
Format:    BC1 (DXT1)
Cache:     ~/.cache/xearthlayer

Service started. Press Ctrl+C to stop.
```

## X-Plane Configuration

### Using the Mount Point

Configure X-Plane to use the `_xel` mount point instead of the original scenery folder:

1. Edit `X-Plane 12/Custom Scenery/scenery_packs.ini`
2. Change the path from the original folder to the mount point
3. Restart X-Plane

Example `scenery_packs.ini` change:
```
# Before
SCENERY_PACK Custom Scenery/Ortho4XP_Europe/

# After
SCENERY_PACK Custom Scenery/Ortho4XP_Europe_xel/
```

### Scenery Loading Order

Ensure the streaming scenery loads at the correct priority. Orthophoto scenery should typically load last (lowest priority) so that airports, landmarks, and overlays appear on top.

## Stopping the Service

Press `Ctrl+C` to stop the service cleanly:

```
^C
Unmounting...
Service stopped.
```

Or from another terminal:

```bash
xearthlayer stop --mount /path/to/mount_xel
```

**Important:** Always stop the service properly before:
- Shutting down
- Moving or deleting the source folder
- Running X-Plane maintenance

## Performance

### First Visit

The first time you fly over an area, textures are downloaded and encoded on-demand. Expect:

- **Good network**: 1-2 seconds per tile
- **Poor network**: May see magenta placeholder tiles temporarily

### Cached Tiles

Subsequent visits load instantly from cache:

- **Memory cache**: ~10ms
- **Disk cache**: ~50-100ms

### Parallel Processing

XEarthLayer downloads and encodes multiple tiles in parallel:

- Default: 8 worker threads
- Each tile: 256 chunk downloads

Configure in `~/.xearthlayer/config.ini`:

```ini
[generation]
threads = 8

[download]
parallel = 32
```

## Caching

### Cache Locations

| Cache | Location | Default Size |
|-------|----------|--------------|
| Memory | RAM | 2 GB |
| Disk | `~/.cache/xearthlayer/` | 20 GB |

### Cache Structure

```
~/.cache/xearthlayer/
├── Bing Maps/
│   └── <zoom>/<row>/<col>_bc1.dds
└── Google Maps/
    └── <zoom>/<row>/<col>_bc1.dds
```

### Managing Cache

View cache statistics:

```bash
xearthlayer cache stats
```

Clear the cache:

```bash
xearthlayer cache clear
```

Clear only disk cache (preserve memory):

```bash
xearthlayer cache clear --disk
```

## Imagery Providers

### Bing Maps (Default)

```bash
xearthlayer start --source /path/to/scenery --provider bing
```

- Free, no API key required
- Good global coverage
- Recommended for most users

### Google GO2

```bash
xearthlayer start --source /path/to/scenery --provider go2
```

- Free, no API key required
- Same imagery as Ortho4XP's GO2 provider
- Best compatibility with Ortho4XP-generated scenery

### Google Maps API

```bash
xearthlayer start --source /path/to/scenery \
  --provider google \
  --google-api-key YOUR_API_KEY
```

- Requires paid API key
- Rate limited (15,000 requests/day)
- Not recommended for regular use

## Timeout and Placeholders

If a tile takes too long to generate (network issues, server problems), XEarthLayer returns a magenta placeholder texture to prevent X-Plane from hanging.

Configure timeout in config:

```ini
[generation]
timeout = 10  # seconds
```

Placeholder tiles are not cached and will be retried on next request.

## Running as a Background Service

### Using systemd

Create `/etc/systemd/user/xearthlayer.service`:

```ini
[Unit]
Description=XEarthLayer Streaming Service
After=network.target

[Service]
Type=simple
ExecStart=/path/to/xearthlayer start --source /path/to/scenery
ExecStop=/path/to/xearthlayer stop --mount /path/to/scenery_xel
Restart=on-failure

[Install]
WantedBy=default.target
```

Enable and start:

```bash
systemctl --user enable xearthlayer
systemctl --user start xearthlayer
```

### Auto-start on Login

Add to your shell profile (`~/.bashrc` or `~/.zshrc`):

```bash
# Start XEarthLayer if not already running
if ! mountpoint -q "/path/to/scenery_xel" 2>/dev/null; then
    xearthlayer start --source /path/to/scenery &
fi
```

## Logging

View logs in real-time:

```bash
tail -f ~/.xearthlayer/xearthlayer.log
```

Log location can be configured:

```ini
[logging]
file = ~/.xearthlayer/xearthlayer.log
```

## Common Issues

### Mount point busy

```
Error: Mount point is busy or already mounted
```

Solution: Stop any existing XEarthLayer instance or unmount manually:

```bash
fusermount -u /path/to/mount_xel
```

### Permission denied

```
Error: Permission denied when creating mount
```

Solution: Ensure you have FUSE permissions:

```bash
# Add yourself to the fuse group
sudo usermod -aG fuse $USER
# Log out and back in
```

### X-Plane doesn't see the scenery

Ensure:
1. The mount point is in your `scenery_packs.ini`
2. X-Plane was restarted after adding the entry
3. The service is running before starting X-Plane

### Magenta tiles everywhere

This indicates network or timeout issues:

1. Check your internet connection
2. Try a different provider
3. Increase the timeout in config
4. Check logs for specific errors
