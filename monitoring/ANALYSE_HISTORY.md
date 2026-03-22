# Tuning-Historie: X-Plane 12 + XEarthLayer + QEMU/KVM

**System:** Ryzen 9 9800X3D 8C/16T, 96 GB RAM, RTX 4090 24 GB, 3× NVMe (2× SN850X 8TB + 990 PRO 4TB)
**Kernel:** Liquorix 6.19.6-1 (PDS), Btrfs RAID0 (xplane_data) + RAID1 (home)

---

## Vorgeschichte (Runs A–T, 2026-02-17 bis 2026-03-09)

16 Runs mit systematischem sysctl-, IO- und XEL-Tuning. Wichtigste Erkenntnisse:

- **min_free_kbytes = 2 GB** + **watermark_scale_factor = 125** eliminieren Direct Reclaim auf X-Plane Main Thread (Run T: 0 Events)
- **zram 16 GB lz4** absorbiert Memory Pressure im RAM statt auf NVMe
- **NVMe IO-Scheduler = none**, WBT = 0, Readahead 256 KB einheitlich
- **vm.swappiness = 8**, page-cluster = 0, vfs_cache_pressure = 60
- **XEL cpu_concurrent = 20**, max_concurrent_jobs = 32, max_concurrent_tasks = 128
- **FUSE FOPEN_DIRECT_IO Patch** verhindert FUSE-Read-induzierten Reclaim
- **Circuit Breaker + Prefetch** stabil konfiguriert (0 CB-Trips, 0 EMFILE)

**Run T (Referenz-Baseline, 2026-03-09):** Bester bisheriger Run — 0 Main Thread Reclaim, 1 allocstall, FPS < 25 nur 3,1%. Stack: zram 16 GB, min_free_kbytes 2 GB, watermark_scale_factor 125, memory_size 2 GB, kein irqbalance.

---

## Run W — zram-Entfernung = Regression (2026-03-14)

**Route:** EDDH → EDDM, 97 Min, FL300
**Änderungen:** zram deaktiviert, irqbalance aktiviert

| Metrik | Run T | Run W | Delta |
|--------|-------|-------|-------|
| Main Thread Reclaim | **0** | **54.686** | ❌❌❌ Katastrophal |
| allocstall Samples | 1 | 77 | 77× ❌ |
| Direct Reclaim total | 753 | 72.644 | 96× ❌ |
| FPS < 25 | 3,1% | 4,6% | +48% ❌ |
| Slow IO (>5ms) | 236 | 1.468 | 6,2× ❌ |
| EMFILE / CB Trips | 0 / 0 | 0 / 0 | ✅ |

**Ergebnis:** Ohne zram kehrt Direct Reclaim auf dem Main Thread zurück. Kernel evicted auf NVMe-Swap statt in zram zu komprimieren.

**irqbalance:** Funktioniert korrekt, NVMe-IRQs auf alle 16 CPUs verteilt. Beibehalten.

**Aktion:** zram 16 GB reaktivieren.

→ Details: `ANALYSE_RUN_W_20260314.md`

---

## Run X — sysctl-Default aufgedeckt (2026-03-15)

**Route:** EDDM → EDDH, 115 Min, FL350-360 (+ OBS YouTube-Streaming)
**Problem:** sysctl-Werte standen noch auf Defaults (Änderung 17 war verfrüht):
- min_free_kbytes = 66 MB (statt 2 GB)
- watermark_scale_factor = 10 (statt 125)
- memory_size = 4 GB (statt 2 GB)

| Metrik | Run T | Run X | Delta |
|--------|-------|-------|-------|
| Main Thread Reclaim | **0** | **12.472** | ❌❌ Regression |
| allocstall Samples | 1 | 38 | ❌ |
| Direct Reclaim total | 753 | 23.952 | 32× ❌ |
| FPS < 25 | 3,1% | 6,93% | ×2,2 ❌ |
| Slow IO (>5ms) | 236 | **30** | ✅ Bester Wert! |
| Tiles generiert | 2.701 | 34.725 | Fehlerfrei ✅ |
| EMFILE / CB Trips | 0 / 0 | 0 / 0 | ✅ |

**Ergebnis:** FUSE-Patch schützt vor FUSE-Read-Reclaim, aber NICHT vor DSF-Loading-Reclaim. min_free_kbytes = 2 GB bleibt unverzichtbar.

**Positiv:** Slow IO bester Wert (30), 34.725 Tiles fehlerfrei, 42 Min stall-frei vor erstem Event.

**Aktion:** sysctl auf Run-T-Level zurücksetzen, memory_size → 2 GB.

→ Details: `ANALYSE_RUN_X_20260315.md`

---

## System-Freeze (2026-03-14, während Run W)

Kompletter System-Freeze (kein Bild/Maus/Tastatur/Netzwerk) = PCIe-Bus-Lockup durch NVIDIA-Treiber. Diagnose und Gegenmaßnahmen:

→ Details: `CRASH_ANALYSIS_20260314.md`

---

## Run Y — Bestaetigungsrun: Run-T-Stack + irqbalance (2026-03-15)

**Region:** Costa Rica (Ground/Low-Level), 20 Min sysmon + 123 Min bpftrace
**Aenderungen:** Run-T-Stack + irqbalance aktiv. Drei sysctl auf Defaults belassen (vfs_cache_pressure=100, dirty_bg=10, dirty_ratio=20).

| Metrik | Run T | Run Y | Delta |
|--------|-------|-------|-------|
| Main Thread Reclaim | **0** | **0** | = PERFEKT |
| allocstall Samples | 1 | **0** | BESSER |
| FPS < 25 | 3,1% | **0,3%** | -90% (regionbedingt) |
| Swap Used | ja | **0 MB** | PERFEKT |
| Slow IO (>5ms) | 236 | **124.060** | REGRESSION (3-Min-Burst) |
| EMFILE / CB Trips | 0 / 0 | 0 / 0 | OK |

**Ergebnis:** Memory-Subsystem perfekt — Run-T-Stack + irqbalance bestaetigt. Massiver Slow-IO-Burst (124K Events in 3 Min, max 792ms um 21:04–21:06) ist ein neues Phaenomen, lag ausserhalb des sysmon-Fensters.

**Problem:** sysmon.py lief nur 20 Min (Default-Dauer), Flug ging 2+ Stunden.

→ Details: `ANALYSE_RUN_Y_20260315.md`

---

## Run Z — Voller 130-Min-Run: YMML→YSSY (2026-03-16)

**Route:** Melbourne → Sydney (ToLiss A320), 130 Min, FL370
**Aenderungen:** Volle sysmon-Dauer (150 Min konfiguriert), australische Route statt europaeisch

| Metrik | Run T | Run Y | Run Z | Delta Z vs Y |
|--------|-------|-------|-------|--------------|
| Main Thread Reclaim | **0** | **0** | **326** (1s Burst) | ⚠️ Startup-Only |
| allocstall Samples | 1 | 0 | **5** | ⚠️ |
| FPS < 25 | 3,1% | 0,3% | **4,09%** | Routenabhaengig |
| Slow IO (>5ms) | 236 | 124.060 | **1.743** | ✅ 71× besser |
| Swap Peak | ja | 0 MB | 3.724 MB | Laengerer Flug |
| EMFILE / CB Trips | 0/0 | 0/0 | 0/0 | ✅ |
| PSI Pressure | — | — | **0** | ✅ |

**Ergebnis:** Slow-IO-Problem geloest (71× Reduktion). Memory-Stack funktioniert im Steady-State (0 Reclaim nach Min 7). Einziger Reclaim-Burst waehrend Startup (Min 7, 326 Events in 1s, max 9,9 ms) durch gleichzeitiges DSF-Loading + DDS-Burst. VRAM nahe Limit (93,9%).

**Aktion:** XEL Soft-Start evaluieren (max_concurrent_jobs rampen statt sofort 32). NVMe PM QOS pruefen.

→ Details: `ANALYSE_RUN_Z_20260316.md`

---

## Run AA — Vorbelasteter Europa-Run: Stansted→EDDN (2026-03-16)

**Route:** England → Nuernberg (FL400), 83 Min
**Problem:** System nicht frisch — Swap bei Start 7,9 GB (Run Z Altlast), available nur 42 GB statt 80 GB

| Metrik | Run Z | Run AA | Delta |
|--------|-------|--------|-------|
| Main Thread Reclaim | 326 | **46.723** | 143× ❌❌❌ |
| Max Reclaim-Latenz | 9,9 ms | **85,5 ms** | 9× ❌ |
| Reclaim-Zeit Main Thread | ~0,1s | **17,4s** | ❌❌❌ |
| allocstall Peak | 350,8 | **7.907** | ❌ |
| Slow IO (>5ms) | 1.743 | **413** | ✅ 76% besser |
| FPS < 25 | 4,09% | **3,30%** | ✅ |
| Swap Peak | 3.724 MB | **18.236 MB** | 5× ❌ |
| X-Plane RSS Peak | 19.477 MB | **24.860 MB** | +28% (europaeische Scenery) |

**Ergebnis:** Nicht als Vergleich geeignet — System war vorbelastet. 46K Main Thread Reclaim Events sind direkte Folge der Swap-Altlast + schwerer europaeischer Scenery. Slow IO weiter verbessert. FPS unter 3,5% Ziel.

**Aktion:** Wiederholung auf gleicher Route mit **frischem System** (Reboot oder Swap-Reset).

→ Details: `ANALYSE_RUN_AA_20260316.md`

---

## Aktueller Tuning-Stack (validiert durch Run T + Y + Z)

```
vm.min_free_kbytes      = 2097152    (2 GB)
vm.watermark_scale_factor = 125
vm.swappiness           = 8
vm.page_cluster         = 0
vm.vfs_cache_pressure   = 60
vm.dirty_background_ratio = 3
vm.dirty_ratio          = 10
zram                    = 16 GB lz4
IO-Scheduler            = none (alle NVMe)
WBT                     = 0
Readahead               = 256 KB
irqbalance              = aktiv (seit Run W validiert)
```

## Run AB — Europaeische Baseline: Budapest→Duesseldorf (2026-03-16)

**Route:** LHBP → EDDL (ToLiss A320), 90 Min, FL-Cruise, ~800 km
**Aenderungen:** Frischer Reboot, Run-T-Stack + irqbalance, sysmon volle 90 Min

| Metrik | Run T | Run Z | Run AA | Run AB | Delta AB vs Z |
|--------|-------|-------|--------|--------|---------------|
| Main Thread Reclaim | **0** | 326 | 46.723 | **20.515** | 63× ❌ |
| Max Reclaim-Latenz | — | 9,9 ms | 85,5 ms | **80,7 ms** | 8× ❌ |
| allocstall Samples | 1 | 5 | 77 | **11** | 2× ⚠️ |
| Slow IO (>5ms) | 236 | 1.743 | 413 | **185** | ✅ 9× besser |
| FPS < 25 | 3,1% | 4,09% | 3,30% | **3,76%** | ≈ |
| Swap Peak | ja | 3.724 MB | 18.236 MB | **16.518 MB** | 4× ❌ |
| Fence Events | 0 | 0 | 0 | **3.810** | NEU ❌ |
| EMFILE / CB Trips | 0/0 | 0/0 | 0/0 | 0/0 | ✅ |

**Ergebnis:** Europaeische Baseline etabliert. min_free_kbytes=2GB reicht NICHT fuer europaeische Langfluege — X-Plane RSS wuechst auf 25 GB, Direct Reclaim kehrt auf den Main Thread zurueck (20K Events, 80ms Max). Slow IO bester Wert aller Runs (185). 3.810 DMA Fence Events sind neu und unerklrt.

**Aktion:** min_free_kbytes auf 3 GB erhoehen. GPU P2→P0 untersuchen.

→ Details: `ANALYSE_RUN_AB_20260316.md`

---

## Run AC — 3GB min_free_kbytes Test: Manchester→Niederlande (2026-03-20)

**Route:** EGCC → Niederlande (FL380), 120 Min
**Aenderungen:** min_free_kbytes 2GB → **3GB** (einzige Aenderung). GPU-Takt-Lock NICHT aktiv.

| Metrik | Run T | Run AB | **Run AC** | Delta AC vs AB |
|--------|-------|--------|------------|----------------|
| Main Thread Reclaim | **0** | 20.515 | **37.444** | +82% ❌ |
| Max Reclaim-Latenz | — | 80,7 ms | **35,5 ms** | -56% ✅ |
| Erste Stalls | — | Min 38 | **Min 101** | +63 Min ✅ |
| Stall-freie Session | — | 42% | **84%** | ×2 ✅ |
| allocstall Sum | ~1 | ~21.871 | **33.502** | +53% ❌ |
| FPS-Drops (Cruise) | 3,1% | bis 39s | **max 1,1s** | ✅✅ |
| Swap Peak | ja | 16.518 MB | **14.356 MB** | -13% ✅ |
| XEL RSS Peak | — | — | **9.875 MB** | ⚠️ NEU |
| XEL Threads Peak | — | — | **549** | ⚠️ NEU |
| Slow IO (>5ms) | 236 | 185 | **5.061** | ❌ (laengere Session) |
| CB Trips | 0 | 0 | **0** | ✅ |
| GPU Throttling | — | ja (P2) | **nein (P0)** | ✅ |

**Ergebnis:** 3GB min_free_kbytes verschiebt die Krise von Min 38 auf Min 113 — **84% der Session stall-frei** (vs 42%). Max-Latenz halbiert (35,5 vs 80,7 ms). FPS-Drops im Cruise alle unter 1,1s (vs 39s). ABER: Krise am Ende heftiger (8.884 allocstall/s Peak) weil groessere Schuld aufgebaut wird.

**Haupterkenntnis:** XEL RSS wuchs auf 9,9 GB (konfiguriert: 2 GB). Combined RSS (X-Plane 22,7 + XEL 9,9 + QEMU 4,2 = 36,8 GB) ueberfordert 3GB kswapd-Vorlauf. XEL-RSS ist der primaere Hebel.

**Aktion:** XEL-RSS-Wachstum begrenzen (warum 9,9 GB bei 2 GB config?). Dann Run AD auf gleicher Route.

→ Details: `ANALYSE_RUN_AC_2026-03-20.md`

---

## Aktueller Tuning-Stack (validiert durch Run T + Y + Z, 3GB fuer Europa besser aber unzureichend)

```
vm.min_free_kbytes      = 3145728    (3 GB) — besser als 2 GB, aber XEL-RSS ist der Hebel
vm.watermark_scale_factor = 125
vm.swappiness           = 8
vm.page_cluster         = 0
vm.vfs_cache_pressure   = 60
vm.dirty_background_ratio = 3
vm.dirty_ratio          = 10
zram                    = 16 GB lz4
IO-Scheduler            = none (alle NVMe)
WBT                     = 0
Readahead               = 256 KB
irqbalance              = aktiv (seit Run W validiert)
```

## Run AD — 0 Direct Reclaim: LFBO→EDWI Langflug (2026-03-21)

**Route:** LFBO (Toulouse) → Nordfrankreich → Belgien → NL → EDWI (Wilhelmshaven), 150 Min, FL380→FL390
**Aenderungen:** Branch `fix/proportional-prefetch-box` (box_extent 6,5°). XEL-RSS-Begrenzung NICHT umgesetzt.

| Metrik | Run T | Run AC | **Run AD** | Delta AD vs AC |
|--------|-------|--------|------------|----------------|
| Main Thread Reclaim | **0** | 37.444 | **0** | -100% ✅✅✅ |
| Max Reclaim-Latenz | — | 35,5 ms | **0 ms** | -100% ✅✅✅ |
| allocstall Sum | ~1 | 33.502 | **71.630** | +114% ❌ (aber "weich") |
| Erste Stalls (sustained) | — | Min 101 | **Min 83** | -18% ⚠️ |
| FPS < 25 | 3,1% | — | **3,58%** | ≈ Run T |
| Swap Peak | ja | 14.356 MB | **11.715 MB** | -18% ✅ |
| XEL RSS Peak | — | 9.875 MB | **15.828 MB** | +60% ❌❌ |
| Combined RSS Peak | ~36.800 MB | ~36.800 MB | **~37.146 MB** | ≈ |
| Slow IO (>5ms) | 236 | 5.061 | **5.816** | ≈ |
| Fence Events | — | — | **11.135** | NEU |
| CB Trips | 0 | 0 | **0** | ✅ |

**Ergebnis:** Qualitativer Durchbruch — **0 Direct Reclaim auf Main Thread** ueber 150 Min europaeischen Langflug (bpftrace bestaetigt). Die 71K allocstalls sind "weiche" Wartezeiten auf kswapd, keine synchronen Reclaim-Blockaden. FPS auf Run-T-Niveau (3,58% < 25). XEL RSS wuchs auf 15,8 GB (config: 2 GB) — weiterhin das primaere Speicherproblem.

**Neue Erkenntnis — zram nicht noetig:** Run AD lief OHNE zram (nur NVMe-Swap) und erreichte 0 Direct Reclaim. In Run W (auch ohne zram, min_free_kbytes=66 MB) waren es 54.686 Events. **min_free_kbytes=3 GB ist der eigentliche Schutz**, zram war nur ein Workaround. Kann aus dem Tuning-Stack entfernt werden.

**Aktion:** Zwei Aenderungen fuer Run AE:
1. XEL Memory Cache 2 → 4 GB (`memory_size=4`)
2. Watermark-Tuning korrigieren: `min_free_kbytes=1GB` + `watermark_scale_factor=500` (statt min_free_kbytes=3GB) — praeziseres Werkzeug, 2 GB weniger Emergency-Reserve-Verschwendung (Faktencheck: Kernel-Commit 795ae7a0 von Johannes Weiner)

→ Details: `ANALYSE_RUN_AD_2026-03-21.md`

---

## Aktueller Tuning-Stack (validiert durch Run T + Y + Z + AD)

```
vm.min_free_kbytes      = 1048576    (1 GB) — Emergency Reserve (reduziert von 3 GB)
vm.watermark_scale_factor = 500      (kswapd-Vorlauf ~4,8 GB — ersetzt uebergrosse min_free_kbytes)
vm.swappiness           = 8
vm.page_cluster         = 0
vm.vfs_cache_pressure   = 100        (Default, seit Run AD bestaetigt)
vm.dirty_background_ratio = 3
vm.dirty_ratio          = 10
zram                    = NICHT NOETIG (Run AD: 0 Direct Reclaim ohne zram)
IO-Scheduler            = none (alle NVMe)
WBT                     = 0
Readahead               = 256 KB
irqbalance              = aktiv
XEL memory_size         = 4          (erhoht von 2 GB)
```

## Run AE — Watermark-Experiment gescheitert: LSZH→EHAM (2026-03-22)

**Route:** LSZH (Zuerich) → EHAM (Amsterdam Schiphol), 120 Min, FL370
**Aenderungen:** memory_size 2→4 GB, min_free_kbytes 3GB→1GB, watermark_scale_factor 125→500, kein zram

| Metrik | Run T | Run AD | **Run AE** | Delta AE vs AD |
|--------|-------|--------|------------|----------------|
| Main Thread Reclaim | **0** | **0** | **10.057** | REGRESSION |
| Max Reclaim-Latenz | — | 0 ms | **14,5 ms** | REGRESSION |
| allocstall Sum | ~1 | 71.630 | **11.425** | -84% BESSER |
| allocstall Samples >0 | 1 | viele | **3 (0,04%)** | BESSER |
| FPS < 25 | 3,1% | 3,58% | **3,4%** | -5% ≈ |
| XEL RSS Peak | — | 15.828 MB | **17.449 MB** | +10% ❌ |
| Combined RSS Peak | — | ~37.146 MB | **~43.700 MB** | +18% ❌ |
| Swap Peak | ja | 11.715 MB | **18.064 MB** | +54% ❌❌ |
| wset_refault_anon | — | 44,8% | **87%** | +94% ❌❌ |
| Slow IO (>5ms) | 236 | 5.816 | **5.511** | -5% ≈ |
| Fence Events | — | 11.135 | **4.953** | -56% ✅ |
| CB Trips | 0 | 0 | **0** | ✅ |

**Ergebnis:** Watermark-Experiment gescheitert. min_free_kbytes=1GB + wsf=500 schuetzt NICHT so gut wie min_free_kbytes=3GB + wsf=125. Direct Reclaim kehrte auf Main Thread zurueck (10K Events, max 14,5 ms). Paradox: weniger allocstalls (-84%), aber wenn kswapd noetig wird, kommt er zu spaet → Direct Reclaim statt weicher Wartezeit.

**Kernerkenntnisse:**
- `min_free_kbytes=3GB` ist der unverzichtbare Schutz — wsf=500 kann ihn NICHT ersetzen
- XEL 4-GB-Cache hat RSS nicht reduziert (17,4 vs 15,8 GB) — moeglicherweise routenabhaengig
- Swap Thrash chronisch (87% Samples mit Anon-Refaults)
- Approach-Stutter (EHAM) durch 1,4 Mio pgfaults/s Peaks

**Aktion:** Watermarks zuruecksetzen auf Run-AD-Stack (min_free_kbytes=3GB, wsf=125). memory_size=4GB beibehalten fuer fairen Vergleich.

→ Details: `ANALYSE_RUN_AE_2026-03-22.md`

---

## Aktueller Tuning-Stack (Run-AD-Stack bestaetigt, Run-AE-Experiment gescheitert)

```
vm.min_free_kbytes      = 3145728    (3 GB) — UNVERZICHTBAR, wsf=500 kein Ersatz
vm.watermark_scale_factor = 125
vm.swappiness           = 8
vm.page_cluster         = 0
vm.vfs_cache_pressure   = 100        (Default)
vm.dirty_background_ratio = 3
vm.dirty_ratio          = 10
zram                    = NICHT NOETIG (Run AD: 0 Direct Reclaim ohne zram)
IO-Scheduler            = none (alle NVMe)
WBT                     = 0
Readahead               = 256 KB
irqbalance              = aktiv
XEL memory_size         = 4          (beibehalten, fairer Vergleich steht aus)
```

## Naechster Schritt

**Run AF:** Gleiche Route (LSZH→EHAM oder aehnlich europaeisch), Run-AD-Stack (min_free_kbytes=3GB, wsf=125), memory_size=4GB beibehalten. Ziel: Bestaetigen dass nur die Watermark-Aenderung die Regression verursacht hat.
