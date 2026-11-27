# level0-bench

An offline benchmark for the level 0 ideas in samsoir/xearthlayer#458: does deblocking,
deringing or sharpening the assembled tile bring ZL16 imagery measurably closer to the
ZL18 truth, per provider, after BC1?

It is a research tool. The filters live in this crate, nothing in XEL calls them, and the
runtime does not link it, so XEL's output is unchanged.

```bash
scripts/level0-bench.sh probe   # one chunk per provider: which tile servers answer
scripts/level0-bench.sh real    # fetch real chunks, then run  -> level0-bench/results
scripts/level0-bench.sh synth   # synthetic scenes, no network -> level0-bench/results-synth
```

| Module | What it does |
|---|---|
| `dataset.rs` | Locations, providers (through `xearthlayer::provider`), on-disk cache, manifest, tile loading with XEL's decode and the #458 linear-light reduction of the truth |
| `jpeg_info.rs` | Quantisation tables, subsampling and a quality estimate from a JPEG header; the thresholds derived from the tables |
| `filters.rs` | `deblock`, `dering`, `unsharp`: integer, in place, deterministic |
| `variants.rs` | The 17 variants (A baseline to G weak unsharp) |
| `bc1.rs` | Level 0 through `DdsEncoder` (ISPC) and a BC1 decode |
| `metrics.rs` | SSIM, linear-light SSIM, PSNR, butteraugli, detail vs truth |
| `synth.rs` | Deterministic synthetic scenes: JPEG quality, softness sweep, mixed quality |
| `report.rs` | `results.csv`, `summary.md`, crops and the HTML contact sheet |

Results:

- [`results/summary.md`](results/summary.md): Bing, ArcGIS and Apple, 12 locations each. The
  recommendation is here (text from [`NOTES.md`](NOTES.md)).
- [`results-synth/summary.md`](results-synth/summary.md): synthetic scenes, used to find how soft a
  provider's ZL16 must be before sharpening gains (text from [`NOTES-synth.md`](NOTES-synth.md)).

Chunks, crops and the contact sheets are not committed; the commands above recreate them. The
synthetic run is byte for byte reproducible except the timings; the real run depends on what the
providers serve that day.

Each tile is 4 x 4 ZL16 chunks against 16 x 16 ZL18 chunks, 272 requests per location and
provider. Fetched chunks are kept under `level0-bench/data`, so a second run downloads nothing.
The tile servers have to be reachable: in a Claude Code cloud session they must be on the
environment's allowed hosts.
