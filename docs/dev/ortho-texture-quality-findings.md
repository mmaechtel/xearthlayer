# Ortho Texture Quality: Findings

What has been measured, researched and decided about making streamed orthophoto textures look
better. This page is the summary; sources, numbers and the full reasoning are on the branches named
under [Where the details are](#where-the-details-are). It records the state of 2026-11-27.

**Nothing here is a measured gain in the simulator.** No source that could be opened measures any
of these techniques on orthophotos used as flight simulator terrain. Every candidate is a proposal
for a test with a stop rule.

## Sharpening level 0

Not a default. Kept as an explicit taste setting on its branch.

- **Own benchmark** (`level0-bench`, 36 tiles from Bing, ArcGIS and Apple against the same ground at
  zoom 18): an unsharp mask lowers SSIM and PSNR on every tile at every strength, 0.1 included.
  Butteraugli improves slightly at 0.1 to 0.6 and gets worse at 1.0. The providers' zoom 16 already
  has 95 to 97% of the gradient energy of the reference.
- **In motion** (literature research, not measured): sharpening scales whatever aliasing mipmapping
  and anisotropic filtering let through, and X-Plane 12 has no temporal anti-aliasing to average
  it out (Laminar: no motion vectors in the engine, FSR 1 only). Display hold blur removes the gain
  along the flight direction at about 30 fps or in fast low flight. The risk of shimmer is plausible
  and unmeasured.
- **Cost** (measured, one 4096 tile, Ryzen 7 9800X3D, one thread): 9 ms with AVX-512, 14 ms with
  AVX2, 24 ms on the x86-64 baseline, against about 42 ms for the ISPC encode and 80 ms to assemble
  the chunks. On the GPU path about 13 ms per tile on the one adapter measured (AMD Radeon
  Graphics, RADV).

## Rejected

| Approach | Reason |
|---|---|
| Deblocking, deringing | Own benchmark: at most +0.0006 SSIM. Literature at JPEG quality 80: +0.00 dB for deblocking. |
| Quantisation-constrained decoding (jpeg2png family, Knusperli) | Literature at quality 80: -0.80 dB; the author of jpeg2png says it is poor on finely textured photographs. |
| Heavy or learned JPEG restoration (BM3D, CNNs) | Gains of 1 to 2 dB at quality 80, at seconds to minutes per tile when scaled to 4096 pixels. |
| Better chroma upsampling | zune-jpeg already uses the triangle filter. |
| Dithering before BC1 | Removed from stb_dxt as "always dubious"; no measured benefit found. |
| Global exposure and saturation | Built and dropped: provider differences come from different acquisitions, which no global value corrects. |
| Local contrast (CLAHE, local Laplacian), dehazing, shadow removal | Per-tile adaptive methods risk steps between tiles; no evidence of benefit; shadow removal is not real-time. |
| Baked noise or grain | Lost in the mip chain and costs BC1 palette; detail of this kind belongs in a `.ter` decal. |
| Learned super-resolution | Real-time models gain 0.1 to 0.4 dB over interpolation; the real next zoom level is one request away. |
| Mip levels from the provider's lower zoom | A download-saving technique (AutoOrtho, OrthoStreamXP), not a quality gain; coarse zooms can be other imagery. |

## Open candidates

In the order they look worth trying. None is built.

| Candidate | What is known | Stop rule |
|---|---|---|
| Detail decal added to `.ter` files in the mount | Packages contain no decal (see below). Laminar documents decals in `.ter` files as cheap and fading with distance. Ortho4XP can write one; OrthoStreamXP attenuates it to 30% because at full strength it "looks like noise". | Hand test first: write the line into a few installed `.ter` files and fly low. Stop if a line at the end of the file has no effect, or if the look is not wanted. |
| Level 0 reduced from zoom 17 | OrthoStreamXP ships it, for graphics memory, not quality. Four times the downloads. No measurement anywhere. | Measure in `level0-bench` against zoom 18. Stop below about +0.002 SSIM. |
| 8192 pixel textures from zoom 17 | The only candidate that adds real information. OrthoStreamXP ships them and rewrites `LOAD_CENTER`. Four times downloads, encode time and memory. No Laminar statement on a size limit in X-Plane 12. | One tile in the simulator, then memory and load time on a real flight. |
| Better filter than box for mip levels 1 to n | Recommended by Wronski and Castano, with no metric. Wider kernels need a rule for tile edges. | Compare against box in a motion benchmark; stop if no metric and no eye sees it. |
| BC7 | Largest format gain, twice the size. Not documented for X-Plane 12; Laminar's tools read DXT1/3/5 only. | One tile in the simulator. Stop if X-Plane does not load it. |
| JPEG upper bound from DCT coefficients | Not done in the benchmark: zune-jpeg exposes no coefficients. `mozjpeg-sys` does. | Offline in the benchmark only; closes the JPEG restoration question. |

## Decals: what the packages contain

Two ortho packages of version 0.5.1 were read completely, every `.ter` file:

| Package | `.ter` files | Decal lines |
|---|---|---|
| NA-GREENLAND | 218,783 | none |
| EU-WEST | 439,849 | none |

The other regions are unchecked. The directives that do occur are listed in
[Scenery Packages](scenery-packages.md#terrain). XEarthLayer would therefore have to add a decal,
where OrthoStreamXP weakens one its packages already ask for.

The decal Ortho4XP uses is `lib/g10/decals/maquify_2_green_key.dcl`. Public copies of the file
hold one `DECAL_PARAMS_PROJ` line: shrubs where green dominates the photo, stony dirt elsewhere,
repeating every 15 m. The file of an X-Plane 12 installation itself has not been read.

How `.ter` files pass through the mount today, and where a rewrite would hook in, is in
[FUSE Filesystem](fuse-filesystem.md#terrain-files-ter).

## Measuring quality in motion

No metric is validated for texture shimmer under camera motion. For a motion benchmark, run
ColorVideoVDP, FovVideoVDP and CGVQM side by side with per-frame SSIM and PSNR as a control, and
accept a metric only after it ranks a ladder of LOD biases (-1 to +1) in the known order. VMAF is
unsuitable: it rewards sharpening.

## Where the details are

| What | Where |
|---|---|
| Research reports (German) with sources: sharpening in motion; improvements beyond sharpening | branch `research/ortho-texture-quality`, `reports/` |
| Research notes, decal analysis, package scans | same branch, `research_notes/` |
| `level0-bench` and its results | branch `experiment/level0-sharpening`, `level0-bench/` |
| `texture.sharpen` on CPU and GPU, with timings | branch `feature/ortho-look-neu` |
| Linear-light mip chain | branch `fix/linear-light-mipmaps` (upstream PR #458) |
