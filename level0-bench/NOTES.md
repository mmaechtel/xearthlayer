## Recommendation

**No PR yet. The synthetic data cannot say whether sharpening helps; only a run on real provider tiles can.**
What the synthetic run does establish:

1. **Whether unsharp masking helps depends on one property of the provider, not on the filter.**
   Where the provider's ZL16 is a plain average of its ZL18 (`synth-q60`, `q75`, `q90`), every unsharp
   strength loses on every tile (0.3: about -0.012 SSIM and -1.2 to -1.7 dB). Where the ZL16 is softer
   than that (`synth-q75-soft`, a 1-2-1 blur, sigma about 0.7 pixel), unsharp gains on every tile
   (0.3: +0.009 SSIM; 0.6 with q-deblock: +0.014 SSIM, +0.55 dB, butteraugli -0.37). The sign flips
   with the softness, so the first thing the real run has to answer, per provider, is how soft its
   ZL16 is against its own ZL18.
2. **Deblocking before sharpening is worth its cost when sharpening is on.** On the soft provider,
   `D q-deblock + usm 0.6` beats `B usm 0.6` (+0.0141 against +0.0131, and 12 of 12 tiles better against
   10 of 12). Farmland is where it matters: plain usm 0.6 gains nothing there (+0.0000), with q-deblock
   it gains +0.0031.
3. **Taking the strength from the quantisation tables does what it should.** At quality 90 the fixed
   threshold (C) is better on only 5 of 12 tiles and slightly negative on average, the table-driven
   one (D) is neutral. At quality 60 D
   gains more than C (+0.0014 against +0.0010). D never needs a per-provider setting.
4. **Deblocking alone is a small gain**: +0.001 SSIM at quality 60, +0.0005 at 75, nothing at 90.
   Farmland gains most (+0.004 at quality 60), forest loses (-0.002): fine texture has real steps at
   block edges that are below the threshold.
5. **Deringing is content-dependent and the most expensive stage.** At quality 60 it adds +0.004 on
   farmland and town and costs -0.006 on forest; at quality 90 it does nothing. Butteraugli gets
   slightly worse with it at quality 60 (+0.010) while SSIM improves. Not worth a first PR.
6. **BC1 does not eat the gain.** BC1 costs the baseline about 0.02 SSIM, but the differences between
   variants before and after BC1 agree to within 0.002 in every row.

If the real run confirms that a provider's ZL16 is soft, the variant to propose is
**`D q-deblock + usm`, off by default, with unsharp 0.3 as the default strength when enabled**: on the
soft provider 0.3 is better on 12 of 12 tiles, and on a provider that turns out not to be soft it does
the least damage of the sharpening variants. 0.6 gains more where it applies (+0.014 against +0.009)
but 1.0 already loses on coast and farmland, so 0.6 is the ceiling. For a provider whose ZL16 is not
soft, the only candidate is `D q-deblock` alone, and its gain is small enough that it has to be
weighed against its cost.

## Caveats per provider

Nothing here is measured on a real provider. What to look for when the real tiles are in:

- **Bing, Google (GO2), Apple**: recompressed pyramids; quality and softness per zoom are unknown
  until fetched. The manifest records the tables of every tile, and `Dataset` above shows whether a
  provider's quality varies between tiles (then D's per-chunk thresholds matter more than here,
  where every chunk of a provider has the same table).
- **ArcGIS, USGS**: mosaics of different sources and dates. ZL18 can be a different capture than
  ZL16, in which case the truth is not the same picture and every score is noise. Check the crops
  before trusting the table. USGS covers the US only, so it gets 6 of the 12 locations.
- **Any provider**: a ZL18 that is itself upsampled from coarser imagery makes the truth soft and
  rewards blur. Compare the truth crop with the baseline crop: the truth must show more detail.

## Method and choices made along the way

- **Branch base.** #458 is not merged into `develop/0.5.1`, so this branch starts from
  `fix/linear-light-mipmaps` (`develop/0.5.1` plus #458). The truth is reduced with that filter:
  `MipmapGenerator::generate_chain_with_count`, two 2x2 linear-light averages.
- **Decode path.** Chunks are decoded with the `image` crate, which is what
  `tasks::chunk_image::decode` runs (zune-jpeg with `image`'s options); XEL's fast path
  `decode_into` is tested to give the same bytes. That function is `pub(crate)`, so the tool calls
  the decoder it wraps rather than the function itself.
- **Chroma upsampling (idea 4).** zune-jpeg 0.5.13 already upsamples 4:2:0 chroma with the
  triangle filter (3:1 weights, libjpeg's "fancy upsampling") in both directions, not nearest
  neighbour. There is nothing cheap left to gain there, so it is not a variant.
- **Tile size.** A benchmark tile is 4 x 4 ZL16 chunks (1024 pixels) against 16 x 16 ZL18 chunks,
  not a full 4096 tile: a full tile needs 4,096 ZL18 chunks per location and provider. Timings are
  scaled by pixel count to 4096 x 4096.
- **Filters** work on stored sRGB bytes, R, G and B independently, with integer arithmetic. The
  deblocker uses the luminance table for all three channels. Thresholds: C fixed at 8 levels; D at
  1.5 times the step the chunk's table can cause (`JpegInfo::block_step`: 3, 7 and 12 levels at
  quality 90, 75 and 60);
  E adds a 3x3 sigma filter in blocks whose luma range is at least 32, up to 1.5 times the table's
  ringing amplitude (1, 3 and 4 levels). Unsharp is a 5x5 binomial blur (sigma about 1 pixel).
- **Not done:** quantisation-aware sharpening (idea 5), reconstruction as an upper bound (idea 6;
  zune-jpeg does not expose DCT coefficients) and a slower BC1 profile (idea 7). With one table per
  synthetic provider, idea 5 would only be a per-provider strength here; it needs real tiles with
  mixed quality to be tested.
- **Butteraugli** is the pure-Rust `butteraugli` crate (0.9, a port of libjxl's), 3-norm.
- **Synthetic scenes** are stationary: the two or three scenes of a content type score almost
  identically, so "12 tiles" is closer to 5 independent samples. The scenes are also cleaner than
  real imagery (no sensor noise, no haze, no resampling history).
- **Time.** The filters are unoptimised scalar code and far above the "few ms per tile" a shipped
  version needs: deblocking about 90 ms, unsharp about 240 ms, deringing about 270 to 560 ms per
  4096 tile on one core of this machine, against 37 ms for the BC1 encode of level 0. A shipped
  deblocker would touch only pixels beside block edges that fail the flatness test, and the blur
  vectorises well. Treat the times as an upper bound and as relative to each other.

## Network

Every tile server was refused by the sandbox's network policy when one chunk per provider was tried:
`ecn.t0.tiles.virtualearth.net` (Bing), `server.arcgisonline.com` (ArcGIS),
`basemap.nationalmap.gov` (USGS) and `duckduckgo.com` (Apple's token) answered with a proxy 403, and
`mt0.google.com` (GO2, plain HTTP) did not resolve. No workaround was tried. The repository has no
real imagery either (only eleven JPEG decoder fixtures), hence the synthetic dataset.

To run on real tiles: `scripts/level0-bench.sh probe`, then `scripts/level0-bench.sh real`. That
fetches 272 chunks per location and provider through `xearthlayer::provider`, caches them under
`level0-bench/data` and writes `level0-bench/results-real`.
