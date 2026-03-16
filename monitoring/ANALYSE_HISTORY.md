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

## Naechster Schritt

**Run AB:** Gleiche europaeische Route (UK→EDDN oder EDDH→EDDM) mit **frischem System** (Reboot). Ziel: Europaeische Baseline ohne Swap-Altlast etablieren.
