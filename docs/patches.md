# Tile Patches

Tile patches allow you to use custom mesh/elevation data from third-party airport addons while XEarthLayer generates consistent textures using your configured imagery provider.

## Overview

Many airport scenery addons (like SFD KLAX, X-Codr KDEN) include custom elevation data to ensure accurate airport terrain. When you build ortho tiles using their elevation data in Ortho4XP, you get a complete tile with:

- **DSF files** - Custom mesh with corrected elevation
- **Terrain files** - Texture coordinate definitions
- **Textures** (optional) - Pre-built DDS files

With tile patches, XEL mounts these tiles and **generates textures dynamically** using your configured provider (Bing, Google, etc.), ensuring consistent imagery across your entire scenery library.

## Quick Start

1. **Create the patches directory**:
   ```bash
   mkdir -p ~/.xearthlayer/patches
   ```

2. **Add your patch tiles**:
   ```bash
   mv ~/Ortho4XP/Tiles/+33-119/ ~/.xearthlayer/patches/KLAX_Mesh/
   ```

3. **Verify patches are detected**:
   ```bash
   xearthlayer patches list
   ```

4. **Start XEL** - Patches are mounted automatically:
   ```bash
   xearthlayer run
   ```

## Directory Structure

```
~/.xearthlayer/patches/
├── A_KDEN_Mesh/                      # 'A' prefix = highest priority
│   ├── Earth nav data/
│   │   └── +30-110/
│   │       └── +39-105.dsf           # Custom mesh DSF
│   └── terrain/
│       └── 24800_13648_USA_216.ter
├── B_KLAX_Mesh/                      # 'B' prefix = second priority
│   ├── Earth nav data/
│   │   └── +30-120/
│   │       └── +33-119.dsf
│   └── terrain/
│       └── 94800_47888_BI18.ter
└── SomeOther/                        # No prefix = lowest priority
    └── ...
```

### Required Structure

Each patch folder **must** contain:
- `Earth nav data/` directory with at least one `.dsf` file

Optional directories:
- `terrain/` - Terrain definition files
- `textures/` - Pre-built DDS textures (XEL ignores these and generates its own)

## Priority Order

When multiple patches contain the same file path, XEL uses **alphabetical folder name** for priority. The patch with the alphabetically-first name wins.

**Example**: If both `A_KLAX/` and `B_KLAX/` contain `+33-119.dsf`, the file from `A_KLAX/` is used.

To control priority, prefix your folder names:
- `A_` or `AA_` - Highest priority
- `B_` or `BA_` - Medium priority
- `Z_` or `ZZ_` - Lowest priority

## Creating Patch Tiles

### Step 1: Get Addon Patch Data

Airport addons typically include an `!Ortho4XP_Patch/` folder containing:
- **Elevation data**: `Elevations/+33-119.tif`
- **OSM patches**: `Patches/+30-120/+33-119/*.osm`

**Example locations**:
- SFD KLAX: `SFD_KLAX_Los_Angeles_HD_1_Airport/!Ortho4XP_Patch/`
- X-Codr KDEN: `KDEN Ortho/Z KDEN Mesh/`

### Step 2: Configure Ortho4XP

Build the tile in Ortho4XP using the addon's elevation data:

1. Copy elevation TIF to Ortho4XP's elevation folder
2. Import OSM patches if provided
3. Configure tile settings:
   - Use the addon's elevation source
   - **Important**: Build the full tile (DSF + terrain)

### Step 3: Install the Patch

Move the built tile to your patches directory:

```bash
mv ~/Ortho4XP/Tiles/+33-119/ ~/.xearthlayer/patches/KLAX_Mesh/
```

### Step 4: Verify

```bash
xearthlayer patches validate --name KLAX_Mesh
```

## CLI Commands

### List Patches

```bash
xearthlayer patches list
```

Shows all patches with their priority order, validation status, and file counts.

### Validate Patches

```bash
# Validate all patches
xearthlayer patches validate

# Validate a specific patch
xearthlayer patches validate --name KLAX_Mesh
```

Checks that patches have the required directory structure.

### Show Patches Directory

```bash
xearthlayer patches path
```

Displays the configured patches directory location.

## Configuration

Patches settings in `~/.xearthlayer/config.ini`:

```ini
[patches]
; Enable/disable patches functionality (default: true)
enabled = true

; Directory containing patch tiles (default: ~/.xearthlayer/patches)
directory = ~/.xearthlayer/patches
```

### Configuration Keys

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `patches.enabled` | bool | `true` | Enable/disable patches mounting |
| `patches.directory` | path | `~/.xearthlayer/patches` | Location of patch tiles |

## X-Plane Scenery Order

XEL mount points are prefixed to control X-Plane's loading order:

| Prefix | Type | Example | Priority |
|--------|------|---------|----------|
| `yz*` | Overlays | `yzXEL_na_overlay` | Highest (loaded first) |
| `zzy*` | Patches | `zzyXEL_patches_ortho` | After overlays |
| `zz*` | Orthos | `zzXEL_na_ortho` | Lowest (loaded last) |

This ensures:
1. Overlays (trees, buildings) appear on top
2. Patches override regional ortho tiles for their specific areas
3. Regional orthos fill in everywhere else

## Troubleshooting

### Patches Not Detected

```bash
xearthlayer patches list
```

Check that:
- Patches directory exists (`~/.xearthlayer/patches/`)
- Each patch has `Earth nav data/` directory
- DSF files are present in `Earth nav data/+XX-XXX/` subdirectories

### Invalid Patch

```bash
xearthlayer patches validate --name MyPatch
```

Common issues:
- Missing `Earth nav data/` directory
- No DSF files found
- Empty directories

### Patches Not Loading in X-Plane

1. Verify patches are mounted:
   ```bash
   ls -la ~/X-Plane\ 12/Custom\ Scenery/zzyXEL_patches_ortho/
   ```

2. Check X-Plane scenery_packs.ini:
   - `zzyXEL_patches_ortho` should appear before `zzXEL_*_ortho` entries

3. Restart X-Plane after adding new patches

## Example Workflows

### SFD KLAX Integration

```bash
# 1. Navigate to KLAX addon
cd "/path/to/SFD_KLAX_Los_Angeles_HD_1_Airport"

# 2. Check for patch data
ls !Ortho4XP_Patch/
# Shows: Elevations/  Patches/

# 3. Build tile in Ortho4XP with KLAX elevation
# (Use Ortho4XP GUI or CLI)

# 4. Install the patch
mkdir -p ~/.xearthlayer/patches
mv ~/Ortho4XP/Tiles/+33-119 ~/.xearthlayer/patches/A_KLAX_Mesh/

# 5. Verify
xearthlayer patches list
```

### Multiple Airport Patches

```bash
# Install multiple patches with priority control
mv ~/Ortho4XP/Tiles/+33-119 ~/.xearthlayer/patches/A_KLAX/
mv ~/Ortho4XP/Tiles/+39-105 ~/.xearthlayer/patches/B_KDEN/
mv ~/Ortho4XP/Tiles/+40-074 ~/.xearthlayer/patches/C_KJFK/

# Verify all patches
xearthlayer patches list
xearthlayer patches validate
```
