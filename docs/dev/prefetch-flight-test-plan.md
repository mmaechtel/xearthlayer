# Prefetch System Flight Test Plan

**Purpose**: Collect empirical data on X-Plane's scenery loading patterns to build an accurate prefetch prediction model.

**Date Created**: 2025-01-25

---

## Prerequisites

### 1. Enable Debug Logging

Before each flight, start XEarthLayer with debug logging enabled:

```bash
# Option A: Environment variable
RUST_LOG=debug xearthlayer run

# Option B: With log file capture (recommended)
RUST_LOG=debug xearthlayer run 2>&1 | tee ~/.xearthlayer/flight_logs/flight_N.log

# Option C: Using tracing file output (if configured)
RUST_LOG=debug xearthlayer run
# Logs go to ~/.xearthlayer/xearthlayer.log
```

### 2. Create Log Directory

```bash
mkdir -p ~/.xearthlayer/flight_logs
```

### 3. X-Plane Setup

- Ensure ortho scenery is installed for the flight area (Hamburg region recommended)
- Clear XEarthLayer memory cache before each flight for clean data:
  ```bash
  xearthlayer cache clear --memory
  ```
- Consider clearing disk cache for first flight only to see full loading patterns

### 4. ForeFlight/Telemetry

- Ensure X-Plane is configured to send UDP telemetry
- Verify XEarthLayer shows "GPS: Connected" in dashboard before takeoff

---

## Flight Test Matrix

| Flight | Route | Heading | Duration | Primary Data |
|--------|-------|---------|----------|--------------|
| 1 | EDDH → EDDF | ~180° S | 30 min | Longitudinal band loading |
| 2 | EDDH → EKCH | ~045° NE | 25 min | Diagonal loading pattern |
| 3 | EDDH → EDDW → turn | ~270° W then S | 30 min | Heading change behavior |
| 4 | EDDH → anywhere (jet) | Any | 20 min | High-speed trigger timing |

---

## Flight 1: Southbound (EDDH → EDDF)

**Objective**: Observe longitudinal band loading (north-south strips)

### Route Details
- **Departure**: EDDH (Hamburg)
- **Destination**: EDDF (Frankfurt)
- **Distance**: ~250nm
- **Heading**: ~180° (due south)
- **Recommended Aircraft**: Any (GA or jet)
- **Cruise Altitude**: FL100-FL350 (your choice)

### Procedure
1. Start XEarthLayer with debug logging
2. Load flight at EDDH (cold & dark or ready for takeoff)
3. **WAIT** for initial scenery load to complete (watch dashboard)
4. Note the time when initial load completes
5. Take off and climb to cruise altitude
6. Fly direct EDDF (heading ~180°)
7. Note any pauses/stutters (indicates scenery loading)
8. Continue until ~30 minutes flight time or arrival
9. Land and shut down XEarthLayer cleanly (press 'q')

### Data to Record
- Start time (real clock)
- Time when initial load completed
- Any notable pauses during flight
- End time

### Log File
Save as: `~/.xearthlayer/flight_logs/flight_1_south.log`

---

## Flight 2: Diagonal Northeast (EDDH → EKCH)

**Objective**: Observe diagonal loading (both lat and lon bands)

### Route Details
- **Departure**: EDDH (Hamburg)
- **Destination**: EKCH (Copenhagen)
- **Distance**: ~175nm
- **Heading**: ~045° (northeast)
- **Recommended Aircraft**: Any
- **Cruise Altitude**: FL100-FL200

### Procedure
1. Clear memory cache: `xearthlayer cache clear --memory`
2. Start XEarthLayer with debug logging
3. Load flight at EDDH
4. Wait for initial scenery load
5. Take off, fly direct EKCH (heading ~045°)
6. Continue for 25 minutes or until arrival

### Key Observations
- Does X-Plane load both N and E bands simultaneously?
- Or does it alternate between lat and lon bands?

### Log File
Save as: `~/.xearthlayer/flight_logs/flight_2_northeast.log`

---

## Flight 3: Westbound with Turn (EDDH → EDDW → South)

**Objective**: Observe behavior when heading changes mid-flight

### Route Details
- **Departure**: EDDH (Hamburg)
- **Waypoint**: EDDW (Bremen) - ~50nm west
- **Then**: Turn south toward EDDF
- **Total Distance**: ~150nm
- **Headings**: ~270° W initially, then ~180° S after turn

### Procedure
1. Clear memory cache
2. Start XEarthLayer with debug logging
3. Load flight at EDDH
4. Wait for initial scenery load
5. Take off, fly direct EDDW (heading ~270°)
6. At EDDW, make a ~90° turn to heading ~180° (south)
7. Continue south for 15+ minutes
8. Note any loading pattern changes after the turn

### Key Observations
- How quickly does loading pattern adapt to new heading?
- Is there a delay before new bands are loaded?
- Does X-Plane "predict" the turn or react to it?

### Log File
Save as: `~/.xearthlayer/flight_logs/flight_3_turn.log`

---

## Flight 4: High-Speed Jet Cruise

**Objective**: Observe if ground speed affects trigger timing

### Route Details
- **Departure**: EDDH (Hamburg)
- **Destination**: Any (EDDF, EDDM, or further)
- **Distance**: 200+ nm
- **Heading**: Any consistent direction
- **Required Aircraft**: Fast jet (A320, 737, or faster)
- **Cruise Altitude**: FL350+
- **Ground Speed**: 450+ knots

### Procedure
1. Clear memory cache
2. Start XEarthLayer with debug logging
3. Load flight at EDDH with jet aircraft
4. Wait for initial scenery load
5. Take off, climb to FL350+
6. Accelerate to cruise speed (M0.78+)
7. Fly straight for 20+ minutes
8. Note: Does scenery load earlier relative to position?

### Key Observations
- At 500kts vs 150kts, does X-Plane trigger loading earlier?
- Is the "midpoint trigger" still at ~0.5° into tile?
- Any loading failures due to speed?

### Log File
Save as: `~/.xearthlayer/flight_logs/flight_4_highspeed.log`

---

## Post-Flight Data Collection

After each flight, collect these files:

```bash
# Create a dated archive
DATE=$(date +%Y%m%d)
mkdir -p ~/.xearthlayer/flight_data_$DATE

# Copy logs
cp ~/.xearthlayer/flight_logs/*.log ~/.xearthlayer/flight_data_$DATE/
cp ~/.xearthlayer/xearthlayer.log ~/.xearthlayer/flight_data_$DATE/main.log

# Note file sizes
ls -lh ~/.xearthlayer/flight_data_$DATE/
```

---

## Analysis Notes Template

For each flight, record these observations:

```
Flight N: [Route]
Date/Time:
Duration:
Aircraft:
Cruise Speed:
Cruise Altitude:

Initial Load:
- Time to complete:
- Approx tiles loaded:

In-Flight Observations:
- Stutters/pauses at: (note times and approx position)
- Smooth periods:

Heading Changes (if any):
- Turn location:
- Time to adapt:

Notes:

```

---

## Expected Log Entries to Look For

### Position Updates (every 20s)
```
DEBUG APT position update: lat=53.45000, lon=9.87340, hdg=180.5, gs_kt=485, alt_ft=33000, dsf_tile=+53+009
```

### DDS Requests (each tile)
```
DEBUG Requesting DDS generation: tile_row=1314, tile_col=2161, tile_zoom=12
DEBUG DDS request completed: tile_row=1314, tile_col=2161, cache_hit=false, duration_ms=1234
```

### Circuit Breaker (loading detection)
```
INFO Circuit breaker OPEN - prefetch paused (high FUSE load)
INFO Circuit breaker CLOSED - prefetch resumed
```

### Burst Detection
```
DEBUG Starting prefetch cycle: loaded_count=5, heading=180.0°
```

---

## Questions We're Trying to Answer

1. **Trigger Position**: At what point within a DSF tile does X-Plane start loading the next band?
   - Expected: ~0.5° into the tile (midpoint)
   - ✅ **ANSWERED (Flight 1)**: ~0.6° into the tile, not 0.5°

2. **Leading Edge Distance**: How much loaded scenery remains ahead when loading triggers?
   - Expected: ~1° (one DSF tile)
   - ✅ **ANSWERED (Flight 1)**: 2-3° ahead, significantly more than expected

3. **Band vs Individual**: Does X-Plane load complete bands or scattered tiles?
   - Expected: Complete rows (latitude bands) or columns (longitude bands)
   - ✅ **ANSWERED (Flight 1)**: Complete bands confirmed - loads 2-3 latitudes spanning 3-4° longitude

4. **Diagonal Flight**: For NE/SE/SW/NW headings, does X-Plane load both bands simultaneously?
   - ✅ **ANSWERED (Flight 2)**: YES - loads BOTH lat and lon bands in same burst
   - EAST direction tends to trigger 3-30 seconds before NORTH

5. **Speed Factor**: Does higher ground speed cause earlier trigger?
   - ⏳ **PENDING (Flight 4)**: Need high-speed jet data

6. **Turn Adaptation**: How quickly does loading pattern change after a heading change?
   - ⏳ **PENDING (Flight 3)**: Need turn maneuver data

---

## Contact

After completing flights, share the log files for analysis. The logs can be analyzed with:

```bash
# Quick summary of DDS requests
grep "Requesting DDS generation" flight_1_south.log | wc -l

# Position updates
grep "APT position update" flight_1_south.log

# Circuit breaker state changes
grep "Circuit breaker" flight_1_south.log
```

---

# Flight Analysis Results

## Flight 1: Southbound (EDDH → EDDF) - COMPLETED

**Date**: 2025-01-27
**Log File**: `~/.xearthlayer/xearthlayer-eddh-eddf.log`
**Duration**: ~2 hours (with 30-minute pause mid-flight)

### Flight Profile

| Metric | Value |
|--------|-------|
| Route | Hamburg (53.6°N, 10.0°E) → Frankfurt (50.0°N, 8.6°E) |
| Heading | ~198° (South-Southwest) |
| Cruise Speed | ~500kt |
| Cruise Altitude | FL300+ |
| Total DDS Requests | 562,612 |
| Position Updates | 357 |

### Key Findings

#### 1. Loading Lead Distance

X-Plane loads scenery **2-3° ahead** of the aircraft in the direction of travel:

| Flight Phase | Lead Distance (South) |
|--------------|----------------------|
| Initial Load | 1.6° |
| Cruise | 2.7-2.8° |
| After 30-min Pause | 3.2° |

#### 2. Band Loading Pattern

X-Plane loads entire **longitudinal bands** (north-south strips) when flying south:

```
Aircraft at 52.7°N heading south:
┌─────────────────────────────────────────────┐
│  X-Plane simultaneously loading:            │
│  - Latitude 50° to 52° (3° band)            │
│  - Longitude 8° to 11° (4° wide)            │
│  - Total: 7,000-11,000 tiles per burst      │
└─────────────────────────────────────────────┘
```

#### 3. Trigger Timing Analysis

DSF tile entry position vs loading trigger:

| Entry Position in Tile | Trigger Delay | Trigger Position |
|------------------------|---------------|------------------|
| ~0.96° (near south edge) | 33-35 seconds | ~0.89° |
| ~0.60° (mid-tile) | 1-2 seconds | ~0.60° |

**Interpretation**:
- If aircraft enters DSF tile at the southern edge (already traveled most of it), X-Plane waits ~35 seconds before loading the next tiles
- If aircraft enters mid-tile, X-Plane loads immediately
- This suggests X-Plane triggers at a **fixed position threshold** (~0.6° into tile heading direction)

#### 4. Pause Period Behavior

During 30-minute pause (minutes 38-77):
- Aircraft stationary at 52.29°N, 10.20°E (DSF +52+010)
- Only 333 DDS requests (minimal background activity)
- After resume: **Immediate burst of 16,152 tiles** spanning 3.2° south

#### 5. DSF Tile Progression

```
Time     DSF Tile    Entry Position    Aircraft State
──────────────────────────────────────────────────────────
 2:33    +53+009     (0.63°, 0.99°)    Stationary at EDDH
23:53    +53+010     (0.61°, 0.01°)    Climbing, 165kt
32:33    +52+010     (0.96°, 0.41°)    Cruise, 423kt
78:33    +51+010     (0.97°, 0.10°)    Resume after pause, 505kt
81:13    +51+009     (0.60°, 0.99°)    Cruise, 505kt
85:53    +50+009     (0.97°, 0.78°)    Cruise, 501kt
94:33    +50+008     (0.24°, 0.96°)    Descent, 341kt
102:13   +49+007     (0.00°, 0.98°)    Approach, 298kt
```

#### 6. Cache Performance

| Metric | Value |
|--------|-------|
| Cache Hit Rate | 93.8% (153,227 / 163,424 completed requests) |
| ZL12 Tiles | 463,649 |
| ZL14 Tiles | 98,963 |

### Answers to Research Questions

1. **Trigger Position**: X-Plane triggers loading when aircraft is ~0.6° into the current DSF tile (in direction of travel), not at 0.5° midpoint

2. **Leading Edge Distance**: 2-3° ahead (not 1° as initially expected)

3. **Band vs Individual**: Confirmed **complete bands** - X-Plane loads 2-3 latitudes simultaneously spanning 3-4° longitude

4. **Quiet Periods for Prefetch**: 2-5 minute windows exist between loading bursts, but aircraft position changes significantly during these windows

### Prefetch Strategy Implications

Based on Flight 1 data:

```
RECOMMENDED PREFETCH PARAMETERS:
┌────────────────────────────────────────────────────────────┐
│ Lead Distance:      2-3° ahead in direction of travel      │
│ Trigger Position:   When aircraft at 0.3-0.5° into DSF tile│
│ Band Width:         2-3 DSF tiles perpendicular to travel  │
│ Prefetch Window:    Start before X-Plane's 0.6° threshold  │
└────────────────────────────────────────────────────────────┘
```

**Optimal Prefetch Strategy**:
1. When aircraft crosses 0.3° into a DSF tile (heading direction), start prefetching
2. Prefetch 2-3° ahead in the direction of travel
3. Prefetch entire bands (not individual tiles)
4. Stop prefetch when circuit breaker detects X-Plane loading (>50 req/sec)

---

## Flight 2: Diagonal Northeast (EDDH → EKCH) - COMPLETED

**Date**: 2025-01-27
**Log File**: `~/.xearthlayer/xearthlayer-eddh-ekch.log`
**Duration**: ~35 minutes

### Flight Profile

| Metric | Value |
|--------|-------|
| Route | Hamburg (53.6°N, 10.0°E) → Copenhagen (55.6°N, 12.6°E) |
| Heading | ~55° (Northeast) |
| Cruise Speed | ~450kt |
| Cruise Altitude | FL200+ |
| Total DDS Requests | ~400,000 |
| Position Updates | 106 |

### Key Findings

#### 1. Diagonal Loading: Both Bands Load Simultaneously

**Critical Discovery**: For diagonal flight (NE heading), X-Plane loads **BOTH** latitude and longitude bands together:

```
Aircraft heading 55° (northeast):
┌─────────────────────────────────────────────────┐
│  X-Plane simultaneously loading:                 │
│  - NORTH bands: latitudes ahead of aircraft      │
│  - EAST bands: longitudes ahead of aircraft      │
│  - Both directions in single loading burst       │
└─────────────────────────────────────────────────┘
```

#### 2. East Direction Loads First

When loading both directions, **EAST tends to trigger before NORTH**:

| Transition | First North Load | First East Load | Difference |
|------------|------------------|-----------------|------------|
| Entry +53+010 | +54 after 16.0s | +11 after 13.0s | East 3s earlier |
| Entry +54+010 | +55 after 8.0s | +11 after 8.0s | Simultaneous |
| Entry +54+011 | +55 after 46.0s | +12 after 17.0s | East 29s earlier |
| Entry +55+011 | +56 after 17.0s | +12 after 17.0s | Simultaneous |
| Entry +55+012 | +56 after 70.0s | +13 after 63.0s | East 7s earlier |

**Interpretation**: The east (longitude) direction appears to have a slightly lower trigger threshold than north (latitude), or X-Plane prioritizes loading in the direction of greater ground speed component.

#### 3. DSF Tile Coverage

| Metric | Value |
|--------|-------|
| Total DSF tiles | 42 |
| Latitude range | 52° to 57° (6° span) |
| Longitude range | 8° to 14° (7° span) |

#### 4. Loading Burst Composition

Major loading bursts (>500 requests/minute) show both directions loading:

| Time | Requests | Aircraft Position | North Bands | East Bands |
|------|----------|-------------------|-------------|------------|
| 3:00 | 21,628 | 53.67°N, 10.23°E | 52-55 (4) | 8-13 (6) |
| 6:00 | 1,445 | 53.87°N, 10.52°E | 53-55 (3) | 9-12 (4) |
| 10:00 | 14,055 | 54.37°N, 11.21°E | 53-56 (4) | 9-13 (5) |
| 14:00 | 2,115 | 54.80°N, 11.80°E | 54-55 (2) | 11-12 (2) |
| 19:00 | 11,419 | 55.25°N, 12.42°E | 54-57 (4) | 10-14 (5) |
| 25:00 | 8,016 | 55.60°N, 12.60°E | 55-57 (3) | 12-14 (3) |

#### 5. DSF Tile Transitions

```
Time     DSF Tile    Entry Position    Aircraft State
──────────────────────────────────────────────────────────
 1:47    +53+010     (0.63°, 0.01°)    Climbing, 86kt
 5:07    +53+010     (0.87°, 0.52°)    Cruise, 426kt
10:27    +54+010     (0.37°, 0.21°)    Cruise, 449kt
13:27    +54+011     (0.80°, 0.80°)    Cruise, 448kt
18:07    +55+011     (0.22°, 0.42°)    Cruise, 456kt
21:47    +55+012     (0.60°, 0.60°)    Cruise, 455kt
26:07    +55+012     (0.60°, 0.60°)    Descent, 331kt
```

### Answers to Research Questions

1. **Does X-Plane load both lat and lon bands simultaneously?**
   ✅ **YES** - For diagonal flight, X-Plane loads BOTH directions in the same loading burst

2. **Or does it alternate between them?**
   ❌ **NO** - They load together, though EAST direction tends to trigger 3-30 seconds before NORTH

3. **Is trigger position still ~0.6°?**
   ✅ **APPROXIMATELY** - Entry positions vary from 0.2° to 0.9° depending on tile geometry

### Prefetch Strategy Implications for Diagonal Flight

Based on Flight 2 data:

```
DIAGONAL PREFETCH PARAMETERS:
┌────────────────────────────────────────────────────────────┐
│ Primary Direction: Prefetch BOTH lat and lon bands         │
│ Lead Distance:     2-3° ahead in BOTH directions           │
│ Trigger Priority:  Longitude (E/W) slightly before Lat     │
│ Band Width:        4-6 DSF tiles in each direction         │
└────────────────────────────────────────────────────────────┘
```

**Optimal Prefetch Strategy for Diagonal Flight**:
1. Detect diagonal heading (not purely N/S or E/W)
2. Prefetch BOTH latitude and longitude bands ahead
3. Prioritize longitude direction slightly (loads first)
4. Use 2-3° lead distance in each direction

---

## Flight 3: Westbound with Turn - PENDING

*Awaiting test flight*

---

## Flight 4: High-Speed Jet - PENDING

*Awaiting test flight*

---

# Combined Findings Summary

## X-Plane Scenery Loading Behavior (Flights 1-2)

Based on empirical data from two test flights, we now have a clear picture of X-Plane's scenery loading patterns:

### Loading Characteristics

| Characteristic | Finding | Source |
|----------------|---------|--------|
| Lead Distance | 2-3° ahead of aircraft | Flight 1 |
| Trigger Position | ~0.6° into current DSF tile | Flight 1 |
| Loading Unit | Complete bands, not individual tiles | Both |
| Band Width | 2-4 DSF tiles perpendicular to travel | Both |
| Diagonal Loading | BOTH lat and lon bands simultaneously | Flight 2 |
| Direction Priority | Longitude (E/W) loads slightly before Latitude (N/S) | Flight 2 |

### Recommended Prefetch Strategy

Based on collected data, the optimal prefetch strategy is:

```
UNIVERSAL PREFETCH PARAMETERS
┌─────────────────────────────────────────────────────────────────┐
│ 1. TRIGGER THRESHOLD                                            │
│    - Start prefetch at 0.3-0.5° into current DSF tile           │
│    - This gives time before X-Plane's 0.6° trigger              │
│                                                                 │
│ 2. LEAD DISTANCE                                                │
│    - Prefetch 2-3° ahead in direction of travel                 │
│    - Match X-Plane's loading distance                           │
│                                                                 │
│ 3. BAND LOADING                                                 │
│    - Prefetch entire bands, not individual tiles                │
│    - Width: 2-4 DSF tiles perpendicular to travel              │
│                                                                 │
│ 4. HEADING-BASED LOGIC                                          │
│    - Cardinal (N/S/E/W): Single band direction                  │
│    - Diagonal (NE/SE/SW/NW): BOTH lat and lon bands             │
│    - Prioritize E/W direction slightly for diagonal             │
│                                                                 │
│ 5. CIRCUIT BREAKER                                              │
│    - Pause prefetch when X-Plane loading detected (>50 req/s)   │
│    - Resume during quiet periods (2-5 minute windows)           │
└─────────────────────────────────────────────────────────────────┘
```

### Heading-Based Prefetch Direction

| Heading Range | Primary Direction | Secondary Direction |
|---------------|-------------------|---------------------|
| 315°-45° (N) | North (+lat bands) | None |
| 45°-135° (E) | East (+lon bands) | None |
| 135°-225° (S) | South (-lat bands) | None |
| 225°-315° (W) | West (-lon bands) | None |
| 22.5°-67.5° (NE) | East (+lon bands) | North (+lat bands) |
| 67.5°-112.5° (E) | East (+lon bands) | None |
| 112.5°-157.5° (SE) | East (+lon bands) | South (-lat bands) |
| ... | ... | ... |

### Outstanding Questions

- **Speed Factor**: Does 500kt cruise require different trigger timing than 150kt GA?
- **Turn Adaptation**: How quickly does X-Plane adapt after heading change?
- **Stationary Aircraft**: What happens during extended ground holds?

These will be answered by Flights 3-4.
