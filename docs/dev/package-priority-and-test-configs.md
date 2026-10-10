# Package Priority, Visibility and Configuration

Internal notes. Written while isolating the mipmap-chain artifact (issue #212)
on a machine with a large number of ortho packages, where it was not obvious
which package actually served the terrain under test.

## 1. Which package wins a tile

Several packages routinely cover the same ground. Only one can back a given
path through the consolidated mount. The rule, from
`ortho_union/builder.rs:297-302` and `ortho_union/index.rs:133`:

> Sources are sorted **ascending by sort key** and the **lowest key wins**.
> - Patch: `sort_key = _patches/{folder_name}`
> - Package: `sort_key = {region}`

The region is the package's code, **not** its directory name — it is the first
token on **line 3** of `xearthlayer_scenery_package.txt`:

```
REGIONAL SCENERY PACKAGE
1.0.0
E5_EU  99.99.99          <- line 3: region code, then version
2026-04-11T00:00:00Z
...
```

The region is lowercased before it becomes the sort key
(`xearthlayer-package/src/core.rs`, `ortho_union/source.rs`), so the comparison
runs on lowercase bytes. `_patches/` sorts before any region because `_` (0x5F)
precedes lowercase letters, so patches always outrank packages. The same
lowercasing puts `_` before every letter inside a code too.

The rule applies per file, not per tile: `terrain/` and `textures/` names are
resolved lazily and the first source holding that name wins, whichever package
the tile's DSF came from. Stray `.ter`/`.png` files in a high-priority package
therefore override other packages' masks at cell borders.

### The trap: digits sort before letters

Comparison is plain byte order, where `-` (0x2D) comes before `0`-`9`
(0x30-0x39), and those before `a`-`z` (0x61-0x7A). A region code that puts a
digit right after the continent letter therefore outranks the plain regional
package:

```
E0_EU     '0' = 0x30   <- wins
E5_EU     '5' = 0x35
EU        'U' = 0x55
EU2                     <- loses to all of the above
```

This is by design — it is how a targeted package takes precedence over the
broad regional one — but it means **the code decides priority, and renaming a
package changes which terrain you fly over**. A few hundred megabytes of
special-purpose scenery can quietly override a 49 GB regional package
everywhere the two overlap, without anything in the logs pointing it out.

### The code prefix encodes the intended layer

The region codes in use form a deliberate ladder, highest priority first:

| Prefix | Purpose |
|---|---|
| `E0_` / `A0_` / `N0_` / `S0_` | Airport packages — narrow, most specific, must win |
| `E1_` / `E2_` (`_TEST` suffix) | Experimental builds, normally disabled |
| `E3_` | Newer or self-built regional meshes |
| `A5_` / `E5_` / `N5_` / `O5_` | Custom meshes built on external elevation data |
| `NA-USA-MX-CENTRAL` / `SA-EAST` / … | World-rebuild regions (staging), the base layer |
| `ZY_` | Old library packages, demoted with `staging-prio.sh switch` until a rebuild region replaces them |
| `ZZ_` | World fillers and gap coverage — deliberately last, overridden by everything |

Because sorting is ascending byte order, this ladder is produced purely by the
prefix, but only between codes that share their first letter: `af-…` would beat
`e5_eu` wherever they overlap. A short old code is a prefix of the new ones and
sorts first (`na` < `na-canada-west`), which is why the old library packages
carry `ZY_`. The per-machine state, as a map, is in `scenery-layers.html`.
`ZZ_` sorts after every letter, which is exactly what a fallback layer
wants; `E0_` sorts before `E5_` and `EU`, which is what a targeted airport
package wants. Renaming a package moves it in this ladder — there is no separate
priority field.

Note the consequence for experimental packages: an `E1_`/`E2_` code outranks
everything except the airport layer, so enabling one silently takes over the
terrain wherever it has coverage.

### Determining the owner of a tile

Do this in Python, not shell. The obvious `[ "$a" \< "$b" ]` string comparison
is a bash-ism that fails under zsh with `condition expected: <`, and the loop
then silently keeps the first candidate instead of the lowest — which produces
plausible but wrong answers.

```python
import os
base = os.path.expanduser("~/.xearthlayer/packages")
pkgs = []
for p in sorted(os.listdir(base)):
    meta = os.path.join(base, p, "xearthlayer_scenery_package.txt")
    if not os.path.isfile(meta):
        continue                      # not indexed — see section 2
    with open(meta) as fh:
        lines = fh.read().split("\n")
    pkgs.append((lines[2].split()[0], p))
pkgs.sort()                           # sort_key = region, lowest wins

def owner(tile):                      # tile e.g. "+46+007"
    for region, p in pkgs:
        end = os.path.join(base, p, "Earth nav data")
        for root, dirs, files in os.walk(end):
            if tile + ".dsf" in files:
                return region, p
    return None
```

The ground-truth alternative, only available **while the service runs**, is to
compare size and mtime of the file in the mount
(`Custom Scenery/zzXEL_ortho/Earth nav data/{grid}/{tile}.dsf`) against each
candidate. With the service stopped the mount directory is empty and this
yields nothing.

## 2. Packages without metadata are invisible

A directory under `~/.xearthlayer/packages/` is indexed **only** if it contains
`xearthlayer_scenery_package.txt`. Without it the package is skipped silently:
it never appears in `xearthlayer packages list` or in
`xearthlayer scenery-index status`, no matter how much data it holds, and no
warning is emitted.

On this machine several packages sit in the packages directory this way and are
completely inert. Restoring the metadata file brings a package in — and its
region code then decides where it ranks, so a package restored this way can
immediately outrank the regional package it sits on top of.

This is not an accident but a deliberate mechanism: see "Disabling by renaming
the metadata file" below.

Check with:

```bash
xearthlayer scenery-index status
ls ~/.xearthlayer/packages/*/xearthlayer_scenery_package.txt
```

## 3. Library packages versus local ones

`xearthlayer packages check` queries `packages.library_url` and distinguishes:

- **`Up to date`** — exists in the library, carries a real version.
- **`v0.0.0 - Not installed`** — the library has no such code. Locally built or
  side-loaded; `packages list` shows the sentinel version `v99.99.99`.

Current inventory (23 indexed packages, 8,373,538 tiles):

**Library packages (11)** — `AF1` `AF2` `AS1` `AS2` `AS3` `AS4` `EU` `EU2`
`NA` `OC` `SA`

**Local packages, active (12)** — `A0_AF` `A0_AS` `A5_AF` `A5_AS` `AS5`
`E0_EU` `E5_EU` `N0_NA` `N5_NA` `O5_NZ` `S0_SA` `ZZ_ARKTIS`

**Local packages, disabled** — the two large gap-fill packages `ZZ_AS1_2`
(4.4 GB) and `ZZ_WORLD_FILL` (3.9 GB), held back on disk-space grounds, plus the
experimental Alps variants (`E3_EU` and the `zzXEL_alps_*` directories under
`~/.xearthlayer/deactivated/`). Their codes are recoverable from those files;
see section 4.

The `*0_*` codes are airport packages, the `*5_*` codes are the custom meshes,
the `ZZ_*` codes are the gap-fill layer — see the ladder in section 1.

### The `ZZ_*` gap-fill layer

`ZZ_WORLD_FILL` (`zzXEL_fillgap_ortho`), `ZZ_AS1_2` and `ZZ_ARKTIS` exist purely
to cover ground no other package reaches: oceanic islands, the Indonesian and
Pacific scatter, eastern Russia and the Kurils, and the Canadian Arctic. Their
codes sort after every letter, so they are overridden anywhere a real package
has coverage and can be enabled without disturbing the ladder.

Coverage audit against the 21 packages that were active before any of them was
switched on:

| Package | DSF tiles | of those, not covered elsewhere | Size | State |
|---|---|---|---|---|
| `ZZ_WORLD_FILL` | 985 | 973 | 3.9 GB | disabled |
| `ZZ_AS1_2` | 2684 | 266 | 4.4 GB | disabled |
| `ZZ_ARKTIS` | 392 | 392 | | enabled |
| `AS5` (`zzXEL_as5_extra_ortho`) | 21 | 20 | | enabled |
| `E3_EU` (`zzXEL_eu_alps_ortho`) | 37 | 0 | | disabled |

Only `ZZ_ARKTIS` and `AS5` are kept on; the two multi-gigabyte fill packages are
disabled for disk space. The remaining gap is 266 DSFs in the Indonesian and
Pacific scatter (`ZZ_AS1_2`) and 581 elsewhere in the world (`ZZ_WORLD_FILL`
minus what `ZZ_ARKTIS` already covers) — those coordinates fall back to whatever
X-Plane's default scenery provides.

Two caveats worth knowing:

- **`ZZ_ARKTIS` is a strict subset of `ZZ_WORLD_FILL`.** All 392 of its DSFs are
  also in the fill package, and both ship the *identical* terrain and texture
  sets (2,454,836 `.ter` files, 159,393 textures each). With the fill package
  disabled it is the one that supplies Arctic Canada; enabling both is harmless
  (`ZZ_ARKTIS` sorts first and wins its tiles) but adds no coverage.
- **`AS5` is not a `A5_` package.** Despite the name it sorts among the library
  `AS*` codes, after `AS4`, not in the custom-mesh tier. With 20 unique tiles and
  one overlap, the placement is inconsequential here, but the code does not
  follow the ladder.

## 4. Enabling and disabling

Either way, **rebuild the index afterwards**. Neither a directory appearing or
disappearing nor a metadata file being renamed invalidates the index cache on
its own; without `xearthlayer scenery-index update` the stale cache keeps
resolving to sources that are no longer valid.

### Preferred: rename the metadata file

```bash
cd ~/.xearthlayer/packages/zzXEL_example_ortho
mv xearthlayer_scenery_package.txt xearthlayer_scenery_package.txt.disabled
xearthlayer scenery-index update
```

The package stays where it is, becomes invisible to the index (section 2), and
**the region code is preserved**. That last part matters: the code is the
priority key and is not recorded anywhere else, so a package whose metadata was
deleted rather than renamed cannot be restored to its original priority without
guessing.

Re-enable by renaming back.

### Alternative: move the directory

```bash
mv ~/.xearthlayer/packages/zzXEL_example_ortho ~/.xearthlayer/deactivated/
xearthlayer scenery-index update
```

Useful when a package should be clearly out of the way — experimental builds,
for instance. It also preserves the metadata, but the package no longer shows up
when auditing the packages directory, which cuts both ways.

Before disabling anything, verify coverage does not develop holes — compare the
DSF sets:

```python
# tiles present only in the packages about to be disabled
only_disabled = set(dsfs_of(disabled)) - set(dsfs_of(remaining))
```

For the self-built regional meshes this returned 0 — all 168 of their DSF tiles
were also covered by library packages, so switching them off left no gaps.

## 5. Configurations used while investigating issue #212

| # | Configuration | Indexed packages | Tiles | Result |
|---|---|---|---|---|
| 1 | everything enabled | 21 | 5,875,279 | original artifact reported |
| 2 | self-built regional meshes off | 21 | 5,875,279 | **banding gone** with the 13-level chain |
| 3 | library packages only | 11 | 4,453,034 | clean reference, no competing sources |
| 4 | library + all `*5_*` + all `*0_*` | 20 | 5,865,035 | production-like |
| 5 | configuration 4 + `AS5` + all three `ZZ_*` | 25 | 11,652,728 | full global coverage, ~8.3 GB extra on disk |
| 6 | configuration 5 minus `ZZ_AS1_2` and `ZZ_WORLD_FILL` | 23 | 8,373,538 | current |

The instructive part: the same tile over the Alps (`+46+007`, under LSGS)
resolved to a **different package in each of configurations 2, 3 and 4** —
purely as a function of which other packages happened to be present. When
attributing anything mesh-related, establish which package actually served the
tile before drawing conclusions. It is not necessarily the one you assume.

This does not affect the mipmap chain: XEarthLayer generates the textures
itself regardless of which package supplies the mesh.

### Configuration 4 — the intended production set

Library packages plus the custom meshes (`A5_AF` `A5_AS` `E5_EU` `N5_NA`
`O5_NZ`) and the airport packages (`A0_AF` `A0_AS` `N0_NA` `S0_SA`). All
experimental variants are switched off.

The EU airport package (`E0_EU`) was initially missing from this set: its
metadata file had been renamed to `.disabled` during an earlier experiment, so
it was inert and easy to overlook. Restoring it brought the airport layer to
full coverage — AF, AS, EU, NA and SA — and raised the index to 21 packages /
5,917,930 tiles.

### Configuration 5 — full gap coverage

Configuration 4 plus `AS5` and all three `ZZ_*` gap-fill packages (section 3).
This closes roughly 1,650 DSF tiles that no other package covered — Arctic
Canada, the Pacific and Indonesian island scatter, eastern Russia, and scattered
islands worldwide. The index roughly doubles to 11.65 M tiles because the fill
packages ship a world-spanning `.ter` set, not because that much new terrain is
served; the `ZZ_*` codes sort last and only take effect where nothing else has
coverage. The Alps variants (`E3_EU`) stay disabled: 0 unique tiles.

### Configuration 6 — the current set

Configuration 5 with `ZZ_AS1_2` (4.4 GB) and `ZZ_WORLD_FILL` (3.9 GB) disabled
again — the coverage they add is real but thinly spread over remote water, and
8.3 GB is a poor trade for it. `ZZ_ARKTIS` stays on and keeps Arctic Canada.
Active DSF coverage is 18,245 tiles across 23 packages.

Note that disabling these does **not** free the space: the packages stay on disk
with their metadata renamed, they are merely unindexed (section 4). Reclaiming
the 8.3 GB means deleting the directories, which loses the region codes with
them.

## 6. What can be changed by editing config.ini, and what has to be rebuilt

Everything below lives in `~/.xearthlayer/config.ini`. The distinction that
matters in practice is not "does it need a restart" — everything needs a
restart — but **what becomes invalid** when you change it.

### Text edit only: restart the service and you are done

| Setting | Expected effect |
|---|---|
| `texture.compressor` (`software` / `ispc` / `gpu`) | Encode speed only. Output is structurally identical, so the existing cache stays valid. `gpu` is a WGSL port of the same ISPC kernels, so image quality is near-identical; `software` is a separate hand-rolled encoder and may differ slightly. |
| `texture.gpu_device` | Which adapter does the encoding. No effect on output. |
| `cache.memory_size`, `cache.disk_size`, `cache.dds_disk_ratio` | Budgets. Shrinking triggers eviction; nothing is invalidated. |
| `cache.disk_io_profile` | Disk concurrency only. |
| `generation.threads`, `generation.timeout` | Throughput, and how long a tile may take before a magenta placeholder is served. |
| `executor.*` (pool sizes, retries, timeouts) | Throughput and network behaviour. |
| `fuse.max_background`, `fuse.congestion_threshold` | How many concurrent scenery reads the kernel allows. Raise if X-Plane stalls at DSF boundaries. |
| `prefetch.*` | What is fetched ahead. Affects nothing already on disk. |
| `prewarm.grid_rows`, `prewarm.grid_cols` | Size of the `--airport` warm-up grid. |
| `logging.file` | Log destination. Note DEBUG at full tile rate produced ~2.9 MB/s here — roughly 10 GB/hour. |
| `patches.enabled`, `patches.directory` | Which patches take part. Patches outrank all packages. |

### Requires the DDS cache to be discarded

| Setting | Why | Expected effect |
|---|---|---|
| `provider.type` | Each provider has its own cache directory. | Nothing is lost, but the new provider starts empty — a full re-download for every tile flown over. |
| `texture.format` (`bc1` ↔ `bc3`) | Cached tiles are the other format and a different size. | **Currently broken for `bc3`.** `validate_dds_or_placeholder` (`fuse/placeholder.rs:211`) compares against `EXPECTED_DDS_SIZE`, a BC1-only constant, while the encoder honours the configured format. A BC3 tile is roughly twice the size, fails the check, and every tile is served as a magenta placeholder. Do not switch to `bc3` until that constant is made format-aware. |
| `cache.directory` | New location, empty. | Full re-download. |

Discard with `xearthlayer cache clear`. Note this clears **both** tiers — the
encoded DDS tiles and the raw imagery chunks. There is no DDS-only clear.

### Requires the scenery index to be rebuilt

Adding, removing or renaming anything under `~/.xearthlayer/packages/` or the
patches directory. Renaming a package's **region code** belongs here too, and is
the subtle one: the code is the priority key, so changing it silently changes
which package wins — see section 1.

### Not a config setting at all: zoom levels and mesh parameters

The ZL of a tile is baked into the package when it is built — the `.ter` files
reference specific textures. It cannot be changed from `config.ini`; it requires
rebuilding the ortho package. The same is true of mesh parameters such as the
triangulation angle. This is why mesh variants exist as separate packages rather
than as settings, and why comparing them means swapping packages and rebuilding
the index.

### Not user-configurable by design: the mipmap chain

The number of mipmap levels is derived from the texture size at encode time. It
is deliberately not exposed in `config.ini` — a truncated chain has no
legitimate use case, and a fixed constant of 5 was the cause of issue #212.

## 7. What was ruled out, and why each looked convincing

Two days went into the scenery side before the cause turned out to be in the
texture encoder. Every item below was varied in isolation and had **no effect
on the actual cause**. They are recorded here so nobody repeats them.

### Scenery build parameters (set when building the package, not in config.ini)

| Tried | Outcome |
|---|---|
| Mesh triangulation angle (`min_angle`) | No effect on the cause. Changing it *did* visibly reduce the artifact, which is exactly why it was so misleading — see below. |
| Terrain decals on/off | No effect. |
| `SUPER_ROUGHNESS` | No effect. |
| Terrain self-shadowing | No effect. |
| Vegetation overlays | No effect. |
| Elevation data / different mesh source | No effect. The decisive A/B test used *identical* DSF geometry and differed only in the textures. |
| Base zoom level (ZL16 vs ZL17 vs ZL18) | No effect on the cause. Both ZL16 and ZL17 showed it, ZL16 slightly worse. |

### X-Plane settings

| Tried | Outcome |
|---|---|
| Every relevant graphics setting | No effect. |
| Anisotropic filtering, raised | Reduces the symptom without addressing it. It makes sampling of the levels that *are* present better; it cannot supply the levels that are missing. Not a substitute for the fix. |
| VRAM / texture-resolution downsampling | No effect. |

### Imagery pipeline

| Tried | Outcome |
|---|---|
| Different providers | No effect. The artifact was independent of the imagery source. |
| Per-chunk mipmap seams | Not the cause. Decoded levels were inspected visually and statistically: median absolute difference between adjacent columns at the 256 px chunk boundaries was 0.29, and the largest jumps occurred at arbitrary positions on real terrain edges. Downsampling correctly spans the whole texture. |
| Downsample filter (box 2×2 vs. Lanczos) | Not the cause. A box filter over the full RGBA buffer is correct and standard for mipmaps. |
| Distance-dependent provider zoom switching | No such logic exists. The zoom mix in the cache is package-driven. |

### The methodological lesson

Several of these *appeared* to work. Changing the mesh angle made the pattern go
away; so did flying somewhere the ZL mix was different. That is because the
artifact's **visibility** depends on many things — slope, viewing angle, sun
position, where texture scale changes, how the mesh triangulates the terrain —
while its **cause** was constant everywhere.

Two consequences worth internalising:

1. **"It looks better" is not evidence of cause.** Only a controlled A/B —
   identical geometry, position, time of day and viewing angle, with exactly one
   variable changed — distinguishes a fix from a change in visibility. That is
   what finally settled it: the same location rendered through XEarthLayer and
   from a pre-baked tile carrying identical geometry but complete textures.
2. **Establish which package actually served the tile before attributing
   anything.** As section 5 shows, the same coordinates resolved to three
   different packages across three configurations. Several early conclusions
   were drawn about a mesh that was not the one being rendered.

A third signal, in hindsight the strongest: *nothing on the scenery side made
any difference*. When an entire category of variables has no effect, the cause
is usually not in that category.

## 8. Related

- `docs/dev/consolidated-mounting-design.md` — the mount and index design
- `docs/package-management.md` — user-facing package commands
- `xearthlayer/src/ortho_union/builder.rs` — `discover_sources()`, the sort
- `xearthlayer/src/ortho_union/index.rs` — priority-ordered resolution
