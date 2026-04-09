# Color Adjustment — Implementation Plan

Per-provider saturation, brightness, and contrast adjustment for satellite imagery tiles.

## Motivation

Satellite imagery from different providers (Bing, Google Go2, Google Maps) varies in color
characteristics. Bing tiles tend to appear washed out or oversaturated depending on region.
Ortho4XP solves this with `saturation_adjust`, `brightness_adjust`, `contrast_adjust` per
provider — we implement the same concept, applied at tile generation time.

## Pipeline Insertion Point

```
Download JPEG chunks → Assemble 4096x4096 RGBA → [COLOR ADJUST] → DDS Encode → Cache
```

The color transform runs inside the existing `execute_blocking` call in `BuildAndCacheDdsTask`,
merged with assembly to avoid an additional thread-pool dispatch. An `is_identity()` check
short-circuits when all values are zero (default), ensuring zero overhead for unconfigured users.

## Config Format

INI-based, flat keys in `[texture]` section. Per-provider overrides use a `_providername` suffix:

```ini
[texture]
; Color adjustment (range: -100 to 100, default: 0)
saturation = 0
brightness = 0
contrast = 0

; Per-provider overrides (optional, override global values for that provider)
; saturation_bing = 10
; brightness_bing = -5
; saturation_go2 = 5
```

The parser resolves the active provider at parse time: if `provider.type = bing` and
`saturation_bing = 10` exists, the effective saturation is 10. Otherwise the global
value is used.

CLI access via `xearthlayer config get/set` works with global keys only:
```bash
xearthlayer config set texture.saturation 10
xearthlayer config get texture.brightness
```

## Algorithm

All values in range [-100, 100]. Internally normalized to [-1.0, 1.0] as `factor = value / 100.0`.

**Order of operations**: Brightness → Contrast → Saturation (standard image processing order).

### Brightness

Shift all channels toward white (positive) or black (negative):
```
pixel = clamp(pixel + factor * 255, 0, 255)
```

### Contrast

Scale deviation from mid-gray:
```
pixel = clamp(((pixel/255 - 0.5) * (1.0 + factor) + 0.5) * 255, 0, 255)
```

### Saturation

Interpolate between luminance (grayscale) and original color:
```
lum = 0.2126 * R + 0.7152 * G + 0.0722 * B
pixel = clamp(lum + (pixel - lum) * (1.0 + factor), 0, 255)
```

This matches PIL/Pillow's `ImageEnhance.Color` behavior used by Ortho4XP.

Alpha channel is never modified.

### Performance

4096x4096 = 16.7M pixels, ~3 arithmetic ops per channel per pixel.
Expected: **2-5 ms** per tile — negligible vs. DDS encode (50-200 ms).

## Data Flow

```
config.ini
  ↓  (parser.rs: resolve per-provider override)
ConfigFile.texture.color_adjustment: ColorAdjustment
  ↓  (run.rs)
TextureConfig.with_color_adjustment()
  ↓  (facade.rs)
RuntimeBuilder.with_color_adjustment()
  ↓  (runtime_builder.rs → create_factory)
DefaultDdsJobFactory { color_adjustment }
  ↓  (factory.rs → create_job)
DdsGenerateJob { color_adjustment }
  ↓  (dds_generate.rs → create_tasks)
BuildAndCacheDdsTask { color_adjustment }
  ↓  (build_and_cache_dds.rs → execute)
apply_to_image(&mut canvas)  // between assembly and encode
```

## Implementation Steps

### Step 1: ColorAdjustment type + transform (standalone, TDD)

**New file**: `xearthlayer/src/texture/color.rs`

- `ColorAdjustment` struct: `saturation: f32, brightness: f32, contrast: f32`
- `Default` impl (all 0.0)
- `is_identity() -> bool`
- `apply_to_image(&self, image: &mut RgbaImage)`
- Unit tests:
  - Identity (0,0,0) leaves image unchanged
  - Saturation -100 → grayscale
  - Brightness +100 → white, -100 → black
  - Contrast 0 → identity
  - Alpha untouched
  - Values clamp, no overflow

**Modify**: `xearthlayer/src/texture/mod.rs` — add `mod color; pub use color::ColorAdjustment;`

### Step 2: Config integration

**`config/defaults.rs`**: Add 3 constants (all 0.0).

**`config/settings.rs`**: Add `color_adjustment: ColorAdjustment` to `TextureSettings`.

**`config/keys.rs`**: Add `TextureSaturation`, `TextureBrightness`, `TextureContrast` variants.
- `FromStr`: `"texture.saturation"` etc.
- Validation: `FloatRangeSpec::new(-100.0, 100.0)` (already exists)
- `get()`/`set_unchecked()` read/write `config.texture.color_adjustment.*`

**`config/parser.rs`**: In `[texture]` section parsing:
1. Parse global `saturation`, `brightness`, `contrast` (default 0.0)
2. Check `saturation_{provider}` etc. for active provider override
3. Clamp to [-100.0, 100.0]
4. Assign to `config.texture.color_adjustment`

**`config/writer.rs`**: Add 3 keys to `[texture]` template with comments.

Tests:
- Global parse roundtrip
- Provider override wins over global
- Invalid value → error
- Missing keys → defaults

### Step 3: Thread through pipeline

**`service/runtime_builder.rs`**:
- Add `color_adjustment: ColorAdjustment` field
- Add `.with_color_adjustment()` builder method
- Pass to factory in `create_factory_*()` methods

**`service/facade.rs`**: Call `.with_color_adjustment(config.texture().color_adjustment())`.

**`xearthlayer-cli/src/commands/run.rs`**: Set on `TextureConfig`.

**`jobs/factory.rs`**: Add field to `DefaultDdsJobFactory`, pass to jobs.

**`jobs/dds_generate.rs`**: Add field to `DdsGenerateJob`, pass to task.

**`tasks/build_and_cache_dds.rs`**: Add field, apply in `execute()`:
```rust
let color_adj = self.color_adjustment;
let image_result = self.executor.execute_blocking(move || {
    let mut canvas = assemble_chunks(chunks)?;
    if !color_adj.is_identity() {
        color_adj.apply_to_image(&mut canvas);
    }
    Ok(canvas)
}).await;
```

### Step 4: Fix existing call sites

Adding a field to `BuildAndCacheDdsTask::new()`, `DdsGenerateJob::new()`, and
`DefaultDdsJobFactory::new()` breaks existing callers. Update all to pass
`ColorAdjustment::default()`. This includes test files.

### Step 5: Documentation

- **`docs/configuration.md`**: Document 3 new keys + per-provider override syntax
- **`CLAUDE.md`**: Add to `[texture]` section description
- **`CHANGELOG.md`**: Add entry under next version

## CPU vs. GPU Execution

The DDS encoding pipeline supports three backends: Software (CPU), ISPC (CPU/SIMD), and
wgpu (GPU compute shaders). The color adjustment runs **on CPU**, before the image enters
any of these backends. This is a deliberate choice:

**Why CPU is correct for the initial implementation:**

1. **Cost is negligible**: Color adjustment is ~2-5 ms for 16.7M pixels (simple arithmetic
   per channel). Assembly is ~10-20 ms, DDS encoding is 50-200 ms (CPU) or 20-50 ms (GPU).
   The transform disappears in the noise of the assembly step it is merged with.

2. **GPU separate pass would be slower**: Uploading 64 MB RGBA to GPU (~2-5 ms PCIe transfer),
   running a compute shader (~0.5 ms), then proceeding to encode adds overhead that exceeds
   the CPU cost. Net negative.

3. **Image is already in CPU memory**: The JPEG chunks are decoded and assembled on CPU
   (`execute_blocking`). The RGBA canvas is already in RAM. Touching it with a simple loop
   costs less than a GPU round-trip.

4. **GPU integration is viable as future optimization**: The color transform could be merged
   into the existing WGSL encode shader as a first pass before BCn compression. This would
   add ~0 ms extra (runs on data already uploaded for encoding). But it requires modifying
   the `block_compression` crate's WGSL shaders — out of scope for the initial feature.

**Decision**: CPU transform merged into the `execute_blocking` assembly call. The `is_identity()`
short-circuit ensures zero cost when unconfigured (the common case).

## Cache Invalidation

Changing color settings does **not** auto-invalidate cached DDS tiles. Users must run
`xearthlayer cache clear` after changing color settings. This is documented in the config
comments. A future enhancement could include a hash of color settings in the cache key.

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| `too_many_arguments` clippy lint | Already suppressed on `DdsGenerateJob::new()`. Acceptable. |
| Existing test breakage from new constructor arg | Mechanical fix: add `ColorAdjustment::default()` to all call sites |
| Performance regression | `is_identity()` short-circuit. Transform is <5ms vs. 50-200ms encode. |
| INI parser complexity for provider suffix | Simple `format!("saturation_{}", provider)` lookup, no new abstractions |

## Future Extensions (not in scope)

- **Tile-to-tile color normalization**: Histogram matching between adjacent tiles (like Ortho4XP V2.0 Color Normalize). Requires neighbor tile data — incompatible with streaming on-demand without significant buffering.
- **Per-region overrides**: Different settings for different geographic regions.
- **Cache key hashing**: Include color settings hash in cache key for automatic invalidation.
- **GPU-accelerated transform**: Move color adjustment into the WGSL compute shader pipeline.
