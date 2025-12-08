# XEarthLayer Bug/Snag List

Tracking issues discovered during development and testing.

## Performance Issues

(None currently open)

## Visual Glitches

(None currently open)

## Tech Debt

### TD2: Refactor CLI Validation to Specification Pattern
- **Status**: Open
- **Priority**: Low
- **Description**: Review CLI command handlers and refactor business validation rules to use the Specification Pattern in library modules rather than being managed in the CLI interface.
- **Context**: The `config` CLI commands now use `ConfigKey` with validation specifications in the config module (`xearthlayer/src/config/keys.rs`). This pattern should be applied to other CLI handlers that contain validation logic.
- **Files to review**:
  - `xearthlayer-cli/src/commands/publish/` - Publisher CLI handlers
  - `xearthlayer-cli/src/commands/packages.rs` - Package manager CLI
- **Benefits**:
  - Validation logic reusable across different interfaces
  - Better separation of concerns (thin CLI layer)
  - Easier to test validation in isolation
- **Added**: 2025-12-05

### TD1: Migrate to Async FUSE Model
- **Status**: Open
- **Priority**: Medium
- **Description**: Current implementation uses blocking thread pool for parallel tile generation.
  The FUSE library (`fuser`) supports an async model that could provide better scalability.
- **Current Implementation**: `ParallelTileGenerator` wraps tile generation with:
  - Blocking thread pool (std::thread)
  - mpsc channels for work distribution
  - Request coalescing via HashMap
- **Future Implementation**:
  - Use `fuser`'s async filesystem trait
  - Integrate with tokio runtime
  - Replace blocking channels with async equivalents
- **Benefits**:
  - Better thread utilization under high load
  - More efficient I/O waiting
  - Native async/await integration
- **Risks**:
  - Significant refactoring required
  - Need to verify compatibility with current architecture
- **Added**: 2025-11-25

## Future Enhancements

### E5: Diagnostic Magenta Chunks
- Encode job ID or error code into failed magenta placeholder chunks
- Allows user to visually identify a failed chunk in the simulator and cross-reference with logs/metrics
- Implementation ideas:
  - Render job ID as text overlay on the magenta chunk
  - Encode as QR code for easy scanning
  - Use subtle color variations to encode numeric ID
- Benefits:
  - Direct visual-to-debug correlation
  - No need to guess which tile failed from coordinates alone
  - Useful for bug reports from users

### E2: Tile Pre-fetching
- Predict which tiles will be needed based on aircraft position/heading
- Generate tiles before X-Plane requests them

### E3: Progressive Loading
- Return low-resolution placeholder immediately
- Upgrade to full resolution in background

### E4: Persistent Cache Warming
- On startup, verify cached tiles are still valid
- Pre-warm cache for known flight areas

## Completed

### P1: Slow Initial Load ✓
- **Completed**: 2025-11-25
- **Original Issue**: Initial scene load took 4-5 minutes at KOAK
- **Resolution**: Implemented parallel tile generation with request coalescing
- **Result**: Load time reduced to ~30 seconds (with cache hits)

### E1: Parallel Tile Generation ✓
- **Completed**: 2025-11-25
- **Implementation**: `ParallelTileGenerator` in `xearthlayer/src/tile/parallel.rs`
- **Features**:
  - Configurable thread pool (default: num_cpus)
  - Request coalescing for duplicate tile requests
  - Timeout handling with magenta placeholder fallback
  - Configuration via `[generation]` section in config.ini

### Graceful Shutdown ✓
- **Completed**: 2025-11-25
- **Implementation**: Signal handling with automatic FUSE unmount
- **Features**:
  - Ctrl+C and SIGTERM trigger clean shutdown
  - BackgroundSession auto-unmounts on drop
  - No stale mounts left behind

---

### V1: Scenery Glitches ✓
- **Closed**: 2025-11-26
- **Original Issue**: Visual glitches observed during first test flight
- **Resolution**: Not an XEarthLayer issue - caused by Ortho4XP scenery pack configuration for X-Plane 12. Improved with better Ortho4XP settings.

## Notes

- Test environment: San Francisco Bay Area (Ortho4XP scenery pack)
- X-Plane 12 on Linux
- First successful end-to-end test: 2025-11-25
