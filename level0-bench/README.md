# level0-bench

An offline benchmark for the level 0 ideas in samsoir/xearthlayer#458: does deblocking,
deringing or sharpening the assembled tile bring ZL16 imagery measurably closer to the
ZL18 truth, per provider, after BC1?

It is a research tool. The filters live in this crate, nothing in XEL calls them, and the
runtime does not link it, so XEL's output is unchanged.

```bash
scripts/level0-bench.sh probe   # one chunk per provider: which tile servers answer
scripts/level0-bench.sh synth   # synthetic scenes, no network -> level0-bench/results
scripts/level0-bench.sh real    # fetch real chunks, then run  -> level0-bench/results-real
```

| Module | What it does |
|---|---|
| `dataset.rs` | Locations, providers (through `xearthlayer::provider`), on-disk cache, manifest, tile loading with XEL's decode and the #458 linear-light reduction of the truth |
| `jpeg_info.rs` | Quantisation tables, subsampling and a quality estimate from a JPEG header; the thresholds derived from the tables |
| `filters.rs` | `deblock`, `dering`, `unsharp`: integer, in place, deterministic |
| `variants.rs` | The 13 variants (A baseline to E deblock + dering + sharpen) |
| `bc1.rs` | Level 0 through `DdsEncoder` (ISPC) and a BC1 decode |
| `metrics.rs` | SSIM, linear-light SSIM, PSNR, butteraugli |
| `synth.rs` | Deterministic synthetic scenes for when no tile server is reachable |
| `report.rs` | `results.csv`, `summary.md`, crops and the HTML contact sheet |

Results: [`results/summary.md`](results/summary.md) (tables are generated, the text below
them comes from [`NOTES.md`](NOTES.md)). Chunks, crops and the contact sheet are not
committed; the commands above recreate them, byte for byte except the timings.

Each tile is 4 x 4 ZL16 chunks against 16 x 16 ZL18 chunks, 272 requests per location and
provider. Fetched chunks are kept under `level0-bench/data`, so a second run downloads
nothing.
