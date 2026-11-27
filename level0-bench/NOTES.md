## Recommendation

**No level 0 sharpening PR. On real tiles from Bing, ArcGIS and Apple, every sharpening variant moves
level 0 away from the ZL18 truth, and deblocking gains almost nothing.**

1. **Unsharp masking loses on all 36 tiles at every strength**, by SSIM, linear-light SSIM and PSNR,
   before and after BC1. 0.3 costs about -0.016 SSIM and -1.1 to -1.5 dB; even 0.1 loses on 36 of 36
   (about -0.0035 SSIM, -0.3 dB). Mountains and town lose most, farmland and coast least.
2. **The reason is in the `Detail vs truth` column.** The providers' ZL16 has 0.95 to 0.97 of the
   gradient energy of their own ZL18 reduced to the same size: it is barely softer than the truth.
   The synthetic softness sweep (`results-synth`) puts the break-even far below that: sharpening
   still loses at 0.89 and only gains at 0.66. Unsharp 0.1 brings the real tiles to a ratio of about
   1.0 and still loses, so what is missing is not contrast an unsharp mask can restore.
3. **Butteraugli disagrees mildly.** Its 3-norm improves with unsharp 0.1 to 0.6 (0.3: -0.12 to -0.20)
   and gets worse at 1.0. So a light sharpen may look closer to the truth to a perceptual metric
   while being further away pixel for pixel. That is a taste setting, not a fidelity gain, and it
   is not enough to make it a default.
4. **Deblocking is neutral.** The table-driven deblock (D) changes SSIM by at most +0.0003 and is
   better on 25 of 36 tiles; the fixed threshold (C) is better on 23 and slightly negative on Apple
   and Bing. At JPEG quality 79 to 85 there is little blocking to remove.
5. **Deringing adds +0.0002 to +0.0006** on 30 of 36 tiles, most on Bing forest (+0.0019). Real, but
   below anything visible, and it is the most expensive stage.
6. **Quantisation-aware sharpening (F) is worse than plain sharpening here**, because every real
   chunk is at or above the reference quality, so the rule only ever raises the strength.
7. **BC1 does not eat or add anything**: differences before and after BC1 agree to within 0.002.

If anything from this is worth carrying forward, it is the measuring tool and the `Detail vs truth`
figure, not a filter. A sharpening option could still be offered as an explicit taste setting
(off by default, 0.1 to 0.3, never above 0.6), but the data does not support calling it an
improvement in accuracy. The table-driven deblock would only pay off for a provider that serves
clearly lower quality than the three measured here.

## Caveats per provider

- **Bing** (quality 79, 4:2:0): the most uniform of the three. Nothing gains.
- **ArcGIS** (quality 75 to 85, mixed 4:2:0 and 4:4:4): the only provider where the fixed deblock is
  better on 12 of 12 tiles, by +0.0007. It is a mosaic, so the ZL16 and ZL18 of a location can come
  from different captures; the baseline SSIM is the lowest (0.898 after BC1), which fits that.
- **Apple** (quality 80, 4:2:0): closest to its truth (detail 0.97) and the provider where sharpening
  costs most.
- **USGS** is not measured: the service has no ZL18, so there is no truth. **GO2** is not measured:
  XEL fetches it over plain HTTP, which the sandbox this ran in does not route.
- **Twelve locations, one ZL16 area of 1024 pixels each.** Enough to see that the sign is the same
  everywhere, not enough to rank content types finely.
- **The truth is the provider's own ZL18**, itself a JPEG. It is the best available reference, not
  the ground.

## Method and choices made along the way

- **Branch base.** #458 is not merged into `develop/0.5.1`, so this branch starts from
  `fix/linear-light-mipmaps` (`develop/0.5.1` plus #458). The truth is reduced with that filter:
  `MipmapGenerator::generate_chain_with_count`, two 2x2 linear-light averages.
- **Decode path.** Chunks are decoded with the `image` crate, which is what
  `tasks::chunk_image::decode` runs (zune-jpeg with `image`'s options); XEL's fast path
  `decode_into` is tested to give the same bytes. That function is `pub(crate)`, so the tool calls
  the decoder it wraps rather than the function itself.
- **Fetching** goes through `xearthlayer::provider` (`ProviderFactory`), so URLs, headers and the
  Apple token flow are XEL's own.
- **Chroma upsampling (idea 4).** zune-jpeg 0.5.13 already upsamples 4:2:0 chroma with the
  triangle filter (3:1 weights, libjpeg's "fancy upsampling"), not nearest neighbour. Nothing cheap
  is left to gain there, so it is not a variant.
- **Tile size.** A benchmark tile is 4 x 4 ZL16 chunks (1024 pixels) against 16 x 16 ZL18 chunks,
  not a full 4096 tile, which would need 4,096 ZL18 chunks per location and provider. Timings are
  scaled by pixel count to 4096 x 4096.
- **Filters** work on stored sRGB bytes, R, G and B independently, with integer arithmetic. The
  deblocker uses the luminance table for all three channels. Thresholds: C fixed at 8 levels; D at
  1.5 times the step the chunk's table can cause (`JpegInfo::block_step`: 3, 7 and 12 levels at
  quality 90, 75 and 60); E adds a 3x3 sigma filter in blocks whose luma range is at least 32, up
  to 1.5 times the table's ringing amplitude. Unsharp is a 5x5 binomial blur (sigma about 1 pixel).
  F scales the unsharp amount per chunk by `clamp(step at quality 75 / chunk step, 0.25, 1.5)`.
  G (unsharp 0.1) was added after the first real run showed 0.3 overshooting.
- **Detail vs truth** is the mean absolute difference between neighbouring luma pixels of an image
  divided by the truth's. It is a softness indicator, not a quality score: artifacts raise it too.
- **Not done:** reconstruction as an upper bound (idea 6; zune-jpeg does not expose DCT
  coefficients) and a slower BC1 profile (idea 7). With no filter gaining before BC1, there was no
  cleaned-up input to test a slower profile on.
- **Butteraugli** is the pure-Rust `butteraugli` crate (0.9, a port of libjxl's), 3-norm.
- **Time.** Scalar Rust, one core: deblock about 40 ms, unsharp about 70 ms, deringing about 150 ms
  per 4096 tile on this machine, against 36 ms for the BC1 encode of level 0. Still above the few
  milliseconds a shipped filter should cost.
