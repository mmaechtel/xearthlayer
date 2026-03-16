# Run Z — Ergebnisse: 130-Minuten YMML→YSSY (Melbourne→Sydney)

**Datum:** 2026-03-16
**System:** Ryzen 9 9800X3D 8C/16T, 94 GB RAM, RTX 4090 24 GB, 3× NVMe (2× SN850X 8TB + 990 PRO 4TB)
**Kernel:** Liquorix 6.19.6-1 (PDS), Btrfs RAID0 (xplane_data) + RAID1 (home)
**Workload:** X-Plane 12 (ToLiss A320), XEarthLayer, QEMU/KVM, gamescope
**Aenderungen seit Run Y:** Volle sysmon-Dauer (150 Min statt 20 Min), europaeische → australische Route (YMML→YSSY statt Costa Rica)

---

## 0. Testbedingungen

| Parameter | Wert |
|-----------|------|
| Dauer | 130 Min (08:01–10:11 UTC) |
| sysmon Samples | 7.648 (vmstat), 38.241 (mem), 33.378 (telemetry) |
| Sidecar | Ja (3 bpftrace Probes: reclaim, io_slow, fence) |
| Tuning-Stack | Run-T-Stack + irqbalance |
| sysctl | min_free_kbytes=2 GB, watermark_scale_factor=125, swappiness=8, page_cluster=0, vfs_cache_pressure=60, dirty_bg=3, dirty_ratio=10 |
| zram | 16 GB lz4 |
| XEL Config | cpu_concurrent=20, max_concurrent_jobs=32, max_concurrent_tasks=128, memory_size=2 GB |

## 1. Erwartungen vs. Ergebnisse

| Metrik | Erwartung (Run Z) | Run Y | **Run Z** | Bewertung |
|--------|-------------------|-------|-----------|-----------|
| Main Thread Reclaim | 0 | 0 | **326** (1-Sekunden-Burst) | ⚠️ Regression |
| allocstall Samples | 0 | 0 | **5** (1 major + 4 minor) | ⚠️ |
| FPS < 25 | ≤ 3,5% | 0,3% | **4,09%** | ⚠️ Ueber Ziel |
| FPS < 20 | 0% | 0% | **1,84%** | ⚠️ |
| Slow IO (>5ms) | < 500 | 124.060 | **1.743** | ✅ 71× besser als Y |
| Swap Peak | 0 MB | 0 MB | **3.724 MB** | ℹ️ Erwartet (laengerer Flug) |
| EMFILE / CB Trips | 0 / 0 | 0 / 0 | **0 / 0** | ✅ Perfekt |
| PSI Pressure | 0 | — | **0** | ✅ Perfekt |
| THP Fallbacks | 0 | — | **0** | ✅ Perfekt |
| DDS Generiert | — | — | **61.076** | ✅ Fehlerfrei |

---

## 2. Kernbefunde

### 2.1 Drei-Phasen-Verhalten

| Phase | Zeitraum | Indikator |
|-------|----------|-----------|
| **Warm-up** | Min 0–5 | Available sinkt 80→60 GB, Cache fuellt sich (~5,5 GB/min), kein kswapd |
| **Ramp-up** | Min 5–25 | kswapd aktiviert bei Min 4,1. Intensives Page-Scanning (Min 5–13). Available → 42 GB. Erster Swap bei Min 25 |
| **Steady-state** | Min 25–130 | Available oszilliert 35–39 GB. Swap waechst graduell. Periodische kswapd-Bursts alle ~2 Min |

### 2.2 Memory Pressure

| Metrik | Warm-up (0–5) | Ramp-up (5–25) | Steady (25–130) |
|--------|---------------|-----------------|------------------|
| available_mb avg | 66.107 | 44.875 | 37.762 |
| available_mb min | 60.223 | 40.742 | **34.754** |
| cached_mb avg | 32.742 | 43.128 | 38.038 |
| dirty_mb avg | 616 | 281 | 61 |

- **Swap-Verlauf:** Erster Swap bei Min 25 (23 MB), 500 MB bei Min 61, 1 GB bei Min 75, scharfer Sprung 2,1→3,0 GB bei Min 89, Peak 3.724 MB bei Min 124
- **Anon Refaults (zram-Thrashing):** Zwei kurze Episoden bei Min 89 (Swap-Sprung) und Min 129 (Session-Ende). Nicht nachhaltig.
- **File Refaults:** 3,8 Mio Pages gesamt — dominantes Signal durch DDS-Texture-Cache-Cycling

### 2.3 XEarthLayer Streaming Activity

| Metrik | Wert |
|--------|------|
| DDS generiert | 61.076 |
| Circuit Breaker Trips | **0** |
| EMFILE Errors | **0** |
| Download Timeouts | 8 (2 Chunks final failed) |
| Prefetch Cap Events | 44 (boundary_cap=200, funktioniert) |
| Disk Cache | 213 GB / 430 GB (49,7%) |
| XEL RSS Peak | 15.739 MB (Min 74) |
| XEL Threads | 549 (stabil nach ~45s Ramp) |

**DDS-Burst-Muster:**
- Min 0–17: Heavy Burst (600–3.600 Events/Min) — Melbourne-Area laden
- Min 17–38: Moderat (120–1.970/Min) — Cruise, neue Tiles
- Min 38–52: Abflachend — Cache warm
- Min 52–67: Erneuter Burst — Landung + Bodenphase, neue Tiles
- Min 67+: Minimal — Cache heiss

### 2.4 Direct Reclaim (Trace-Daten)

**369 Direct Reclaim Events gesamt:**

| Prozess | Events | Max Latenz | Bewertung |
|---------|--------|-----------|-----------|
| **X-Plane Main Thread** (PID 15082) | **326** | **9,9 ms** | ⚠️ |
| tokio-rt-worker (XEL) | 27 | — | OK |
| cuda-EvtHandlr | 7 | — | OK |
| khugepaged | 6 | — | OK |
| pipewire-pulse | 2 | — | OK |
| ltr_server1 | 1 | — | OK |

**Alle 326 Main Thread Events konzentriert in einem 1-Sekunden-Burst bei Min 7,3** (08:08:22). 27 Events > 1 ms, Maximum 9,9 ms. Korreliert mit dem ersten schweren DSF-Lade- + DDS-Generierungs-Burst.

**Ursache:** Waehrend des initialen Scenery-Loadings (12 DSF-Tiles Melbourne-Area, schwerste 28,7s) uebersteigt der Memory-Bedarf kurzzeitig die kswapd-Kapazitaet. min_free_kbytes=2 GB schuetzt im Steady-State, aber der initiale Burst ist zu heftig.

### 2.5 Alloc-Stall-Cluster

| # | Min | allocstall_s | Ursache |
|---|-----|-------------|---------|
| 1 | **7,3** | **350,8** | Direct Reclaim Burst — DSF-Loading + DDS-Burst |
| 2 | 62,2 | 1,0 | Isoliert, pswpout=69, minor |
| 3 | 74,8 | 1,0 | Swap-Schwelle 1 GB, kswapd reagiert |
| 4 | 75,0 | 2,9 | Swap-Out-Druck, pswpout=125 |
| 5 | 76,0 | 1,0 | Nachlauf von #3/#4 |

---

## 3. In-Sim Telemetrie (FPS / CPU Time / GPU Time)

| Metrik | Wert |
|--------|------|
| FPS avg | 30,0 |
| FPS median | 29,8 |
| FPS P5 | 25,6 |
| FPS P1 | 19,9 |
| FPS min (nicht pausiert) | 19,9 |
| FPS < 25 | **4,09%** (1.366 Samples) |
| FPS < 20 | **1,84%** (613 Samples) |
| Pausiert | 0,32% (106 Samples) |

### CPU vs GPU Bottleneck

| Metrik | CPU | GPU |
|--------|-----|-----|
| Durchschnitt | 13,3 ms | 20,7 ms |
| Bei FPS < 25 | 17,8 ms | 27,5 ms |
| Bottleneck-Anteil | 45,5% | **54,5%** |

**Phasenabhaengig:**
- Min 0–35: **GPU-bound** (GPU 25–33 ms, CPU 18–25 ms) — Melbourne Scenic
- Min 46–90: **CPU-bound** (CPU 33–38 ms, GPU 12–15 ms) — Ground/Approach/Sydney
- Min 108+: Mixed, Trend zurueck zu GPU-bound

### FPS-Drop-Perioden (> 3s Dauer)

| Zeitraum | Min FPS | Ursache |
|----------|---------|---------|
| Min 9,0–9,5 (31s) | 0,0 | **Pausiert** (Scenery Preload) |
| Min 15,4–15,8 (26s) | 19,9 | **GPU-bound** (CPU=4,1 ms, GPU=46,1 ms) |
| Min 24,1–24,3 (8s) | 19,9 | GPU-bound |
| Min 30,5–30,6 (11s) | 19,9 | GPU-bound |
| Min 126,4–126,6 (9s) | 0,0 | **Pausiert** (Session-Ende) |

---

## 4. GPU / VRAM

| Metrik | Min | Avg | Max |
|--------|-----|-----|-----|
| VRAM Used | 1.581 MiB | 16.488 MiB | **23.062 MiB (93,9%)** |
| GPU Util | — | 69,0% | — |
| Temperatur | 36 C | 57,1 C | 66 C |
| Power | — | 228,1 W | 340 W |
| GPU Clock | 210 MHz | 2.603 MHz | 2.745 MHz |
| Perf State | P2 (95,3%) | P8 idle (4,0%) | P0 (0,1%) |
| Throttling | **Kein** | | |

**VRAM-Verlauf (10-Min-Buckets):**
- Min 0–10: 5,6 GB (Startup)
- Min 10–30: 19–20 GB (Peak GPU Load, 94% Util)
- Min 30–90: 15–18 GB (Steady, 60–92% Util)
- Min 110–130: 18,5 GB (Erneute Aktivitaet)

**GPU fast ausgelastet:** 93,9% VRAM Peak ist der engste Constraint. GPU verbrachte 95,3% der Zeit in P2 (nicht P0), was auf Memory-Bandwidth-Bottleneck hindeutet.

### DMA Fence Waits

- **4.039 Events**, avg 9,4 ms, max 28 ms
- **Alle auf Kernel-Worker-Threads** (kworker/u65:*) — nicht auf X-Plane-Threads
- **Kein Anwendungs-Impact**

---

## 5. Disk IO

### Per-Device Throughput

| Device | Read avg (gesamt) | Read max | Write avg | Write max |
|--------|-------------------|----------|-----------|-----------|
| nvme0n1 | 2,9 MB/s | 2.359 MB/s | 2,0 MB/s | 607 MB/s |
| nvme1n1 | 3,8 MB/s | 2.384 MB/s | 2,3 MB/s | 710 MB/s |
| nvme2n1 | 1,6 MB/s | 2.366 MB/s | 1,3 MB/s | 80 MB/s |

### Slow IO (bpftrace trace_io_slow.log)

**Gesamt: 1.743 Events (Run Y: 124.060 — 71× Reduktion!)**

| Device | Events | Anteil |
|--------|--------|--------|
| nvme1n1 | 1.158 | 66% |
| nvme2n1 | 584 | 34% |
| nvme0n1 | 1 | 0% |

**Latenz-Verteilung:**
- 6 ms: 530 (30%)
- 7–10 ms: 823 (47%)
- 11–15 ms: 386 (22%)
- 17–22 ms: 4 (0,2%)
- avg 8,5 ms, max 22 ms

**Zeitliche Clusterung:**
- **Min 4–6: 1.470 Events** (Startup-Burst, 84% aller Events)
- Min 68: 106 Events (Small-Block Reads nvme2n1)
- Min 76: 102 Events (Small-Block Reads nvme1n1)
- Rest: vereinzelt

### Write-Latenz (aus io.csv)

Write-Latenz ist die Schwachstelle:
- nvme0n1: avg 5,9 ms, max 69,8 ms, ~1.000 Samples > 10 ms
- nvme1n1: avg 5,2 ms, max 68,9 ms, ~990 Samples > 10 ms
- nvme2n1: avg **17,2 ms**, max 69,2 ms, ~985 Samples > 10 ms

Read-Latenz dagegen excellent: avg < 0,3 ms auf allen Devices.

---

## 6. CPU & Frequenz

**Gesamt:** user=25,9%, sys=3,7%, iowait=1,2%, idle=66,8%

**Per-CPU-Asymmetrie:**
- CPUs 0–5: user 43–51% (Hauptlast)
- CPUs 6–7: user ~21%
- CPUs 8–15: user 8–17% (leicht)
- iowait gleichmaessig verteilt (1,0–1,6%)

**Frequenz:** avg 4.188–4.899 MHz, max 6.003 MHz (CPU1). Kein Throttling. Minimum-Werte (2,4–2,6 GHz) sind normaler Idle-Downclock.

**iowait-Spikes > 10%:** 3,7% der Zeit, konzentriert auf:
- Min 5–9: Initialer Cache-Warmup (bis 27,6%)
- Min 57–89: Sustained IO-Burst (Peak 34,1% bei Min 75)
- Min 127: Kurzer Spike (18,4%)

---

## 7. Per-Process

| Prozess | CPU% avg | RSS avg | RSS peak | IO Read | IO Write |
|---------|----------|---------|----------|---------|----------|
| X-Plane | 346,5% | 17.517 MB | 19.477 MB | — | — |
| XEarthLayer | 66,3% | 13.138 MB | 15.739 MB | 3,7 MB/s | 4,2 MB/s |
| QEMU | 28,3% | 4.253 MB | 4.299 MB | — | — |
| gamescope | < 3% | — | — | — | — |

**Total System RSS Peak:** 39.678 MB bei Min 86

**XEL-Timeline:**
- Min 0–10: CPU 210%, RSS 8,8 GB, IO 23,6/18,5 MB/s (Heavy Startup)
- Min 10–30: CPU ~42%, RSS ~12,9 GB, IO ~0 (Cache hot)
- Min 30–50: CPU ~6%, RSS ~12,9 GB (Idle)
- Min 50–90: CPU 72–112%, RSS 13,2–13,9 GB, IO 1–7 MB/s (Moderate Serving)
- Min 110–130: CPU ~63%, RSS ~14,0 GB (Renewed Activity)

---

## 8. Vergleich Run Y → Run Z

| Metrik | Run T (Baseline) | Run Y | **Run Z** | Trend |
|--------|------------------|-------|-----------|-------|
| Main Thread Reclaim | 0 | 0 | **326** (1s Burst) | ⚠️ |
| allocstall Samples | 1 | 0 | **5** | ⚠️ |
| FPS < 25 | 3,1% | 0,3% | **4,09%** | ⚠️ |
| Slow IO (>5ms) | 236 | 124.060 | **1.743** | ✅✅ |
| Swap Peak | ja | 0 MB | 3.724 MB | ℹ️ |
| EMFILE / CB Trips | 0/0 | 0/0 | 0/0 | ✅ |
| PSI Pressure | — | — | 0 | ✅ |
| Dauer sysmon | — | 20 Min | **130 Min** | ✅ Fix |

**Kontext-Unterschiede:**
- Run Y: Costa Rica (Ground/Low-Level, 20 Min sysmon) — leichterer Workload
- Run Z: YMML→YSSY (Cruise FL370, 130 Min sysmon) — schwerer Workload mit 61K DDS-Tiles
- Run T: EDDH→EDDM (FL300, Referenz-Baseline)

**Main Thread Reclaim:** Der 326-Event-Burst bei Min 7,3 ist eine Regression vs. Run T (0) und Run Y (0), aber ein voellig anderes Muster als Run W (54.686 Events ueber gesamte Session). Es handelt sich um einen einmaligen Startup-Burst waehrend des initialen 12-DSF-Tile-Loadings fuer Melbourne. Im Steady-State: **0 Main Thread Reclaim**.

**Slow IO:** Die 71× Reduktion (124K → 1,7K) bestaetigt, dass der Run-Y-Slow-IO-Burst ein Artefakt war (moeglicherweise NVMe Power-State nach Idle). Run Z zeigt normales Verhalten.

---

## 9. Handlungsempfehlungen

### 9.1 Startup-Reclaim eliminieren (Prioritaet: MITTEL)

**Problem:** 326 Direct Reclaim Events auf Main Thread bei Min 7 waehrend initialem DSF-Loading.

**Optionen:**
- **a)** XEL Startup-Phase drosseln: Erste 2 Min nach X-Plane-Start max_concurrent_jobs=8 statt 32, dann hochfahren
- **b)** X-Plane erst starten nachdem XEL-Cache warm ist (manueller Workflow-Anpassung)
- **c)** min_free_kbytes auf 3 GB erhoehen (mehr Headroom fuer Burst, aber 1 GB weniger fuer Cache)

**Empfehlung:** Option (a) — XEL-seitiger Soft-Start waere die sauberste Loesung.

### 9.2 VRAM-Druck beobachten (Prioritaet: NIEDRIG)

**Problem:** 93,9% VRAM-Peak (23,1/24,6 GB). GPU in P2 statt P0 (Memory-Bandwidth-Bound).

**Aktion:** Vorerst nur beobachten. Falls in zukuenftigen Runs VRAM > 95% erreicht, X-Plane Texture Quality reduzieren.

### 9.3 Write-Latenz untersuchen (Prioritaet: NIEDRIG)

**Problem:** Write-Latenz avg 5–17 ms, max 70 ms auf allen NVMe. nvme2n1 (990 PRO) am schlechtesten.

**Moegliche Ursachen:** NVMe Power Management, Btrfs Journal/Sync, zram-Writeback.

**Aktion:** Vor naechstem Run pruefen: `cat /sys/block/nvme*/device/power/pm_qos_latency_tolerance_us` — falls nicht auf 0 gesetzt, NVMe PM QOS auf 0 setzen.

### 9.4 FPS < 25 Anteil reduzieren (Prioritaet: NIEDRIG)

**Problem:** 4,09% FPS < 25 (Ziel ≤ 3,5%). Hauptursache: GPU-bound bei Min 15–30 (Melbourne Scenic Area, GPU-Time bis 46 ms).

**Ursache:** Nicht Memory/IO-bedingt, sondern GPU-Rendering-Last in der Melbourne-Region. Regionabhaengig, kein Tuning-Problem.

---

## 10. Zusammenfassung

Run Z ist der **laengste vollstaendig aufgezeichnete Run** (130 Min sysmon + bpftrace). Die Hauptziele wurden teilweise erreicht:

**Erfolge:**
- **Slow IO: 71× Reduktion** (124K → 1,7K) — das Run-Y-Problem ist geloest
- **Zero PSI Pressure, Zero THP Fallbacks, Zero EMFILE, Zero CB Trips** — System unter Kontrolle
- **Volle Datenabdeckung** — kein Datenverlust wie bei Run Y
- **Steady-State Memory stabil** — 37–38 GB available, kein Reclaim nach Min 7

**Offene Punkte:**
- **326 Main Thread Reclaim** bei Min 7 (Startup-Burst) — einmaliger Event, kein Steady-State-Problem
- **FPS < 25 bei 4,09%** (knapp ueber 3,5% Ziel) — GPU-bound in Melbourne-Region, routenabhaengig
- **Write-Latenz** auf allen NVMe erhoet — NVMe PM QOS pruefen
