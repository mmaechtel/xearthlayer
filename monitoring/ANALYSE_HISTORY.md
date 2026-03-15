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

## Aktueller Tuning-Stack (validiert durch Run T)

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

## Nächster Schritt

**Run Y:** Bestätigungsrun mit Run-T-Stack + irqbalance. Erwartung: 0 Main Thread Reclaim, ≤ 3,5% FPS < 25.
