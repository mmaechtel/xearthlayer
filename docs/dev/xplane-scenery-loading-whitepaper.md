# X-Plane 12 Scenery Loading Behavior

**A Technical White Paper Based on Empirical Research**

**Authors**: XEarthLayer Project
**Date**: January 2026
**Version**: 1.0

---

## Abstract

This document presents empirical findings on how X-Plane 12 loads orthophoto scenery during flight. Through systematic flight testing with instrumented logging, we captured over 2.8 million scenery tile requests across four test flights totaling 8+ hours of flight time. The research reveals predictable patterns in X-Plane's scenery loading behavior that are not documented in the X-Plane SDK or developer resources.

These findings are valuable to developers building scenery streaming systems, caching solutions, or performance optimization tools for X-Plane.

---

## Table of Contents

1. [Introduction](#introduction)
2. [Methodology](#methodology)
3. [Key Findings](#key-findings)
4. [Detailed Analysis](#detailed-analysis)
5. [Conclusions](#conclusions)
6. [Appendix: Raw Data](#appendix-raw-data)

---

## Introduction

### Background

X-Plane 12 uses a tile-based scenery system organized around DSF (Distribution Scenery Format) tiles, where each DSF tile covers 1° latitude × 1° longitude. Within each DSF tile, orthophoto textures are stored as DDS files at various zoom levels (typically ZL12-ZL18).

The X-Plane SDK currently provides no documentation on:
- When the simulator decides to load scenery ahead of the aircraft
- How much scenery is loaded in each loading event
- The spatial pattern of tile loading (individual tiles vs. bands)
- How loading behavior changes with aircraft speed or heading

Understanding these behaviors is essential for:
- Scenery streaming systems (loading tiles on-demand from remote servers)
- Prefetch/caching systems (pre-loading tiles before X-Plane requests them)
- Performance optimization (reducing scenery loading stutters)

### Research Goals

1. Determine the **trigger position** within a DSF tile that initiates scenery loading
2. Measure the **lead distance** (how far ahead X-Plane loads)
3. Identify **loading patterns** (individual tiles vs. complete bands)
4. Analyze behavior during **diagonal flight** (NE/SE/SW/NW headings)
5. Test whether **aircraft speed** affects loading timing
6. Measure **turn adaptation** timing after heading changes
7. Observe loading behavior **over oceans** vs. land

---

## Methodology

### Test Environment

| Component | Specification |
|-----------|---------------|
| Simulator | X-Plane 12.4 |
| System | Linux, AMD Ryzen 9 9950X3D, 94GB RAM, NVMe storage |
| Network | 5 Gbps symmetrical fiber |
| Instrumentation | XEarthLayer with debug logging enabled |

### Instrumented Logging

All scenery requests from X-Plane were captured via a FUSE virtual filesystem that logged:
- Timestamp of each DDS texture request
- Tile coordinates (row, col, zoom level)
- DSF tile identifier
- Cache hit/miss status
- Request latency

Aircraft position was logged every 20 seconds via UDP telemetry:
- Latitude, longitude, altitude
- Heading, ground speed
- DSF tile containing aircraft

### Test Flights

| Flight | Route | Heading | Duration | DDS Requests | Purpose |
|--------|-------|---------|----------|--------------|---------|
| 1 | EDDH → EDDF | ~198° (S) | 2:00:00 | 562,612 | Longitudinal band loading |
| 2 | EDDH → EKCH | ~55° (NE) | 0:35:00 | ~400,000 | Diagonal loading pattern |
| 3 | EDDH → LFMN | ~171° (S) | 2:21:37 | 1,224,600 | Turn adaptation, long-haul |
| 4 | KJFK → EGLL | ~65° (NE) | 2:26:56 | 813,206 | High-speed cruise, ocean |

**Total**: 8+ hours of flight time, 2.8+ million DDS requests analyzed.

---

## Key Findings

### Summary Table

| Behavior | Finding | Confidence |
|----------|---------|------------|
| **Trigger Position** | ~0.6° into current DSF tile (heading direction) | High |
| **Lead Distance** | 2-3° ahead of aircraft | High |
| **Loading Unit** | Complete bands, not individual tiles | High |
| **Band Width** | 2-4 DSF tiles perpendicular to travel | High |
| **Diagonal Loading** | BOTH latitude and longitude bands simultaneously | High |
| **Direction Priority** | Longitude (E/W) loads slightly before latitude (N/S) | Medium |
| **Speed Independence** | Trigger position unchanged at any speed (150kt-560kt) | High |
| **Turn Adaptation** | 20-40 seconds after heading stabilizes | Medium |
| **Ocean Behavior** | Same request rate, but 97% cache hits | High |
| **Burst Size** | 2,000-4,000 tiles per loading event | High |

---

## Detailed Analysis

### 1. Trigger Position

X-Plane initiates scenery loading when the aircraft reaches approximately **0.6° into the current DSF tile** in the direction of travel.

**Evidence (Flight 1)**:
```
Time     DSF Tile    Entry Position    Loading Triggered
──────────────────────────────────────────────────────────
32:33    +52+010     (0.96°, 0.41°)    35 seconds later (at 0.89°)
78:33    +51+010     (0.97°, 0.10°)    2 seconds later (at 0.60°)
81:13    +51+009     (0.60°, 0.99°)    Immediate (already at threshold)
```

**Interpretation**: The trigger is position-based, not time-based. If the aircraft enters a DSF tile already past the 0.6° threshold, loading begins immediately. If entry is at the tile edge (0.0°), loading waits until 0.6° is reached.

### 2. Lead Distance

X-Plane loads scenery **2-3° ahead** of the aircraft in the direction of travel, significantly more than the assumed 1° (one DSF tile).

**Evidence (Flight 1)**:
```
Aircraft Position: 52.7°N, heading south
Loading observed:
  - Latitude 50° to 52° (2.7° ahead)
  - Longitude 8° to 11° (4° wide band)
```

**Evidence (Flight 3)**:
```
Aircraft Position: 48.5°N, heading south toward Nice
Loading observed:
  - Latitude 45° to 48° (3.5° ahead)
```

### 3. Band Loading Pattern

X-Plane loads **complete bands** of tiles, not individual tiles scattered across the map.

**Observed Pattern (Southbound Flight)**:
```
┌─────────────────────────────────────────────┐
│  Loading burst contains:                    │
│  - 3 complete latitude rows (50°, 51°, 52°) │
│  - Each row spans 4° longitude (8° to 11°)  │
│  - Total: ~7,000-11,000 tiles per burst     │
└─────────────────────────────────────────────┘
```

This suggests X-Plane's scenery system organizes loading by DSF tile rows/columns rather than proximity-based algorithms.

### 4. Diagonal Flight Loading

When flying diagonal headings (NE/SE/SW/NW), X-Plane loads **BOTH latitude and longitude bands simultaneously**.

**Evidence (Flight 2 - EDDH to EKCH, heading 55° NE)**:
```
Entry to +54+011:
  - NORTH band (+55 latitude) loaded after 46.0 seconds
  - EAST band (+12 longitude) loaded after 17.0 seconds

Observation: East direction loaded 29 seconds BEFORE north direction
```

**Evidence (Flight 4 - KJFK to EGLL, heading 65° NE)**:
```
Both latitude and longitude bands observed loading together
during diagonal transatlantic flight.
```

**Interpretation**: For diagonal flight, X-Plane appears to:
1. Detect both directional components of travel
2. Load bands in both directions
3. Prioritize the direction with larger velocity component (E/W loads slightly before N/S)

### 5. Speed Independence

Aircraft ground speed does **NOT** affect the trigger position. Whether flying at 150 knots or 560 knots, X-Plane triggers loading at the same ~0.6° position within the DSF tile.

**Evidence (Flight 4 - 550+ kt cruise)**:
```
DSF Tile    Entry Position    Ground Speed
+42-071     0.00°, 0.03°     556 kt
+43-069     0.01°, 0.09°     555 kt
+44-065     0.18°, 0.01°     554 kt
+45-062     0.31°, 0.96°     555 kt
+46-058     0.23°, 0.01°     560 kt

Trigger position remained consistent at ~0.6° despite high speed.
```

**Interpretation**: The trigger is purely position-based. At higher speeds, the aircraft simply reaches the trigger position faster, giving less time for background loading systems to prepare.

### 6. Turn Adaptation

After a heading change, X-Plane adapts its loading pattern within **20-40 seconds**.

**Evidence (Flight 3 - Turn during approach to Nice)**:
```
Time        Heading    Event
20:52:03    149.7°     Pre-turn heading
20:52:23    104.6°     Turn initiated (45° change)
20:52:43    88.8°      Turn complete
~20:53:10   -          Loading pattern shifted to east bands
```

**Interpretation**: X-Plane does not predict turns. It reacts to heading changes and adjusts the loading pattern after the turn stabilizes.

### 7. Ocean Behavior

Over featureless ocean, X-Plane maintains the **same request rate** but achieves very high cache hit rates.

**Evidence (Flight 4 - Atlantic crossing)**:
```
Phase              Request Rate    Cache Hit Rate
JFK departure      3,883/min       ~70%
Mid-Atlantic       3,808/min       97%
Newfoundland       1,262/min       ~85%
```

**Interpretation**: X-Plane continues requesting tiles over ocean (likely water textures), but the tiles are simpler and highly cached, data is reduced due to the water masks. This creates an opportunity for prefetch systems to prepare upcoming coastal tiles.

### 8. Initial Load Area

When a flight begins, X-Plane loads approximately **12° × 12°** (144 square degrees) of scenery around the aircraft position.

**Evidence (All Flights)**:
```
Flight 1 (EDDH): Initial load observed spanning 51°-57°N, 6°-14°E
Flight 4 (KJFK): Initial load observed spanning 37°-45°N, 77°-69°W
```

This initial load takes **4-5 minutes** from a cold cache on a fast internet connection and provides a baseline for system performance calibration.

---

## Conclusions

### X-Plane 12 Scenery Loading Model

Based on our empirical research, X-Plane 12's scenery loading follows this model:

```
X-PLANE SCENERY LOADING ALGORITHM (Inferred)
┌───────────────────────────────────────────────────────────────┐
│ 1. INITIAL LOAD                                               │
│    - Load 12° × 12° area around starting position             │
│    - Complete before flight begins                            │
│                                                               │
│ 2. IN-FLIGHT LOADING                                          │
│    - Monitor aircraft position within current DSF tile        │
│    - When position reaches 0.6° in heading direction:         │
│      → Load complete bands 2-3° ahead                         │
│      → Band width: 2-4 DSF tiles perpendicular to travel      │
│                                                               │
│ 3. DIAGONAL HANDLING                                          │
│    - For NE/SE/SW/NW headings, load BOTH lat and lon bands    │
│    - Prioritize direction with larger velocity component      │
│                                                               │
│ 4. TURN RESPONSE                                              │
│    - Detect heading change                                    │
│    - Wait 20-40 seconds for heading to stabilize              │
│    - Recalculate bands for new direction                      │
└───────────────────────────────────────────────────────────────┘
```

### Implications for Developers

1. **Prefetch Systems**: Trigger prefetch at 0.3-0.5° into DSF tile to complete before X-Plane's 0.6° threshold

2. **Caching Systems**: Cache complete bands, not individual tiles. X-Plane will request entire rows/columns.

3. **Performance Optimization**: The 2-3° lead distance gives streaming systems approximately 2-5 minutes (depending on speed) to prepare tiles

4. **Diagonal Flight**: Must handle both latitude and longitude bands simultaneously for NE/SE/SW/NW headings

5. **Speed Considerations**: Faster aircraft reduce the time window but don't change the trigger position

---

## Appendix: Raw Data

### Flight Test Logs

| Flight | Log File | Size |
|--------|----------|------|
| 1 | `xearthlayer-eddh-eddf.log` | 433 MB |
| 2 | `xearthlayer-eddh-ekch.log` | 307 MB |
| 3 | `xearthlayer-eddh-lfmn.log` | 1.4 GB |
| 4 | `xearthlayer-kjfk-egll-not-completed.log` | 861 MB |

### Analysis Tools

Log analysis performed using `scripts/analyze_flight_logs.py` which extracts:
- Position updates from APT telemetry
- DDS request timestamps and coordinates
- Burst detection (loading events)
- Cache hit/miss statistics

### References

- X-Plane 12 SDK Documentation (limited scenery system documentation)
- DSF Specification: https://developer.x-plane.com/article/dsf-specification/
- XEarthLayer Project: https://github.com/[project-url]

---

**Document Version History**

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | January 2026 | Initial release based on 4 test flights |
