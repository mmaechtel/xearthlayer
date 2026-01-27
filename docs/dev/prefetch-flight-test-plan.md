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

2. **Leading Edge Distance**: How much loaded scenery remains ahead when loading triggers?
   - Expected: ~1° (one DSF tile)

3. **Band vs Individual**: Does X-Plane load complete bands or scattered tiles?
   - Expected: Complete rows (latitude bands) or columns (longitude bands)

4. **Diagonal Flight**: For NE/SE/SW/NW headings, does X-Plane load both bands simultaneously?

5. **Speed Factor**: Does higher ground speed cause earlier trigger?

6. **Turn Adaptation**: How quickly does loading pattern change after a heading change?

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
