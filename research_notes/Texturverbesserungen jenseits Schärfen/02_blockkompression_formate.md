# Block compression quality beyond sharpening: better BC1 encoding, alternative formats, dithering

Research date: 2026-11-27. Source rule applied: every cited finding below comes from a page or file that was
downloaded and read in this session ([OPENED]). Each finding carries an evidence label:
MEASURED / AUTHOR CLAIM / VENDOR DOCUMENTATION / LAMINAR STATEMENT / BLOG OPINION / SOURCE CODE FACT / OWN INFERENCE.
Items known only from search snippets or from memory are listed separately under "Unverified (not opened)" in each
Gaps section and must not be cited as fact.

Hosts that could not be reached from this environment (so their content is NOT verified):
aras-p.info, richg42.blogspot.com, userpages.cs.umbc.edu, hal.science, www.nist.gov, pubmed (cookie wall),
developer.download.nvidia.com, export.arxiv.org. This removes the three most number-rich sources I intended to use
(Aras Pranckevičius "Texture Compression in 2020", Geldreich's 2020 blog benchmarks, Griffin & Olano).

Opened sources (bibliography):

- [S1] Ignacio Castaño, "ICBC - A High Quality SIMD BC1 Encoder", README + chart `doc/kodim-chart.png`, GitHub, n.d. (v1.03-1.05 era, 2020). https://github.com/castano/icbc , chart: https://raw.githubusercontent.com/castano/icbc/master/doc/kodim-chart.png
- [S2] Ignacio Castaño, "BC1 Compression Revisited", ludicon.com blog. https://www.ludicon.com/castano/blog/?p=1359
- [S3] Richard Geldreich, `rgbcx.h` v1.12 header comments, 2020, in bc7enc_rdo. https://github.com/richgel999/bc7enc_rdo/blob/master/rgbcx.h
- [S4] Richard Geldreich, bc7enc README (BC1-5 + BC7 encoders; benchmark table vs ispc_texcomp). https://github.com/richgel999/bc7enc
- [S5] Richard Geldreich, bc7enc_rdo README. https://github.com/richgel999/bc7enc_rdo
- [S6] Fabian Giesen / Sean Barrett, `stb_dxt.h` v1.13 (version history and flag comments). https://github.com/nothings/stb/blob/master/stb_dxt.h
- [S7] Fabian Giesen, "What's that magic computation in stb__RefineBlock?", 2022-11-08. https://fgiesen.wordpress.com/2022/11/08/whats-that-magic-computation-in-stb__refineblock/
- [S8] Intel, ISPCTextureCompressor readme, `ispc_texcomp.h`, `kernel.ispc`. https://github.com/GameTechDev/ISPCTextureCompressor
- [S9] Traverse Research, intel-tex-rs-2 README (`intel_tex_2` crate). https://github.com/Traverse-Research/intel-tex-rs-2
- [S10] N. Hasenbanck, `block_compression` crate README + `src/settings.rs`; crates.io metadata. https://github.com/hasenbanck/block_compression , https://crates.io/crates/block_compression
- [S11] Matias Goldberg (darksylinc), Betsy GPU Compressor README. https://github.com/darksylinc/betsy
- [S12] J. Solanti, texpresso README + crates.io metadata. https://github.com/jansol/texpresso
- [S13] Ben Supnik (Laminar Research), "DDS Revisited in X-Plane 10", 2012-01. https://developer.x-plane.com/2012/01/dds-revisited-in-x-plane-10/
- [S14] Ben Supnik, "Do Not use DDS as an Editing Format!" (post + author comment), 2012-01/02. https://developer.x-plane.com/?p=3970
- [S15] Ben Supnik, "DDS and V10 - What Does It Do?", 2012-11. https://developer.x-plane.com/?p=4593
- [S16] Ben Supnik, "Revisiting Texture Compression", 2010-09. https://developer.x-plane.com/2010/09/revisiting-texture-compression/
- [S17] Laminar Research, "DDSTool Manual". https://developer.x-plane.com/docs/scenery/ddstool-manual/
- [S18] Laminar Research, xptools source: `src/XPTools/DDSTool.cpp`, `src/Utils/BitmapUtils.cpp`, `BitmapUtils.h` (master). https://github.com/X-Plane/xptools
- [S19] developer.x-plane.com site search for "BC7" and "BC5" (both: "No results found"). https://developer.x-plane.com/?s=BC7 , https://developer.x-plane.com/?s=BC5
- [S20] Wikipedia, "S3 Texture Compression" (wikitext). https://en.wikipedia.org/wiki/S3_Texture_Compression
- [S21] NVIDIA, "NVIDIA Texture Tools 3" product page + release notes. https://developer.nvidia.com/gpu-accelerated-texture-compression
- [S22] XEarthLayer source: `xearthlayer/src/dds/compressor.rs`, `types.rs`, `gpu_channel.rs`, `xearthlayer/Cargo.toml` (local repository).

## 1. BC1 encoder quality ladder: how many dB between a fast ISPC-class BC1 and the best BC1 encoder?

### Takeaway
The only quantitative encoder ladder I could open (Castaño's ICBC chart, Kodim+Waterloo) spans about 1.05 dB from stb_dxt-class (about 29.85 dB) to the best ICBC/rgbcx settings (about 30.9 dB); the ISPC BC1 kernel is algorithmically stb_dxt-class (PCA + one least-squares refinement, no quality knob) but is not on that chart, so its exact position is an inference. No opened source shows that a gain of this size is visible on photographic terrain, and no encoder comparison with SSIM or a perceptual metric was found.

### Cited Findings
- MEASURED (chart read by eye, values approximate, x axis = average PSNR in dB on Kodim+Waterloo, y axis = "Time", unit not labelled): stb about 29.85-30.06 dB at about 0.2-0.3 time units; RGBCX levels 0-18 from about 29.86 dB (about 0.2) to about 30.90 dB (about 20.6); ICBC-AVX512 from about 29.97 dB (about 0.6) to about 30.90 dB (about 5.5), reaching about 30.80 dB at about 0.9; ICBC-Scalar reaching about 30.90 dB at about 19.3; squish 1.15 about 30.42 dB at about 7.3; AMD CMP_Core about 30.25 dB at about 14.6. The whole ladder is therefore about 1.05 dB wide — [S1 chart](https://raw.githubusercontent.com/castano/icbc/master/doc/kodim-chart.png)
- AUTHOR CLAIM: chart configuration is ICBC 1.03 with `three_color_mode` and `three_color_black` true; RGBCx 1.12 levels 0-18 with `allow_3color` and `use_transparent_texels_for_black` enabled; stb_dxt 1.09 "with various improvements that Rich and I have proposed"; squish 1.15; CMP_Core V4.0.4855 — [S1](https://github.com/castano/icbc)
- AUTHOR CLAIM: Castaño calls rgbcx "a close competitor and most likely a better choice if you are looking for a well supported and actively developed library" — [S1](https://github.com/castano/icbc)
- SOURCE CODE FACT: the ISPC BC1 kernel `CompressBlockBC1_core` hard-codes `powerIterations = 4` and `refineIterations = 1`: covariance, principal axis by power iteration, endpoint pick, one least-squares refinement. `CompressBlocksBC1(const rgba_surface*, uint8_t*)` takes no settings struct, so there are no BC1 quality levels and no channel weights in ispc_texcomp — [S8 kernel.ispc / ispc_texcomp.h](https://github.com/GameTechDev/ISPCTextureCompressor)
- VENDOR DOCUMENTATION: Intel has discontinued ISPCTextureCompressor ("This project will no longer be maintained by Intel"); licence MIT — [S8](https://github.com/GameTechDev/ISPCTextureCompressor)
- BLOG (author description): stb_dxt computes average and covariance, takes the principal component as the endpoint direction, then refines; Giesen describes it as "a simple BC1 encoder that gives decent quality" and fast enough for per-session baking — [S7](https://fgiesen.wordpress.com/2022/11/08/whats-that-magic-computation-in-stb__refineblock/)
- SOURCE CODE FACT: stb_dxt `STB_DXT_HIGHQUAL` "does two refinement steps instead of 1. ~30-40% slower" — [S6](https://github.com/nothings/stb/blob/master/stb_dxt.h)
- AUTHOR CLAIM: rgbcx levels 0-4 "are fast and compete against stb_dxt (default and HIGHQUAL)", the remaining levels "compete against squish/NVTT/icbc and icbc HQ"; recommended default is level 10 with `allow_3color` true and `use_transparent_texels_for_black` false; 3-colour modes are not used below level 5 — [S3](https://github.com/richgel999/bc7enc_rdo/blob/master/rgbcx.h)
- AUTHOR CLAIM: using 3-colour blocks for black/very dark texels raises average quality "substantially for my 100 texture corpus (~.5 dB)"; plain 3-colour mode gives "a small but noticeable gain in average quality, but lower perf"; iterative cluster fit (squish's option) is "not really worth the tiny boost in quality" — [S3](https://github.com/richgel999/bc7enc_rdo/blob/master/rgbcx.h)
- AUTHOR CLAIM (constraint): with `use_transparent_texels_for_black` "Your engine/shader MUST ignore the sampled" alpha — the black texels decode as transparent black — [S3](https://github.com/richgel999/bc7enc_rdo/blob/master/rgbcx.h); Castaño gives the same warning: "the resulting texture may have unexpected alpha values ... we have to be careful the shaders don't assume opaque alpha" — [S2](https://www.ludicon.com/castano/blog/?p=1359)
- LAMINAR STATEMENT (directly relevant to the previous item): "X-Plane will treat DXT1 as having alpha if there is at least one transparent pixel; it will treat the image as not having an alpha channel if there are no transparent pixels" — [S17](https://developer.x-plane.com/docs/scenery/ddstool-manual/)
- AUTHOR CLAIM: rgbcx's "prioritized cluster fit" is "3-4x faster than traditional cluster fit (as implemented in libsquish with SSE2) at the same or slightly higher average quality", "faster than both AMD Compressonator and libsquish at the same average quality", and "suitable for GPU encoder implementations"; rgbcx is public domain or MIT — [S4](https://github.com/richgel999/bc7enc), [S3](https://github.com/richgel999/bc7enc_rdo/blob/master/rgbcx.h)
- BLOG (author explanation): cluster fit (Simon Brown / libsquish) evaluates the 969 order-preserving index clusterings per block with least squares; "the 3 color mode rarely produces higher quality results than the 4 color mode ... unless you want to squeeze as much quality as possible"; ICBC supports per-texel weights (useful for alpha maps, lightmaps, RGBM, and potentially perceptual weight maps) — [S2](https://www.ludicon.com/castano/blog/?p=1359)
- BLOG OPINION: Castaño: "The BC1 format is not particularly relevant today" (a reader comment disagrees) — [S2](https://www.ludicon.com/castano/blog/?p=1359)
- SOURCE CODE FACT / LAMINAR: Laminar's own DDSTool compresses with libsquish using `kColourIterativeClusterFit` (the slowest, highest-quality squish colour mode); a source comment says "libsquish runs at 1+ Mpixels/sec" — [S18 BitmapUtils.cpp](https://github.com/X-Plane/xptools). Supnik: "DDSTool sets libsquish for maximum quality and then takes quite a while to grind each texture" — [S13](https://developer.x-plane.com/2012/01/dds-revisited-in-x-plane-10/)
- VENDOR DOCUMENTATION: NVTT 3 is a CUDA-accelerated SDK for BC1-7 and LDR ASTC with CPU fallbacks; release note 3.2.2: "BC1-BC3, BC1a: Improved compression quality at Quality::Production|Highest". Distributed as a binary SDK for Windows and Linux (no quality numbers on the page) — [S21](https://developer.nvidia.com/gpu-accelerated-texture-compression)
- AUTHOR CLAIM: Betsy's GPU BC1 is "Based on stb_dxt"; the project is "WIP", GLSL compute, OpenGL — [S11](https://github.com/darksylinc/betsy)
- VENDOR DOCUMENTATION (Rust availability): `texpresso` 2.0.2 is "a pure Rust port of Simon Brown's libsquish" supporting BC1-BC5 (no BC7), MIT — [S12](https://github.com/jansol/texpresso); `intel_tex_2` exposes BC1/BC3/BC4/BC5/BC6H/BC7/ETC1, MIT/Apache-2.0 — [S9](https://github.com/Traverse-Research/intel-tex-rs-2)
- PROJECT FACT: XEarthLayer's ISPC path calls `intel_tex_2::bc1::compress_blocks` / `bc3::compress_blocks`; `DdsFormat` has only `BC1` and `BC3`; the GPU path maps to `CompressionVariant::BC1` / `BC3` — [S22 compressor.rs, types.rs, gpu_channel.rs](https://github.com/samsoir/xearthlayer)

### Inferences
- OWN INFERENCE: the ISPC BC1 kernel (PCA + 1 refinement, 4-colour mode only) should land near the stb_dxt points of the ICBC chart, i.e. roughly 0.8-1.0 dB (RGB PSNR, Kodim+Waterloo) below the best BC1 encoders. Not measured anywhere I could open. Note the task brief said "a few refinement iterations"; the kernel source says exactly one.
- OWN INFERENCE: about 1 dB at about 30 dB average is a small difference; since the project's own benchmark found BC1 before/after agreeing within 0.002 SSIM relative to filter differences, an encoder upgrade worth at most about 1 dB PSNR is unlikely to be measurable in that benchmark's terms. This is a plausibility argument, not a demonstrated result.
- OWN INFERENCE: the cheapest step up the ladder is ICBC with AVX-512 or rgbcx at a low-mid level: on the chart ICBC-AVX512 reaches about 30.8 dB at about 3-4x the stb time, and rgbcx reaches about 30.5 dB at about 2x the stb time. Scalar/AVX2 machines pay much more (ICBC-Scalar about 4 time units for 30.8 dB, about 15-20x stb). Relative to a 42 ms tile this is tens to hundreds of ms for level 0, i.e. outside the stated budget unless restricted to small mips (see section 6).
- OWN INFERENCE: `use_transparent_texels_for_black` / ICBC `three_color_black` (the source of the roughly 0.5 dB claim) is risky for X-Plane: per the DDSTool manual X-Plane treats a DXT1 file with at least one transparent pixel as having alpha, so dark texels in shadows or water could be drawn as holes or change the texture's alpha classification. Should be assumed unusable unless tested in the sim. Plain 3-colour mode without transparent texels is safe but gives only "a small" gain.
- OWN INFERENCE: no pure-Rust or crates.io binding for rgbcx or icbc was verified; both are single-header C++ and would need a `cc` build + FFI. texpresso (pure Rust libsquish port) is available but libsquish sits mid-ladder at high cost on the chart (about 30.42 dB at about 7.3 time units, about 25-35x stb time).

### Gaps
- No opened source places Intel ISPC BC1 on the same PSNR axis as stb/rgbcx/icbc/squish. Encode speeds in Mpix/s for the ladder were not obtained (the ICBC chart's time axis has no unit).
- No opened source reports SSIM or a perceptual metric (FLIP, LPIPS) across BC1 encoders, and none tests satellite/aerial imagery. Kodim/Waterloo are natural photographs, not orthophotos at 2.4 m/pixel after JPEG q75-85.
- No opened source quantifies luma-weighted (perceptual) BC1 error on photographs. rgbcx/bc7enc document perceptual weighting only for BC7 (section 3); ISPC BC1 has no weights.
- BC1 failure cases: the opened Wikipedia text only documents banding on gradients from 16-bit interpolation on old GeForce cards; I did not open a source that describes RGB565 endpoint banding / green-purple tint on smooth gradients or the more-than-two-colour-axes block failure. These are widely stated but remain unsourced here.
- "etsy" in the brief: no encoder of that name found; presumably a duplicate of Betsy.
- Unverified (not opened): Aras Pranckevičius, "Texture Compression in 2020" (aras-p.info, Mpix/s vs Luma-PSNR for ISPC, rgbcx, stb_dxt, icbc, Compressonator, NVTT); Geldreich's blog (richg42.blogspot.com, April 2020), where a search snippet reports stb_dxt about 35.75 dB vs best rgbcx variant about 35.93 dB in one test and says the CPU BC1 Pareto frontier is "defined by ispc_texcomp at low quality and by rgbcx at any higher quality level". Both are snippet-only.

## 2. Dithering / error diffusion before BC1 to hide RGB565 banding

### Takeaway
The one encoder known for a dithering option, stb_dxt, removed it: the flag is documented as "was always dubious, now deprecated. does nothing!" and the removal is attributed to Giesen (ryg) in v1.11. I found no opened source with a measured benefit of dithering for BC1, so no gain is demonstrated.

### Cited Findings
- SOURCE CODE FACT: stb_dxt version history: "v1.11 - (ryg) avoid racy global init, better single-color tables, remove dither" — [S6](https://github.com/nothings/stb/blob/master/stb_dxt.h)
- SOURCE CODE FACT / AUTHOR OPINION: `#define STB_DXT_DITHER 1 // use dithering. was always dubious, now deprecated. does nothing!` — [S6](https://github.com/nothings/stb/blob/master/stb_dxt.h)
- SOURCE CODE FACT: neither `ispc_texcomp.h` (BC1 entry point has no settings) nor the rgbcx public API (`encode_bc1(level, dst, pixels, allow_3color, use_transparent_texels_for_black)`) exposes a dithering option — [S8](https://github.com/GameTechDev/ISPCTextureCompressor), [S3](https://github.com/richgel999/bc7enc_rdo/blob/master/rgbcx.h)
- AUTHOR DESCRIPTION: neither the ICBC README nor Castaño's "BC1 Compression Revisited" mentions dithering among ICBC's algorithms (box fit, least squares, cluster fit, iterative refinement, summed-area tables) — [S1](https://github.com/castano/icbc), [S2](https://www.ludicon.com/castano/blog/?p=1359)
- SOURCE CODE FACT: Laminar's DDSTool calls `squish::CompressImage` with only the format flag and `kColourIterativeClusterFit`; no dithering step precedes it — [S18](https://github.com/X-Plane/xptools)

### Inferences
- OWN INFERENCE: the user's question "did Giesen remove/criticise stb_dxt dithering?" is confirmed as to removal (v1.11, credited to ryg) and as to the judgement "always dubious" written in the header; the header does not explain why.
- OWN INFERENCE: a BC1 block can hold only four colours on one line in RGB space, so per-pixel error diffusion inside a block can only choose among those four values. Dither noise injected before encoding also widens each block's colour spread, which pulls the endpoints apart. Whether the net effect on smooth fields/water/snow is better or worse is not established by any opened source. If tried, it should be an A/B test on the project's own benchmark, and against the source imagery's own JPEG noise, which may already act as dither.

### Gaps
- No measured numbers (PSNR/SSIM or subjective) for dithered vs undithered BC1 were found.
- Whether libsquish or NVTT offer any dithering/error-diffusion option was not verified from their documentation (NVTT 3 page opened, no mention found; libsquish docs not opened).
- Giesen's reasoning for the removal (commit message or blog post) was not found; his blog search for "stb_dxt" returned only the 2022 RefineBlock post, which does not discuss dithering.

## 3. BC7 instead of BC1: quality gain, size, encode cost, and X-Plane 12 compatibility

### Takeaway
BC7 is clearly a higher-quality format (ispc_texcomp BC7 reaches 42.7-46.8 dB RGB PSNR on a 31-image photo corpus) at exactly twice BC1's size, and both encoders the project already links can produce it, but I found no Laminar statement that X-Plane 12 loads BC7 DDS: the developer site has zero hits for "BC7" or "BC5", every Laminar text opened speaks only of DXT1/3/5, and Laminar's own DDSTool source reads only DXT1/3/5 and has BC4/BC5 output commented out with "todo: figure out what XP12 wants". Compatibility is unproven and must be tested in the sim before any work is spent.

### Cited Findings
- LAMINAR STATEMENT: "X-Plane only uses DDS files for DXT" (Ben Supnik, author comment, 2012-02-04) — [S14](https://developer.x-plane.com/?p=3970)
- LAMINAR STATEMENT: DDSTool's documented output formats are DXT1, DXT3, DXT5 and uncompressed RGB; uncompressed "is NOT recommended for scenery use; there are only a few very special cases where X-Plane will allow an uncompressed DDS file" — [S17](https://developer.x-plane.com/docs/scenery/ddstool-manual/)
- LAMINAR STATEMENT: for orthophotos "via either a .pol or .ter with the LOAD_CENTER directive, use DDS and only DDS" (X-Plane 10 era) — [S13](https://developer.x-plane.com/2012/01/dds-revisited-in-x-plane-10/)
- MEASURED (site search): developer.x-plane.com returns "No results found" for "BC7" and for "BC5" (queried 2026-11-27) — [S19](https://developer.x-plane.com/?s=BC7)
- SOURCE CODE FACT: in xptools master, DDSTool's menu and usage text list `--png2dxt1/3/5`, `--png2dxt` (auto BC1/BC3) and `--png2rgb`; `--png2bc4` and `--png2bc5` exist only as commented-out lines; the DDS writer has BC4/BC5 FourCC cases under the comment "todo: figure out what XP12 wants ..."; the DDS reader accepts only FourCC `DXT1`, `DXT3`, `DXT5` (anything else bails); `BitmapUtils.h` defines DXGI enum constants for BC4, BC5 and BC7 (`DXGI_FORMAT_BC7_UNORM = 98`, `_SRGB = 99`) but no BC7 code path writes or reads them — [S18](https://github.com/X-Plane/xptools)
- VENDOR/REFERENCE: BC7 "encodes 16 input RGB8/RGBA8 pixels into 128 bits of output" (8 bpp vs BC1's 64 bits per block, 4 bpp), was added in Direct3D 11, "the quality is much better as a result" of the mode selection, and BC6H/BC7 are specified more exactly than BC1-5, which "decode slightly differently among GPU vendors" — [S20](https://en.wikipedia.org/wiki/S3_Texture_Compression)
- MEASURED (author benchmark, 31 images "kodim corpus+others", total encode time for the corpus, machine not stated): ispc_texcomp BC7 average RGB PSNR 42.7 dB at 1.9 s (ultrafast), 46.1 dB at 41.5 s (fast), 46.5 dB at about 100 s (basic), 46.8 dB at 355.4 s (slow); Rec.709 luma PSNR 46.2 dB (ultrafast), 48.0 (fast), 48.3 (basic), 48.6 (slow) — [S4](https://github.com/richgel999/bc7enc)
- MEASURED (same table): bc7enc16 (modes 1+6 only, scalar) 42.7 dB RGB at 3.8 s (mode 6 only), 45.5 dB at 19.8 s, 46.1 dB at 51 s; in perceptual mode 48.4 dB luma at 8.9 s and 50.0 dB luma at 122.6 s — [S4](https://github.com/richgel999/bc7enc)
- AUTHOR CLAIM: bc7enc output "is far higher quality than any BC1 encoder"; "Fast mode 6-only encoders will have noticeable block artifacts which this codec avoids by fully supporting mode 1"; most BC7 encoders use linear RGB metrics, "which is a fundamental weakness", and bc7enc computes error in weighted YCbCr — [S4](https://github.com/richgel999/bc7enc)
- AUTHOR CLAIM: bc7e.ispc supports all BC7 modes; a faster real-time analytical encoder "bc7f" (2026) is announced as an alternative; bc7e has been ported to plain C++ in basis_universal — [S5](https://github.com/richgel999/bc7enc_rdo)
- SOURCE CODE FACT: ispc_texcomp exposes BC7 profiles `GetProfile_ultrafast / veryfast / fast / basic / slow` (plus alpha variants) and `CompressBlocksBC7(src, dst, settings)`; `intel_tex_2` lists BC7 as supported — [S8](https://github.com/GameTechDev/ISPCTextureCompressor), [S9](https://github.com/Traverse-Research/intel-tex-rs-2)
- VENDOR DOCUMENTATION: `block_compression` supports BC1, BC2, BC3, BC4, BC5, BC6H and BC7 (WGSL port of the ISPC kernels; tested on DX12, Metal, Vulkan); `BC7Settings` offers `opaque_ultra_fast / opaque_very_fast / opaque_fast / opaque_basic / opaque_slow` and alpha presets; "The pipeline creation for BC7 and especially BC6H takes a long time under DX12", so they sit behind cargo features; MIT; latest 0.11.0 (2026-10-24) — [S10](https://github.com/hasenbanck/block_compression)
- PROJECT FACT: XEarthLayer depends on `block_compression = { version = "0.10", default-features = false, features = ["bc15", "wgpu"] }`, i.e. the BC7 feature is currently compiled out — [S22 Cargo.toml](https://github.com/samsoir/xearthlayer)
- LAMINAR STATEMENT (context): DDS is "virtually always used with a compressed image format (like DXT1 or DXT5)"; if the user unchecks texture compression, "DDS files remain compressed" — [S16](https://developer.x-plane.com/2010/09/revisiting-texture-compression/)

### Inferences
- OWN INFERENCE (size): BC7 is 8 bpp, so a 4096 tile with mips goes from 10.66 MiB to about 21.3 MiB on disk, in XEL's memory cache and in VRAM. A DDS file carries one pixel format for all mip levels, so "BC7 for level 0 only" is not expressible in a single DDS; it is all levels or none.
- OWN INFERENCE (gain): no opened source measures BC1 and BC7 on the same corpus. Putting the two opened benchmarks side by side (BC1 about 30-31 dB on Kodim+Waterloo in [S1]; BC7 about 42.7-46.8 dB on Kodim+others in [S4]) suggests a gap above 10 dB, but the corpora and PSNR conventions differ (the ICBC number may be computed differently from Geldreich's RGB PSNR), so this is only an order-of-magnitude indication.
- OWN INFERENCE (cost): in [S4] ispc_texcomp "ultrafast" is the only profile in real-time territory (1.9 s for 31 images vs 41.5 s for "fast", about 22x). Mpix/s cannot be derived because image sizes and thread count are not stated. For XEL the realistic BC7 candidates are ISPC ultrafast/veryfast on CPU or the wgpu path's `opaque_ultra_fast` / `opaque_very_fast`; the per-tile cost must be measured locally.
- OWN INFERENCE (compatibility): the evidence leans against assuming BC7 works. Laminar's last explicit statement is DXT-only (2012); the developer site never mentions BC7; Laminar's own tool chain neither writes nor reads it and treats even BC4/BC5 for XP12 as an open "todo". X-Plane 12's Vulkan/Metal renderer could technically sample BC7 on desktop GPUs, but whether its DDS loader accepts a DX10 header with `DXGI_FORMAT_BC7_UNORM(_SRGB)` for `.ter` orthophoto textures is not documented. A one-tile experiment in the sim (hand-converted BC7 DDS) would settle it cheaply. Until then: not a supported format.
- OWN INFERENCE (Apple Silicon / ASTC / ETC): no Laminar text opened mentions ASTC or ETC DDS input. xptools contains PVRTC/ATC code only under `PHONE` builds (mobile product). Since XEL must emit files X-Plane's desktop loader accepts on all platforms, ASTC/ETC are not relevant unless Laminar documents them; nothing found.

### Gaps
- No Laminar statement, release note or documentation about BC7 (or BC4/BC5) support in X-Plane 12 was found on developer.x-plane.com; www.x-plane.com and the forums could not be consulted. The claim "X-Plane 12 added BC4/BC5 for normal maps" is NOT confirmed by anything I opened; the xptools comment suggests the tooling side was unfinished.
- No same-corpus BC1-vs-BC7 PSNR/SSIM numbers; no BC7 Mpix/s for ISPC ultrafast/veryfast or for `block_compression` BC7.
- Whether BC7 at ultrafast (mostly mode 6) beats a good BC1 enough to justify 2x memory on already-JPEG-compressed orthophotos is untested.
- Unverified (not opened): Aras Pranckevičius' 2020 comparison (BC7 encoders Mpix/s vs quality); Geldreich's bc7f post (richg42.blogspot.com, 2026-01); Intel's "Fast ISPC Texture Compressor" article (www.intel.com, blocked).

## 4. YCoCg-DXT5 (van Waveren & Castaño 2007)

### Takeaway
Not applicable: the technique stores a transformed colour space (luma in the DXT5 alpha block, chroma in the colour block) that the sampling shader must convert back to RGB, and XEarthLayer cannot change X-Plane's terrain shader. This is my reasoning; the paper itself could not be downloaded here.

### Cited Findings
- AUTHOR CITATION: the ICBC README cites "Real-Time-YCoCg-DXT-Compression" (NVIDIA whitepaper, 2007) as the source of ICBC's fast box-fitting DXT encoder — [S1](https://github.com/castano/icbc)
- LAMINAR STATEMENT: X-Plane "loads the DDS" as a pre-compressed texture; DXT5 is documented as RGB plus "a compressed alpha channel" used for translucency — [S17](https://developer.x-plane.com/docs/scenery/ddstool-manual/)
- AUTHOR CLAIM (analogous constraint): tricks that repurpose BC1/BC3 channels require engine cooperation — e.g. rgbcx's transparent-black option: "Your engine/shader MUST ignore the sampled" alpha — [S3](https://github.com/richgel999/bc7enc_rdo/blob/master/rgbcx.h)

### Inferences
- OWN INFERENCE: a YCoCg-DXT5 file fed to X-Plane as a normal DXT5 would be displayed with wrong colours and interpreted as having a varying alpha channel. It also costs 8 bpp, the same as BC7. Rule out.

### Gaps
- Unverified (not opened): J.M.P. van Waveren, I. Castaño, "Real-Time YCoCg-DXT Compression", NVIDIA, 2007 (developer.download.nvidia.com unreachable). Its quality numbers (PSNR gain over DXT1) are therefore not quoted.

## 5. Rate-distortion-optimised encoders and "smooth block" handling (bc7enc_rdo)

### Takeaway
RDO encoders trade quality away for smaller LZ-compressed files; they cannot raise fidelity and only matter for disk size of an LZ-compressed cache, which XEarthLayer's DDS cache is not.

### Cited Findings
- AUTHOR CLAIM: bc7enc_rdo applies "a simple post-processing transform on the encoded texture data designed to trade off quality for smaller compressed file sizes using LZ compression. Significant (10-50%) size reductions are possible"; the BC7 reduced-entropy mode gives "5-10% smaller file sizes" with minimal quality impact — [S5](https://github.com/richgel999/bc7enc_rdo)
- AUTHOR CLAIM: the RDO uses the Lagrangian multiplier method "modified to favor MSE on very smooth blocks"; an option disables "ultra-smooth block handling"; "The next major focus will be improving the default smooth block handling" — [S5](https://github.com/richgel999/bc7enc_rdo)
- REFERENCE: Oodle Texture is "a lossy preprocessor that modifies the input texture so that the BCn output is more easily compressed by a LZ77 compressor (rate-distortion optimization)" — [S20](https://en.wikipedia.org/wiki/S3_Texture_Compression)

### Inferences
- OWN INFERENCE: the smooth-block special case in bc7enc_rdo exists because RDO damage is most visible on smooth blocks; it is evidence that smooth regions are where block-compression errors are noticed, but it is not a quality-raising feature for a non-RDO pipeline. Relevant to XEL only if the DDS disk cache were ever stored LZ/zstd-compressed.

### Gaps
- No numbers for RDO BC1 on photographic data were opened.

## 6. Encoding only some mip levels with a slower/better profile: cost arithmetic

### Takeaway
Level 0 holds 75% of the pixels, so a better encoder restricted to levels 1 and up costs a quarter of applying it everywhere, and restricted to levels 2 and up about 6%; whether the better blocks are visible on minified, trilinearly filtered mips is not shown by any source.

### Cited Findings
- PROJECT FACT (from the task brief and CLAUDE.md, not independently measured here): 4096x4096 tile, 5-level mip chain, about 42 ms per tile with mips on one core with the ISPC BC1 kernel; tile size 10.66 MiB — [S22](https://github.com/samsoir/xearthlayer)
- MEASURED (relative cost of better BC1 encoders, chart time units): stb about 0.2-0.3; rgbcx about 0.5 for about 30.5 dB, about 1.1 for about 30.7 dB, about 3.8 for about 30.8 dB; ICBC-AVX512 about 0.9 for about 30.8 dB, about 5.5 for about 30.9 dB; squish about 7.3 — [S1 chart](https://raw.githubusercontent.com/castano/icbc/master/doc/kodim-chart.png)

### Inferences
- OWN INFERENCE (arithmetic): pixels per level for 4096/2048/1024/512/256 are 16.78 / 4.19 / 1.05 / 0.26 / 0.066 Mpix, total 22.35 Mpix; level 0 = 75.1%, level 1 = 18.8%, levels 2-4 = 6.1%. With 42 ms total, level 0 is about 31.5 ms, level 1 about 7.9 ms, levels 2-4 about 2.6 ms.
- OWN INFERENCE: an encoder k times slower than ISPC BC1 adds (k-1) x 31.5 ms on level 0, (k-1) x 7.9 ms on level 1, (k-1) x 2.6 ms on levels 2-4. For k = 4 (roughly ICBC-AVX512 at 30.8 dB or rgbcx at about 30.7 dB, if ISPC is taken as stb-speed, which is an assumption): +95 ms / +24 ms / +8 ms. For k = 10: +284 ms / +71 ms / +23 ms. So within "a few ms to low tens of ms", only levels 1 and up at k of about 2-4, or levels 2 and up at k up to about 10, fit. Level 0, where the aircraft-near detail lives, does not fit on CPU.
- OWN INFERENCE: this split helps least where it matters most: X-Plane shows level 0 near the aircraft and higher levels at distance, where minification and filtering average block errors anyway. The expected visible benefit of better small-mip encoding is therefore low; no source measures it.
- OWN INFERENCE: ISPC's actual speed is likely well above scalar stb_dxt (it is SIMD-vectorised across blocks), so the k factors relative to ISPC are probably larger than the chart's stb-relative ratios. Needs a local benchmark.

### Gaps
- No published measurement of per-mip-level encoder quality effects. No Mpix/s for ICBC/rgbcx on the project's hardware.

## 7. GPU real-time encoders (van Waveren 2006, Castaño CUDA, Narkowicz BC6H, Betsy) vs ISPC

### Takeaway
The GPU BC1 encoders I could open are ports of the same fast CPU algorithms (Betsy from stb_dxt, `block_compression` from the ISPC kernels), so they offer throughput, not higher BC1 quality; the only GPU-side quality step verified as available to the project is switching the wgpu path to BC7 (section 3).

### Cited Findings
- AUTHOR CLAIM: Betsy BC1 is "Based on stb_dxt", BC3 is "BC1 for RGB + BC4 for alpha", BC6H is a "GLSL port of GPURealTimeBC6H" (Narkowicz); the project is WIP, requires compute shaders, runs on OpenGL; expected quality is the same as the originals "in theory", with possible float-precision differences — [S11](https://github.com/darksylinc/betsy)
- VENDOR DOCUMENTATION: `block_compression` shaders "are a port of Intel's ISPC Texture Compressor's kernel to WGSL compute shader" — [S10](https://github.com/hasenbanck/block_compression)
- AUTHOR CLAIM: rgbcx's prioritized cluster fit "is suitable for GPU encoder implementations" — [S4](https://github.com/richgel999/bc7enc)
- BLOG (author description): Castaño's CUDA cluster-fit encoder solved one least-squares system per thread over the precomputed 969 cluster combinations; ICBC's SIMD version reuses that idea with summed-area tables — [S2](https://www.ludicon.com/castano/blog/?p=1359)
- VENDOR DOCUMENTATION: NVTT 3 offers CUDA compression for BC1-BC7 and ASTC with CPU fallback, including an "On-The-Fly API" that compresses "entirely within GPU memory"; binary SDK, Windows and Linux, CUDA (NVIDIA GPUs) — [S21](https://developer.nvidia.com/gpu-accelerated-texture-compression)

### Inferences
- OWN INFERENCE: a higher-quality GPU BC1 (cluster fit over 969 or a pruned set of orderings per block, as in Castaño's CUDA encoder or rgbcx) is feasible as a WGSL compute shader but does not exist in the `block_compression` crate; it would be new shader work for at most about 1 dB PSNR (section 1). NVTT 3 is not an option for a cross-platform open-source Rust binary (CUDA-only acceleration, proprietary SDK, no macOS).
- OWN INFERENCE: because the GPU the encoder would use is usually the one X-Plane renders with (the project already warns about this), a heavier GPU encode pass competes with the simulator; the stated "or a GPU pass" budget applies only to hosts with a second GPU.

### Gaps
- Unverified (not opened): J.M.P. van Waveren, "Real-Time DXT Compression" (Intel, 2006); I. Castaño, "High Quality DXT Compression using CUDA" (NVIDIA, 2007); K. Narkowicz, "GPU Real-Time BC6H" (repository named in the Betsy README but not opened). No quality numbers for any of these vs ISPC were obtained.
- No measured PSNR for `block_compression` output vs the CPU ISPC kernel (bit-exactness or drift unknown).

## 8. Counter-evidence: are encoder upgrades invisible in busy photographic textures?

### Takeaway
I could not open Griffin & Olano's studies, so the masking argument remains unverified here; the only opened evidence on visibility is Laminar's qualitative statement that slow offline DXT compression looks clearly better than fast driver compression on a livery (flat colours and sharp edges, the opposite of noisy terrain), plus the project's own finding that BC1 is not measurable against filter differences.

### Cited Findings
- LAMINAR STATEMENT (qualitative, with pictures): driver-compressed livery "just looks awful ... it has done a fast job, not a high quality job"; the libsquish-max-quality version makes "better use of those compressed bits"; "the compressed version is a better way to save VRAM than down-sizing" — [S13](https://developer.x-plane.com/2012/01/dds-revisited-in-x-plane-10/)
- LAMINAR STATEMENT: "Sometimes DDS is relatively harmless to a texture, and sometimes it just absolutely destroys it. I don't have a good answer to this" — [S15](https://developer.x-plane.com/?p=4593)
- LAMINAR STATEMENT: "If the image file contains details that don't compress well, they're going to get splatted and stay splatted" — [S16](https://developer.x-plane.com/2010/09/revisiting-texture-compression/)
- PROJECT FACT (from the task brief): the project's benchmark found before/after BC1 agreeing within 0.002 SSIM relative to filter differences; encoders were not compared — task brief, not re-measured.
- AUTHOR CLAIM (indirect): bc7enc_rdo biases its RDO "to favor MSE on very smooth blocks", i.e. its author treats smooth blocks as the place where errors show — [S5](https://github.com/richgel999/bc7enc_rdo)

### Inferences
- OWN INFERENCE: Laminar's "looks awful vs. looks fine" example compares a 2012 GPU driver's on-the-fly compressor with maximum-quality cluster fit on a synthetic livery (sharp coloured edges, text, gradients). It shows that encoder quality can be visible on such content; it does not show that the roughly 1 dB between a PCA+refine encoder and cluster fit is visible on noisy orthophotos.
- OWN INFERENCE: taken together (ladder only about 1 dB wide; project SSIM delta of 0.002; texture content is JPEG-compressed, noisy, viewed under minification and motion), the default expectation should be that a BC1 encoder upgrade is not visible in flight. Smooth low-detail areas (water, snow, uniform fields) are the only plausible place for a visible gain, and that is exactly where no measurement exists.

### Gaps
- Unverified (not opened; host unreachable or cookie wall): W. Griffin, M. Olano, "Objective Image Quality Assessment of Texture Compression", ACM I3D 2014; W. Griffin, M. Olano, "Evaluating Texture Compression Masking Effects Using Objective Image Quality Assessment Metrics", IEEE TVCG 21(8), 2015, DOI 10.1109/TVCG.2015.2429576. A search summary says they argue artefacts are often masked by texture mapping and multi-texture combination and that "current GPU compression is overly conservative"; this is snippet-level only. Also not opened: the HAL-hosted "A Psychophysical Evaluation of Texture Compression Masking Effects" (TVCG 2018).
- No study of BC1 artefact visibility specifically on aerial/satellite imagery was found.

## Summary table (for the report writer)

| Option | Demonstrated gain (opened source) | Cost | Implementation / licence / Rust | X-Plane compatibility | Verdict |
|---|---|---|---|---|---|
| Keep ISPC BC1 (current) | baseline; kernel = PCA + 1 LS refinement, no quality knob [S8] | about 42 ms/tile (project) | `intel_tex_2`, MIT/Apache [S9]; upstream discontinued [S8] | DXT1 documented [S17] | baseline |
| rgbcx higher level / ICBC, 4-colour + safe 3-colour | up to about +1.05 dB PSNR over stb-class on Kodim+Waterloo [S1]; ISPC position inferred | about 2-4x stb time for +0.6 to +0.95 dB, much more for the last 0.1 dB [S1] | C++ single headers; rgbcx PD/MIT [S3]; no Rust crate verified | same BC1 files | no visible gain demonstrated; fits budget only on small mips |
| "Transparent texels for black" (3-colour black) | about +0.5 dB on author's corpus [S3] | small | rgbcx / ICBC | risky: X-Plane treats DXT1 with any transparent pixel as alpha [S17] | avoid unless sim-tested |
| libsquish / texpresso cluster fit | about 30.42 dB vs stb about 29.85-30.06 [S1] | about 25-35x stb time [S1]; "1+ Mpixels/sec" iterative in DDSTool [S18] | texpresso pure Rust, MIT, BC1-5 [S12] | same BC1 files; this is what Laminar's DDSTool uses [S18] | too slow, dominated by rgbcx/ICBC |
| Dithering before BC1 | none; stb_dxt removed it as "always dubious" [S6] | small | would be custom | same BC1 files | no gain demonstrated |
| BC7 (all levels) | BC7 42.7-46.8 dB RGB PSNR on photos [S4]; no same-corpus BC1 comparison | 2x size (8 bpp) [S20]; ultrafast profile about 22x faster than "fast" [S4] | `intel_tex_2` BC7 profiles [S8][S9]; `block_compression` BC7 behind feature, slow DX12 pipeline creation [S10] | NOT documented by Laminar; site has no BC7/BC5 mention [S19]; Laminar tools read DXT1/3/5 only [S18] | largest potential gain, blocked on compatibility test and memory |
| YCoCg-DXT5 | paper not opened | 8 bpp | — | needs shader decode; not possible (inference) | not applicable |
| RDO (bc7enc_rdo) | trades quality for 10-50% smaller LZ size [S5] | slower | C++ | same formats | irrelevant for fidelity |
| Better encoder on mips 1+ / 2+ only | none measured | +(k-1) x 7.9 ms / 2.6 ms (arithmetic) | as above | same | cheap but benefit unproven |
| GPU BC1 encoders (Betsy, block_compression) | same algorithms as stb / ISPC [S10][S11] | fast | block_compression MIT (in use) | same | throughput only, no quality gain |
