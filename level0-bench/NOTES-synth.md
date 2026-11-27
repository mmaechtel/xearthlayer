## What the synthetic run is for

It answers two questions the real tiles cannot, because here the JPEG quality and the softness of
the provider's ZL16 are set, not found.

1. **How soft must a provider's ZL16 be before sharpening gains?** The sweep at quality 75:

   | Provider | ZL16 blur sigma | Detail vs truth | Unsharp 0.3, SSIM after BC1 | Tiles better |
   |---|---|---|---|---|
   | `synth-q75` | none | 0.96 | -0.0127 | 0 of 12 |
   | `synth-q75-s04` | 0.4 px | 0.89 | -0.0074 | 0 of 12 |
   | `synth-q75-s07` | 0.7 px | 0.66 | +0.0088 | 12 of 12 |
   | `synth-q75-s10` | 1.0 px | 0.53 | +0.0121 | 12 of 12 |

   The break-even lies between a detail ratio of 0.89 and 0.66. The real providers measure 0.95 to
   0.97 (`results/summary.md`), well on the losing side, and they do lose.
2. **Does deblocking respond to JPEG quality as designed?** Yes. The table-driven deblock gains
   +0.0014 SSIM at quality 60, +0.0005 at 75 and nothing at 90, where the fixed threshold is
   slightly negative. On `synth-mixed` (quality 55, 75 and 90 alternating per chunk) it gains
   +0.0007 on 10 of 12 tiles. Farmland gains most, forest loses.

Other findings:

- **Deblock before sharpening** helps where sharpening helps at all: on `synth-q75-s07`,
  q-deblock + unsharp 0.6 gains +0.0129 on 12 of 12 tiles.
- **Deringing** gains on farmland and town at low quality and costs on forest.
- **Quantisation-aware sharpening (F)** loses less than plain sharpening at quality 60 (-0.0051
  against -0.0107) and more at quality 90 (-0.0198 against -0.0106), as the rule intends, but it
  cannot turn a loss into a gain.

## Limits

- Procedural scenes, not imagery: no sensor noise, haze or resampling history, and the scenes of
  one content type score almost identically, so 12 tiles are closer to 5 independent samples.
- The provider's ZL16 is modelled as a 4x4 average of the pristine scene, optionally blurred. A real
  pyramid may be built differently.
- Method, thresholds and timing notes are in `results/summary.md`.
