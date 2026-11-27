# EU-WEST ortho package: .ter directive scan

- Package: `zzXEL_eu-west_ortho` (EU-WEST, ortho), version 0.5.1, library v2 metadata dated 2026-10-31T01:05:54Z
- Parts read: 14 of 14 (`.aa` to `.an`), streamed in order into a tar.gz reader, nothing stored on disk
- Every part downloaded with curl exit 0, and the SHA-256 of each streamed part matches the metadata
- Stream complete: yes. The tar reader reached the end of the archive with no "stream ended" error

## Entries per folder

| Folder | Entries |
|---|---|
| (top level) | 6 |
| Earth nav data | 1,267 |
| night | 1,163 |
| terrain | 439,849 (all .ter) |
| textures | 32,321 |
| Total | 474,606 |

## Directive tally (first word of each line, over all 439,849 .ter files)

| Directive | Count |
|---|---|
| A | 439,849 |
| 800 | 439,849 |
| TERRAIN | 439,849 |
| LOAD_CENTER | 439,849 |
| BASE_TEX_NOWRAP | 439,849 |
| NO_ALPHA | 230,020 |
| TEXTURE_LIT | 230,020 |
| LUMINANCE | 230,020 |
| BORDER_TEX | 209,829 |
| WET | 209,829 |
| NO_SHADOW | 209,829 |
| LOAD_CENTER_BORDER | 32,320 |

There are 230,020 land files (NO_ALPHA, TEXTURE_LIT and LUMINANCE) and 209,829 sea overlay files (BORDER_TEX, WET and NO_SHADOW); 230,020 + 209,829 = 439,849. Only 32,320 of the sea overlays have a `LOAD_CENTER_BORDER` line, so the other 177,509 have `BORDER_TEX` without it. The scan does not explain why.

## DECAL lines

None. No line's first word contains `DECAL`. This matches the earlier NA-GREENLAND result (218,783 .ter files, no DECAL).

## Samples

Land, `terrain/10000_35504_GO216.ter`:

```
A
800
TERRAIN

LOAD_CENTER 77.12783 15.07324 2179 4096
BASE_TEX_NOWRAP ../textures/10000_35504_GO216.dds
NO_ALPHA
TEXTURE_LIT ../textures/10000_35504_GO216_LIT.dds
LUMINANCE 600
```

Sea overlay, `terrain/10000_35504_GO216_sea_overlay.ter`:

```
A
800
TERRAIN

LOAD_CENTER 77.12783 15.07324 2179 4096
BASE_TEX_NOWRAP ../textures/10000_35504_GO216.dds
LOAD_CENTER_BORDER 77.12783 15.07324 2179 1024
BORDER_TEX ../textures/10000_35504_ZL16.png
WET
NO_SHADOW
```

## Method and limits

- The scanner tallies only the first word of each non-empty line. A DECAL keyword anywhere else in a line would not be counted, though that is not valid .ter syntax.
- Only `.ter` files were checked. `.dsf`, `.pol` and other files were not searched for decal references.
- The scanner was a copy of `scan_ter.py` with one change: it also keeps the first `*sea*` file as a sample. The counting logic was not changed.
