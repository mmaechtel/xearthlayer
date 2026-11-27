# Serving `.ter` files with a decal line through the mount: design analysis

Read-only analysis of the code on branch `claude/kind-ramanujan-l2icgq` (2026-11-27). Nothing was
changed. "FS" is `xearthlayer/src/fuse/fuse3/ortho_union_fs.rs`. Items marked (inference) are not
read from code.

## Headline

A `.ter` is a plain passthrough today, with no content hook. The cheapest sound design appends the
decal text as a constant suffix: the size is `real_len + suffix_len` without opening the file at
lookup, and a read is "disk window, then suffix slice". That removes the size-before-open problem
and needs no content cache. It rests on two X-Plane behaviours that the code cannot confirm: a
decal line at the end of a `.ter` works, and trailing blank lines are harmless.

## Current path

- **Index:** `terrain/` and `textures/` are never scanned; they are registered as empty directories
  (`ortho_union/parallel.rs:92`, `:146-161`). The sequential `add_source` used by some tests does
  index terrain files (`ortho_union/index.rs:205-259`), so a hook must cover both branches.
- **lookup** (`FS:678-772`): indexed hit (`:699-710`, missed by production `.ter` files), then the
  lazy branch (`:730-740`): `resolve_lazy_geo` -> `fs::metadata` -> path inode -> `metadata_to_attr`,
  TTL 1 s.
- **Resolution:** `resolve_lazy_geo` (`FS:492-502`) tries patch sources first for geo-filtered
  names, then all sources. `resolve_lazy` (`index.rs:297-328`) returns the first
  `source/terrain/<name>` that exists; it returns only a path, not the matching source.
  `*_sea_overlay.ter` and `*_water_overlay.ter` never parse in `is_geo_filtered` (`FS:395-410`,
  `prefetch/tile_based/dsf_coord.rs:121-157`).
- **Attributes:** from the real file's metadata (`fuse/fuse3/shared.rs:119-150`), TTL 1 s (`:32`).
- **getattr** (`FS:774-835`): re-resolves by path on every call; nothing is cached.
- **open** (`FS:1160-1164`): non-virtual inode returns `fh: 0, flags: 0`; no fd is kept.
- **read** (`FS:916-965`): by path per call: re-resolve, disk I/O permit, open, seek, read.
- **readdir:** lists only the index, so `terrain/` is empty in production; X-Plane reaches `.ter`
  files by lookup alone.
- **macOS:** passthrough files use `flags: 0` on both platforms (page cache). Only virtual DDS
  differs (`FS:109-112`). A rewritten `.ter` should keep `flags: 0`.

## Where content could be substituted

No generic virtual-file mechanism exists. The pattern to copy is the lit seam: `LitTextureSource
{ size, read }` with the contract that `size` equals what `read` serves
(`night/service.rs:33-43`), a default implementation, injection by `with_lit_source` (`FS:307`).
A `.ter` keeps its path inode; the hook sits after path resolution in lookup (`:701-703`,
`:731-733`), getattr (`:814-817`, `:827-830`) and read (`:927-964`).

## Other readers of `.ter` files

All read the real package directories, none goes through the mount, so a FUSE-level rewrite does
not affect them: scenery index (`prefetch/scenery_index.rs:142-219`, `:536`, `:585-627`) and its
cache (`prefetch/scenery_cache.rs:209-262`), prewarm scanner (`prefetch/prewarm/scanner.rs:69-203`),
`available_tiles_in_dsf_tile` (`ortho_union/index.rs:604-656`), ortho union index cache
(`ortho_union/cache.rs:23-88`: version, source paths, root mtimes), `manager/sizes.rs:44-100`,
publisher dedupe (`xearthlayer-publisher/src/dedupe/detector.rs:401`), patch discovery
(`patches/discovery.rs:256`).

## Locating X-Plane

`MountManager::night_lit_source` (`manager/mounts.rs:246-279`) derives the installation from the
Custom Scenery path (`xplane/environment.rs:61-77`) and reads
`Resources/default scenery/1000 world terrain`; the decal folder is the sibling `1000 decals`.
`LibraryIndex` (`night/laminar/library.rs:15-60`) already parses `EXPORT` lines of a `library.txt`.
When the installation is missing, night lighting warns, stays black and records
`NightLighting::Dark` (`mounts.rs:224-258`).

## A folder that exists only in the mount

No precedent. Cheaper: virtual DDS files already exist inside the lazy `textures/` directory
without being listed, and X-Plane finds them by lookup. Faded decal textures can be served as
`../textures/xel_decal_<name>.png` with one lookup branch placed before the `.dds` branch (the DDS
filename regex is not anchored at the start, `fuse/filename.rs:116`).

## Configuration

A new key touches `config/settings.rs`, `defaults.rs`, `parser.rs`, `writer.rs`, `keys.rs`,
`docs/configuration.md`, `service/orchestrator_config.rs`, `service/orchestrator/core.rs` and
`manager/mounts.rs`. `config upgrade` needs nothing (`config/upgrade.rs:436-447`).

## Form (a): `DECAL_LIB` line on or off

1. `TerrainRewrite` trait with a pure implementation and a `NoRewrite` default (S). One constant
   suffix `"\nDECAL_LIB lib/g10/decals/maquify_2_green_key.dcl\n"`, applied to land names
   (`tile_from_ter_name` with `is_sea == false`).
2. Report which source matched, so patches can be excluded (S).
3. FUSE hook: `with_terrain_rewrite`, one helper used by lookup, getattr and read; size plus
   suffix length; read serves the disk window then the suffix (M).
4. Late guard: a file that already holds a `DECAL` token gets whitespace of the same length (S).
5. Config and wiring (S).
6. Check in the simulator on Linux and macOS (S, needs X-Plane).

Smallest first step: 1, 3 and 5, package sources only.

## Form (b): adjustable strength

Builds on (a); the suffix is still one constant per session.

1. `DecalLibrary` reader for `1000 decals/library.txt` and the `.dcl` (M). The `.dcl` grammar must
   come from a real installation.
2. Strength policy, pure (S): 100 keeps the library line, 0 no suffix, otherwise the library's
   lines with texture paths replaced.
3. Faded texture source: decode with `image`, `c' = 128 + (c - 128) * N / 100`, re-encode, hold in
   memory (M).
4. FUSE branch for the virtual textures (M).
5. Fallback and status when the installation or library is missing (S).
6. Config `strength`, docs, simulator check (S).

## Risks and unknowns

- X-Plane's parser: position of the decal line, trailing blank lines, duplicate decal lines. If
  position matters, the design falls back to reading at lookup (one extra open and read, or a
  bounded cache; an unbounded one would be about 55 MB for 218,783 files).
- Existing decal lines were checked in one package only (NA-GREENLAND, none found); the EU-WEST
  scan is recorded in `01_eu_west_ter_scan.md` once it has finished.
- Size before open holds only while the suffix is constant for the life of the mount.
- macOS is untested; macFUSE may hold pages for the life of the vnode (inference), harmless with
  fixed content.
- Patches are hand-made and may carry their own decals; not rewriting them needs step 2.
- Form (b): `.dcl` grammar and decal texture format are unverified.
- How decals look on lit textures at night is unknown.

## Open questions

1. Append at the end of the file (subject to a simulator test), or place the line where Ortho4XP
   does?
2. Leave patch `.ter` files alone?
3. Always exclude sea and water overlays?
4. Default off or on?
5. Form (b) without a decal library: fall back to the plain line, or switch off?
6. One global strength, or per region?
7. A `[decal]` section or keys under `[texture]`?
8. Ship form (a) first?
