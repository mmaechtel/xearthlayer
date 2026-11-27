# 02: The decal library `maquify_2_green_key.dcl` and the decal directives

Research date 2026-11-27. No X-Plane installation was available, so the file in
`Resources/default scenery/1000 decals/` was not read directly. Every statement below names
the source it came from and carries a tag:

- **LAMINAR DOCUMENTATION**: developer.x-plane.com
- **SOURCE CODE READ**: code in a repository, read by me
- **TEST FIXTURE**: test data in a repository
- **THIRD-PARTY COPY**: a `.dcl` file found in someone else's repository (copied from an X-Plane install at some point)
- **THIRD-PARTY NOTES**: another project's research notes describing its local X-Plane install
- **DEVELOPER OPINION**: what a tool's author says in release notes, docs or help text
- **OWN INFERENCE**: my conclusion, not stated in any source

The OrthoStreamXP checkout is at
`/tmp/claude-0/-home-user-xearthlayer/4d861a8f-421a-5076-a53d-74daa3f67b1a/scratchpad/dl/osclone/repo`
(commit 946f632). Paths in the OrthoStreamXP section below are relative to that folder. Web downloads are in
`/tmp/claude-0/-home-user-xearthlayer/4d861a8f-421a-5076-a53d-74daa3f67b1a/scratchpad/decalweb/`.

---

## 1. Summary

1. **What the real file contains is only partly established.** Two public GitHub repositories each hold a
   byte-identical copy of `maquify_2_green_key.dcl`, copied from an older X-Plane install (the `.pol`
   files next to it were made by WED 2.0.0r3, which is X-Plane 10 era). That copy holds **one**
   `DECAL_PARAMS_PROJ` line (it repeats every 15 m × 15 m) and a Laminar-style comment. Whether
   X-Plane 12 ships the same numbers was **not confirmed**. A third-party scan of an X-Plane 12.4.3 install
   quotes the sibling `maquify_1_green_key.dcl` with the same key structure but 30 m repeats and dither 0.5.
   OrthoStreamXP's test fixture is a third variant (25 m, key ±12/20.5/8, constant 1.5) and points at a
   made-up texture name, so it is not a copy of the real file.
2. **What "green key" means.** The RGB decal (shrubs) is brought in where the albedo's green channel
   is higher than its red and blue. The alpha decal (stony dirt) is brought in everywhere else. The
   file's own comment says: *"draws shrubby vegetation over anything vaguely green, and stony dirt everywhere else"*.
3. **How the decal blends.** Laminar says a greyscale decal blends *"similar to Photoshop's 'hard light'
   mode"*, and that a decal seen from far away (blurred to one tone) has *"no effect"*. OrthoStreamXP models
   this as modulation around mid-grey 128 and weakens the decal by fading the decal texture toward 128.
4. **Ortho4XP** writes `DECAL_LIB lib/g10/decals/maquify_2_green_key.dcl` on land **and sea** triangles
   (`tri_type != 1`), but not on inland water. Its help text names `maquify_1_green_key.dcl`, which does not match the code.

---

## 2. Laminar documentation: decal directives

### 2.1 "Standard Shading Options for X-Plane Scenery"
URL: https://developer.x-plane.com/article/standard-shading-options-for-x-plane-10-scenery/ (also served as `?p=4889`).
Tag: **LAMINAR DOCUMENTATION** for everything in this section.

- The decal stage *"blends a higher resolution repeating texture into a base texture to create the illusion of high
  resolution. Because the decal is part of the shading process (and not an additional layer on top of the terrain),
  the performance impact is quite low."*
- **Limit:** *"Shaders can use up to two decal commands (neither of which are required). Shaders can also refer to a
  separate .dcl file (which might be in the library); if the .dcl file contains two decal directives, then no other
  decals can be included. (In other words, you get two decal effects, not two decal commands – one command loading two
  effects from one file uses up the entire decal budget.)"*
- Scale ratio = *"how many times the decal repeats for each iteration of the albedo texture"*. Texture paths are relative.
- The RGB part is a colour decal. The alpha channel is *"treated as a gray-scale decal that takes on the 'tint' of the albedo"*.
  Within one decal command, RGB and A share one scale. Two separate decal commands can use different scales.
- A greyscale PNG loads an "alpha-only" decal and saves VRAM. An RGB decal costs the same memory as an RGBA decal.
- Directives, verbatim syntax:
  - `DECAL [scale] [texture file]` and `DECAL_RGBA [scale] [texture file]`: *"add a decal using only the alpha channel and
    only the RGB channel of the decal file, respectively."*
  - `DECAL_KEYED [scale] [r] [g] [b] [a] [alpha] [texture file]`: colour-keyed to the albedo. *"if you set G to 1.0 then
    you will get 100% of your RGB decal when the green channel of the albedo is at 100%"*. `alpha` sets how much of the
    alpha-channel decal is used where the RGB decal is not (0 = never, 1 = everywhere the RGB decal is not used).
  - `DECAL_PARAMS [scale] [dither] [r] [g] [b] [a] [m] [k] [r] [g] [b] [a] [m] [k] [texture file]`
  - `DECAL_PARAMS_PROJ [scale_x] [scale_y] [dither] [r] [g] [b] [a] [m] [k] [r] [g] [b] [a] [m] [k] [texture file]`
    (all on one line). *"For the projected format, the decal does not follow the albedo, but rather repeats every scale_x
    meters horizontally and every scale_y meters vertically."*
  - **dither:** *"increasing this parameter from 0 to 1 mixes the alpha channel of the decal into the alpha channel of the
    albedo"*, used for ragged alpha edges. Note: OWN INFERENCE: on an opaque tile with `NO_ALPHA` the dither has no visible effect.
  - **The two sets of six numbers:** r, g, b, a bring the decal in by the albedo's red, green, blue and alpha channel.
    `m` ("modulator") *"has no effect except in certain cases"* (the TEXTURE_CONTROL / TEXTURE_MODULATOR red/blue channel is multiplied by m).
    `k` is a constant that *"brings in the decal no matter what"*. *"the levels of all parameters are added before the decal is
    brought in"*. Example: `k = 1, G = -1` gives *"a decal that avoids vegetation"*. **The first set drives the RGB decal and the second set drives the alpha decal.**
  - `DECAL_LIB [file path or library path]`: loads a decal from another file, possibly from the library. *"Whether this command
    uses one or two decal shaders depends on the particular file loaded."*
  - X-Plane 12.1.0+: `NORMAL_DECAL [scale] [texture file] [gloss]`, `NORMAL_DECAL_KEYED [scale] [r] [g] [b] [a] [texture file] [gloss]`,
    `NORMAL_DECAL_PARAMS [scale] [r] [g] [b] [a] [m] [k] [texture file] [gloss]`,
    `NORMAL_DECAL_PARAMS_PROJ [scale_x] [scale_y] [r] [g] [b] [a] [m] [k] [texture file] [gloss]`. The page does not define `gloss`.
  - Since 12.1.0 decals and normal decals also work in `.obj`.
- Alpha commands (`NO_ALPHA`, `DITHER_ALPHA`, `NO_BLEND`) *"take effect after decaling"*.
- Detailing extension: the control texture's red channel brings in the RGB decal (and the first normal decal), and its blue channel
  the alpha decal (and the second normal decal). *"the gray-scale detail does apply on top of the RGB detail"*.
  With a second decal the modulator channels are swapped.
- The page has **no** `.dcl` file-format section and no example `.dcl` contents.

### 2.2 "Using Decals to Add Detail To Scenery"
URL: https://developer.x-plane.com/article/using-decals-to-add-detail-to-scenery/ (also `?p=5399`).
Tag: **LAMINAR DOCUMENTATION**.

- *"Decals work by modulating the textures they overlay."*
- *"The decal shader ensures that when a decal has become a solid tone (because it is far away) the net result is the same as
  your underlying albedo. In other words, as you move away from the decal, the decal blurs into 'no effect'."*
- **Blend rule:** *"X-Plane greyscale decals are applied using a blending mode similar to Photoshop's 'hard light' mode."*
- `DECAL 25.0 ..textures/soil/DECAL_stony_dirt.png` on a `PROJECTED 1500 1500` terrain = repeats every 60 m.
- Worked `DECAL_PARAMS` example on a `.ter`:
  `DECAL_PARAMS 25.0 0.5 -3.5 0.0 -3.5 3.0 2.0 -2.5 2.0 0.0 ../textures/loveg/RGBA_DECAL_shrub_dirt_sdry3.png`
  - Note: this example has only 4 numbers per key (r g b a). The other article documents 6 (r g b a m k). This is a discrepancy in Laminar's own documents. OWN INFERENCE: the article is older (X-Plane 10) and predates m and k, or it shortens the line.
  - It is the **same texture** that `maquify_*_green_key.dcl` uses (`RGBA_DECAL_shrub_dirt_sdry3.png`): an RGB "shrub" decal and a greyscale "stony_dirt" decal in its alpha channel.
  - **The "key":** *"A key is a variable which tells the shader how strongly to apply a decal to a given pixel of the albedo texture. DECAL_KEYED uses a single key to control both the RGB and the greyscale decals, while DECAL_PARAMS uses two keys, one for each decal."*
  - *"The final key values are clamped between 0.0 and 1.0, so 'overdriving' the key values will not cause artefacts"*. Large coefficients (±3.5) are used to get *"a reasonably sharp transition"*, because the tonal difference between vegetation and rock pixels is small.
  - The RGB (shrub) decal key is tuned to *"darker, predominantly green colours"*, and the greyscale (stony dirt) key to *"brighter, grey tones"*.
  - Dither ratio 0.5 = the albedo and the decal have equal influence on composite-border dithering. Higher values favour the decal.
- Scale advice: a ratio above about 100–150 between successive elements (e.g. a 10 m decal on a 4000 m albedo) looks like an unconnected layer. **DEVELOPER OPINION** (Laminar artist, in the same article).

### 2.3 Other Laminar pages
- "Detail Textures for Aircraft", https://developer.x-plane.com/article/detail-textures-for-aircraft/ (**LAMINAR DOCUMENTATION**):
  *"Detail Textures were previously referred to as 'DECALS'. The term DECAL is a legacy term."* The directives keep their names.
- "A Few Notes on 12.1.0 for Developers", https://developer.x-plane.com/2024/02/a-few-notes-on-12-1-0-for-developers/ (**LAMINAR DOCUMENTATION**):
  normal-map decals were added; the whole decal system is usable in OBJs.
- `https://developer.x-plane.com/?s=maquify` returns **no results**. `?s=decal` lists only the pages above plus unrelated blog posts.
  No Laminar page documents the `.dcl` format, `1000 decals` or `maquify`.

---

## 3. What `maquify_2_green_key.dcl` contains

### 3.1 Third-party copies (the only copies of maquify_2 I saw)
Found with `gh search code maquify_2_green_key`. Downloaded raw:
- https://raw.githubusercontent.com/kdegonge/PANC-Anchorage/master/orthos/maquify_2_green_key.dcl
- https://raw.githubusercontent.com/kdegonge/KSDF-Louisville/master/ortho/maquify_2_green_key.dcl

The two copies are byte-identical (296 bytes, LF line ends, tab separated). Tag: **THIRD-PARTY COPY**. Contents, with tabs shown as `→`:

```
A
1000
DECAL

DECAL_PARAMS_PROJ→15→15→0.8→→-7.0→15→-8.0→0→0.0→0.0→→7→-15→8→-0.0→0.0→1.0→RGBA_DECAL_shrub_dirt_sdry3.png


# This decal draws shrubby vegetation over anything vaguely green, and stony dirt everywhere else. Effective for semi-desert, or any predominantly green/brown loveg terrain.
```

- The repo author copied the file next to their own draped polygons (`DECAL_LIB maquify_2_green_key.dcl` in e.g.
  https://raw.githubusercontent.com/kdegonge/PANC-Anchorage/master/orthos/6vun345845.pol, header `# Created by WED 2.0.0r3`),
  and the texture path was flattened. OWN INFERENCE: the original line probably reads `textures/RGBA_DECAL_shrub_dirt_sdry3.png`,
  as the X-Plane 12 copy of maquify_1 does (3.2). The rest (header `A / 1000 / DECAL`, numbers, Laminar-style comment) looks
  unedited. Its age is WED 2.0 era (about X-Plane 10), so **it may differ from what X-Plane 12 ships.**

Decoding by Laminar's parameter list (OWN INFERENCE from 2.1):

| Field | Value | Meaning |
|---|---|---|
| scale_x, scale_y | 15, 15 | projected: repeats every 15 m × 15 m on the ground, independent of the photo's UVs |
| dither | 0.8 | mixes the decal alpha into the albedo alpha; has no visible effect with `NO_ALPHA` (OWN INFERENCE) |
| RGB key r g b a m k | -7.0, 15, -8.0, 0, 0.0, 0.0 | shrub decal (RGB) = clamp(-7R + 15G - 8B) |
| alpha key r g b a m k | 7, -15, 8, -0.0, 0.0, 1.0 | stony-dirt decal (alpha) = clamp(7R - 15G + 8B + 1) |
| texture | RGBA_DECAL_shrub_dirt_sdry3.png | RGB = shrub, A = stony dirt (per the Laminar article in 2.2) |

The two keys are exact complements up to the constant, so they hand over from one to the other (OWN INFERENCE).
- Neutral grey albedo (R = G = B = x): shrub = 0 and dirt = 1.0, so pure stony dirt.
- Green-dominant pixel, e.g. (0.30, 0.40, 0.25): shrub = clamp(-2.1 + 6 - 2) = 1, dirt = clamp(2.1 - 6 + 2 + 1) = 0, so pure shrubs.
- Whether the key reads sRGB or linear albedo values is not documented.

### 3.2 Sibling `maquify_1_green_key.dcl`, X-Plane 12.4.3 (third-party notes)
Source: https://raw.githubusercontent.com/The3dVehicleguy/Scenery-Editor-X/master/research_notes/X%20Plane%2012%20graphics%20pipeline/default_art_assets.md
(a different project's notes from a scan of a local X-Plane 12.4.3 install). Tag: **THIRD-PARTY NOTES**, quoted, not verified by me:

> `maquify_1_green_key.dcl` keys an RGBA decal to "anything vaguely green" in the albedo:
> `DECAL_PARAMS_PROJ 30 30 0.5 -12.0 20.5 -8.0 0 0.0 0.0 12 -20 8 -0.0 0.0 1.0 textures/RGBA_DECAL_shrub_dirt_sdry3.png`.

Further claims in the same notes:
- `.dcl` header is `1000 DECAL`; X-Plane 12.4.3 ships 220 `.dcl` files.
- `RGBA_DECAL_shrub_dirt_sdry3.png` is RGBA 1024² in `1000 decals/textures`.
- Most-referenced libraries in the default art: `maquify_1_alpha_key.dcl` (6140 refs), `maquify_1_green_key.dcl` (1400 refs).
  maquify_2 is not in that list, so the default scenery itself seems to use it little or not at all (OWN INFERENCE).
- `DECAL`, `DECAL_RGBA`, `DECAL_KEYED`, `NORMAL_DECAL` and `NORMAL_DECAL_KEYED` are never used in the shipped art. Everything uses `DECAL_PARAMS(_PROJ)` and `NORMAL_DECAL_PARAMS(_PROJ)`.

maquify_1 (X-Plane 12, per these notes) compared with maquify_2 (older third-party copy): the same structure and texture.
maquify_1 has a 30 m repeat (not 15 m), dither 0.5 (not 0.8), and stronger keys (-12/20.5/-8 and 12/-20/8 rather than -7/15/-8 and 7/-15/8).
That makes the shrub/dirt edge sharper (OWN INFERENCE).

### 3.3 Existence in X-Plane 12
- OrthoStudio XP `src/orthostudio/decals.py` (https://raw.githubusercontent.com/radioharris/orthostudio-xp/main/src/orthostudio/decals.py, **SOURCE CODE READ**)
  lists 68 decal names that X-Plane exports as `lib/g10/decals/<name>`. The list includes `maquify_1_alpha_key.dcl`, `maquify_1_green_key.dcl`,
  `maquify_1_mod_key.dcl` and `maquify_2_green_key.dcl`, and leaves out five names that *"X-Plane 12.4.4 no longer exports"*.
  `DEFAULT_DECAL = "maquify_2_green_key.dcl"` with the docstring *"Ortho4XP's, the only one before the choice existed"*. So maquify_2 is still exported in 12.4.4 (DEVELOPER OPINION / claim).
- Decal4Ortho README (https://raw.githubusercontent.com/kellpossible/decal4ortho/master/README.md, **SOURCE CODE READ**) uses
  `lib/g10/decals/maquify_1_green_key.dcl` as its DEFAULT and also suggests `grass_and_asphalt_green_key.dcl`. It skips files containing `WET` by default.

### 3.4 What could not be established
- The exact contents of X-Plane 12's own `maquify_2_green_key.dcl` (numbers, texture path, whether a `NORMAL_DECAL_*` line was added in 12.1+).
  Before relying on any number, read it from a real install: `Resources/default scenery/1000 decals/library.txt` (EXPORT line) and the `.dcl` it points to.
- The texture size of `RGBA_DECAL_shrub_dirt_sdry3.png`: 1024² RGBA per the third-party notes only.

---

## 4. Ortho4XP
Source: https://raw.githubusercontent.com/oscarpilote/Ortho4XP/master/src/O4_DSF_Utils.py and `O4_Config_Utils.py`. Tag: **SOURCE CODE READ**.

- `use_decal_on_terrain`: type bool, default **False**. Hint text: *"Terrain files for all but water triangles will contain the
  maquify_1_green_key.dcl decal directive. The effect is noticeable at very low altitude and helps to overcome the orthophoto blur at
  such levels. Can be slightly distracting at higher altitude."* (DEVELOPER OPINION in the hint).
- The code (O4_DSF_Utils.py, around line 341) writes **maquify_2**, not maquify_1:
  ```python
  # Hack/TODO
  # Should we use decals on ocean floor ?
  #if (not tri_type) and (tile.use_decal_on_terrain):
  if (tri_type != 1) and (tile.use_decal_on_terrain):
      f.write("DECAL_LIB lib/g10/decals/maquify_2_green_key.dcl\n")
  ```
  `tri_type` 0 = land, 1 = inland water (`_water` suffix), 2 = sea (`_sea` suffix). So the decal goes on **land and sea** `.ter` files,
  not on inland water. The line is written after `BASE_TEX_NOWRAP` and the BORDER_TEX/mask lines, and before `WET` / `NO_ALPHA` / `NO_SHADOW`.
- Ortho4XP `.ter` layout: `A\n800\nTERRAIN\n\n`, `LOAD_CENTER lat lon size 4096`, `BASE_TEX_NOWRAP ../textures/<tex>`, optionally
  `WATER_COLOR_MASK` (XP12 water, not overlay) / `BORDER_TEX` / `LOAD_CENTER_BORDER`, optionally `DECAL_LIB …`, then `WET` (water) or `NO_ALPHA` (land),
  then `NO_SHADOW` for water or when terrain shadows are off.

---

## 5. OrthoStreamXP (commit 946f632)

### 5.1 How it works (**SOURCE CODE READ**)
`src/OrthoStreamXP/Scenery/FileSystem/TerrainDecals.cs`, `TerrainDefinitions.cs`:
- Class doc: the library *"sprinkles procedural grass, shrubs and dirt over the ground up close; over sharp aerial photos it looks like noise.
  The decals modulate the ground's colours around mid-grey (2x modulation: 128 leaves it as it is), so a decal texture faded towards grey is
  the same decal, weaker."* (The "2x modulation around 128" is the author's model. Laminar says "similar to hard light".)
- Strength 100 %: the `DECAL_LIB` line stays. Strength 0 %: every `DECAL\w*` line is removed (regex `^[ \t]*DECAL\w*\b[^\n]*\n?`).
  In between: the `DECAL_LIB <lib>` line is **replaced by the library's own decal lines**, with each texture repointed to `../orthostreamxp_decals/<name>`.
- Reading the library: `<X-Plane>/Resources/default scenery/1000 decals/library.txt`, `EXPORT lib/g10/decals/... <file>` lines map a library path to a file.
  The `.dcl` must have `DECAL` on line 3 (after `A`/`I` and the version). Lines 4+ that are empty or `#` are skipped. Every other line must start with `DECAL`
  (so `NORMAL_DECAL_*` lines are **not** accepted, and the library is then kept at full strength). The last token is the texture, resolved relative to the `.dcl` folder.
  Tokens are re-joined with tabs.
- **Serving the faded textures:** a virtual folder `orthostreamxp_decals` exists only in the mount (`TerrainDecals.Folder`). `SceneryOverlay.cs` line 71–72
  serves `_decals.Texture(path)` from memory. Each texture is faded once (lazy, cached per name), decoded with SkiaSharp as unpremultiplied RGBA8888,
  each channel v mapped to `round(128 + (v - 128) * strength/100)`, **alpha included** (*"Alpha is a second (greyscale) decal, not transparency, so it is faded the same way and never premultiplied"*),
  and re-encoded as **PNG**. If two decal textures have the same file name from different paths, the library is kept unfaded.
- Fallbacks keep the library at 100 % (with a log warning): no X-Plane folder, library not exported, `.dcl` header not `DECAL`, any line it cannot fade, texture missing.
- Generated `.ter` for a land tile (no package file): `A\n800\nTERRAIN\n\nLOAD_CENTER … \nBASE_TEX_NOWRAP ../textures/<stem>.dds\n[LIT_TEX_NOWRAP …\n]DECAL_LIB lib/g10/decals/maquify_2_green_key.dcl\nNO_ALPHA\n`,
  then the strength rule is applied (`TerrainDefinitions.Generated`).
- **Water/sea overlays:** generated `_water_overlay.ter` / `_sea_overlay.ter` get **no decal** (`BASE_TEX_NOWRAP` + `BORDER_TEX` (water_transition.png or the tile's ZL mask) + `WET` + `NO_SHADOW`).
  For a package's own overlay `.ter`, `Adjusted` still runs `WithDecals`, so any decal line the package put there is faded or removed like on land. The night-texture line is never added to overlays.
- Wiring: `MountSession.cs` line 143 `new TerrainDecals(Path.GetDirectoryName(s.CustomScenery), s.GroundDetail)`. The X-Plane folder is derived from the Custom Scenery path.

### 5.2 Test fixtures (**TEST FIXTURE**), `tests/OrthoStreamXP.Tests/TerrainDefinitionsTests.cs`
- The class claim: *"Generated .ter files must match AutoOrtho's own files byte for byte (expected texts are copied from z_ao_eur)."* Expected land `.ter`:
  ```
  A\n800\nTERRAIN\n\nLOAD_CENTER 74.50816 18.85254 2613 8192\nBASE_TEX_NOWRAP ../textures/11952_36192_BI16.dds\nDECAL_LIB lib/g10/decals/maquify_2_green_key.dcl\nNO_ALPHA\n
  ```
  Water overlay: `…BASE_TEX_NOWRAP ../textures/100000_125184_BI18.dds\nBORDER_TEX ../textures/water_transition.png\nWET\nNO_SHADOW\n` (no decal).
  This means AutoOrtho's own z_ao packages carry the maquify_2 line on land tiles (claim by the fixture author).
- Fake X-Plane `library.txt`: `A\n800\nLIBRARY\n\nEXPORT\t lib/g10/decals/maquify_2_green_key.dcl\t\tmaquify_2_green_key.dcl\n` (literally `EXPORT\t{Library}\t\tmaquify_2_green_key.dcl`).
- Fake `.dcl` (C# string):
  ```
  "A\r\n1000\r\nDECAL\r\n\r\nDECAL_PARAMS_PROJ\t25\t25\t0.8\t\t-12.0\t20.5\t-8.0\t0\t0.0\t0.0\t\t12\t-20\t8\t-0.0\t0.0\t1.5\ttextures/shrub_dirt.png\r\n\r\n\r\n# a comment\r\n"
  ```
  with a 2×1 RGBA texture `textures/shrub_dirt.png`. **It does not match any real copy:** it mixes 0.8 dither (as in the maquify_2 copy),
  maquify_1-like keys (-12/20.5/-8, 12/-20/8), a 25 m scale and k = 1.5 found in neither, and an invented texture name. Treat it as a format example, not as data.
- Expected inlined line at 50 %: `DECAL_PARAMS_PROJ\t25\t25\t0.8\t-12.0\t20.5\t-8.0\t0\t0.0\t0.0\t12\t-20\t8\t-0.0\t0.0\t1.5\t../orthostreamxp_decals/shrub_dirt.png\n`.
  Pixels `[0,0,0,0, 255,128,64,255]` fade to `[64,64,64,64, 192,128,96,192]`.
- A `.dcl` with an appended `NORMAL_DECAL_PARAMS 1 2 normal.png` line keeps `DECAL_LIB` unchanged (the library is not faded).
- Setting test: default 30; 0 and 29 are read as 30; 101 and -1 are errors.

### 5.3 Defaults and the developer's reasons (**DEVELOPER OPINION**)
- `release-notes/v1.2.0.md` (default 0 %): *"X-Plane draws its own procedural grass, shrubs and dirt over the ground up close. On sharp aerial photos this looks like noise: dark speckles and pale stony patches that the real imagery does not have. It is now off by default, so the photos look as they do on the map."* 10–90 % = *"a lighter version"*, 100 % = full effect.
- `release-notes/v1.2.2.md`: help text clarified. *"At 0 % (the default) the ground looks exactly like the photos; higher values add texture up close but make the photos look grainier"* (#21).
- `release-notes/v1.5.1.md` (default 30 %): *"The Ground detail slider … now goes from 30 % to 100 %, and 30 % is the default. At 30 %, X-Plane's grass and dirt pattern is faint, and the photos stay clean and sharp. Off is no longer a choice."* A former Off becomes 30 %. **No reason is given for dropping Off.**
- `docs/settings.md` "Ground detail": *"This helps X-Plane's blurry default textures, but on sharp aerial photos it looks like noise … The photo scenery packages ask for it in every photo tile"*. Strengths below 100 % *"are made from X-Plane's own decal files, so they need the X-Plane folder"*.
- GUI help (`Gui/Views/SettingsView.xaml`): *"Try it if the ground looks too flat to you during taxi or landing."* `docs/troubleshooting.md`: noisy, speckled ground means lower Ground detail.

---

## 6. Points for XEarthLayer (OWN INFERENCE)
- One `DECAL_LIB lib/g10/decals/maquify_2_green_key.dcl` line uses one decal slot (the file has one `DECAL_PARAMS_PROJ`, if the old copy is still accurate). Two decals is the limit.
- Being projected (metres, not UVs), the decal repeat is independent of tile zoom or texture size. That matters because XEL serves different texture resolutions.
- The "fade toward 128" method assumes 128 is neutral. Laminar's text (*"blurs into 'no effect'"*) could also mean the shader makes the decal's own average neutral. If the texture's mean is not 128, fading toward 128 would also shift brightness slightly, not only weaken the decal. This is untested.
- Fading needs the user's X-Plane install (reading `library.txt` and the `.dcl`, and redistribution concerns). The plain 0 % or 100 % options need only the line text.
- Ortho4XP puts the decal on sea `.ter` files as well. OrthoStreamXP's generated overlays have none.
