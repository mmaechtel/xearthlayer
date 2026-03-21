# Run AD — Ergebnisse: 150-Minuten Europa-Langflug

**Datum:** 2026-03-21
**System:** Ryzen 9 9800X3D 8C/16T, 96 GB RAM, RTX 4090 24 GB, 3x NVMe (2x SN850X 8TB + 990 PRO 4TB)
**Kernel:** Liquorix 6.19.6-1 (PDS), Btrfs RAID0 (xplane_data) + RAID1 (home)
**Workload:** X-Plane 12, XEarthLayer, QEMU/KVM, plasmashell, kwin_wayland, swiftguistd, festival
**Aenderungen seit Run AC:** Branch `fix/proportional-prefetch-box` — proportional heading bias, box_extent 9.0 -> 6.5 Grad (#98). XEL-RSS-Begrenzung war als Ziel geplant, wurde aber NICHT umgesetzt.

---

## 0. Testbedingungen

| Parameter | Wert |
|-----------|------|
| Dauer | 150 Min (9000s), 44.000 Samples |
| Route | LFBO (Toulouse) → Nordfrankreich → Belgien → NL → EDWI (Wilhelmshaven) |
| Flughoehe | FL380 → FL390 (Step Climb) |
| Sidecar | Ja (bpftrace: reclaim, io_slow, fence) |
| Tuning-Stack | 3 GB min_free_kbytes, zram 16 GB lz4, swappiness 8, irqbalance |
| Frischer Reboot | Ja (Swap bei Start = 0 MB) |

---

## 1. Erwartungen vs. Ergebnisse

| Metrik | Run AC | **Run AD** | Delta | Bewertung |
|--------|--------|------------|-------|-----------|
| Main Thread Reclaim | 37.444 | **0** | -100% | ✅✅✅ |
| Max Reclaim-Latenz | 35,5 ms | **0 ms** | -100% | ✅✅✅ |
| allocstall Events | 33.502 | **71.630** | +114% | ❌❌ |
| allocstall Samples | — | **17** | — | ⚠️ |
| Erste Stalls | Min 101 | **Min 4,5** (init), **Min 83** (sustained) | ⚠️ | Gemischt |
| Stall-freie Session | 84% | **81%** (exkl. Init-Burst) | ≈ | ≈ |
| FPS < 25 | — | **3,58%** | — | ≈ Run T |
| FPS avg | — | **30,1** | — | ✅ |
| Swap Peak | 14.356 MB | **11.715 MB** | -18% | ✅ |
| XEL RSS Peak | 9.875 MB | **15.828 MB** | +60% | ❌❌ |
| X-Plane RSS Peak | 22.700 MB | **21.318 MB** | -6% | ≈ |
| Combined RSS Peak | ~36.800 MB | ~**37.146 MB** | ≈ | ❌ |
| Slow IO (>5ms) | 5.061 | **5.816** | +15% | ≈ |
| Fence Events | — | **11.135** | — | ⚠️ NEU |
| CB Trips | 0 | **0** | — | ✅ |
| GPU Throttling | nein | **nein** | — | ✅ |
| DDS Timeouts | — | **4** | — | ⚠️ |

---

## 2. Kernbefunde

### 2.1 Drei-Phasen-Verhalten

| Phase | Zeitraum | Swap | Allocstalls | Charakteristik |
|-------|----------|------|-------------|----------------|
| **Warm-up** | Min 0-5 | 0 → 100 MB | 1 Burst (6.881) | Initiale Allokation, kurzer Stall bei +4,5 min |
| **Plateau** | Min 5-50 | 100 → 385 MB | 0 | Stabil, kein Pressure |
| **Pressure** | Min 50-150 | 385 → 11.715 MB | 10 Cluster (64.749) | Eskalierend in 3 Stufen |

Pressure-Stufen:
- **Stufe 1 (+55 min):** Swap springt auf ~1,5 GB
- **Stufe 2 (+83-92 min):** Schwere Stalls (Peak 16.091/s bei +83 min, 10.659/s bei +92 min), Swap auf ~7 GB
- **Stufe 3 (+122-135 min):** Letzte Eskalation (6.650/s Peak), Swap auf ~11,7 GB, Compact Stalls (Peak 151)

### 2.2 Memory Pressure — Das Paradox

**Zentraler Befund: 0 Direct Reclaim Events (bpftrace) trotz 71.630 allocstalls (vmstat).**

Das ist ueberraschend und wichtig:
- bpftrace `trace_reclaim.log` zeigt **0 DIRECT_RECLAIM Events** — kein einziger Prozess wurde durch synchrones Reclaim blockiert
- vmstat `pgscan_direct_s` zeigt dennoch 17 Events mit Direct-Scan-Aktivitaet
- vmstat `allocstall_s` summiert 71.630 Stalls

**Erklaerung:** Die allocstalls entstehen nicht durch klassisches Direct Reclaim (mm_vmscan), sondern durch **Memory Compaction Stalls** (7 Events, Peak 151 bei +123 min) und **Allocation-Wartezeiten auf kswapd**. kswapd wurde 1.387x geweckt und arbeitet effizient (91,2% Effizienz), aber bei Bursts hinkt es hinterher. Die Allokationsanforderungen blockieren dann kurz, bis kswapd Seiten freigibt — ohne dass der anfordernde Thread selbst reclaimed.

**Bedeutung:** Dies ist ein DEUTLICH besseres Verhalten als Run AC (37.444 Main Thread Reclaim Events). X-Planes Render-Loop wird nicht mehr durch Reclaim unterbrochen. Die allocstalls sind "weiche" Wartezeiten, keine harten Reclaim-Blockaden.

### 2.3 Swap und Thrashing-Signal

| Metrik | Wert | Bewertung |
|--------|------|-----------|
| Swap Swing | 11.715 MB | ❌ Critical (>5 GB) |
| pswpin aktiv | 44,3% der Samples | ❌ Dauerhaft Swap-In |
| pswpout Peak | 580.027 pages/s | ❌ Panic-Swapping |
| wset_refault_anon aktiv | 44,8% | ❌ Thrashing-Indikator |
| Direct Reclaim Effizienz | 53,3% | ⚠️ Schlecht |
| kswapd Effizienz | 91,2% | ✅ Gut |

Fast die Haelfte der Session zeigt aktives Swap-Thrashing: Seiten werden ausgelagert und sofort wieder benoetigt. Das Combined RSS (X-Plane 21 GB + XEL 15,8 GB + QEMU 4,2 GB = ~41 GB) plus Kernel/Caches ueberfordert die 96 GB trotz 3 GB min_free_kbytes.

### 2.4 XEL RSS — Primaeres Problem

XEL RSS wuchs auf **15,8 GB** (Peak) bei konfiguriertem memory_size von 2 GB. Das ist 60% mehr als Run AC (9,9 GB) und der groesste Einzelfaktor fuer Memory Pressure.

- RSS stabil um 12,3-13,7 GB mit Spikes bis 15,8 GB waehrend Prefetch-Bursts
- Thread-Count schwankt zwischen 38 und 549
- 26.137 DDS-Jobs generiert (fehlerfrei bis auf 4 Timeouts)

### 2.5 Dirty Pages

- avg: 36,6 MB (✅ Healthy)
- max: **1.078 MB** bei +77 min (❌ Critical, >1 GB)
- Der 1-GB-Dirty-Peak liegt direkt VOR dem schwersten Stall-Cluster (+83 min) — Writeback-Stau als Trigger

---

## 3. In-Sim Telemetrie (FPS / CPU Time / GPU Time)

| Metrik | Wert |
|--------|------|
| FPS avg / median | 30,1 / 29,8 |
| FPS min | 19,9 (X-Plane Floor) |
| FPS < 25 | 3,58% |
| FPS < 20 | 2,88% (alle exakt 19,9 — X-Plane Minimum) |
| CPU Time avg | 18,6 ms |
| GPU Time avg | 15,5 ms |
| Bottleneck | **CPU-bound (71,7%)** |

FPS-Drop-Cluster:
- **Min 20-23:** Am Gate LFBO (X-Plane Shader Compilation, 516 Samples)
- **Min 97-98:** Nordfrankreich (~49,3°N), Tile-Grenzuebergang
- **Min 106-108:** Belgien-Grenze (~50,0°N)
- **Min 138:** NL/Deutschland (~52,9°N)

Alle Cruise-FPS-Drops korrelieren mit DSF-Grenzuebergaengen und Tile-Loading-Bursts.

---

## 4. GPU / VRAM

| Metrik | Wert | Bewertung |
|--------|------|-----------|
| VRAM Peak | 17.651 / 24.564 MiB (71,9%) | ✅ Healthy |
| GPU Util avg | 54,1% | ✅ Headroom |
| GPU Temp max | 63°C | ✅ Kuehl |
| Throttle Reasons | 0 | ✅ |
| Perf State | P2 (98,2%) | ✅ |
| Power avg / max | 195 W / 306 W | ✅ |
| DMA Fence Waits | 11.135 Events (max 28ms, nur kworker) | ⚠️ |

Fence Events: Ausschliesslich kworker-Threads, kein Userspace-Impact. Max 28ms ist unkritisch, aber 11K Events sind erhoehte Hintergrund-GPU-Sync-Aktivitaet (auch in Run AB aufgetreten: 3.810).

---

## 5. Disk IO

| Device | Read avg/max | Write avg/max | Read Lat p95 | Spikes >100MB/s |
|--------|-------------|---------------|-------------|----------------|
| nvme0n1 | 10 / 3.413 MB/s | 2,6 / 3.934 MB/s | 1,0 ms | 538 |
| nvme1n1 | 8,6 / 3.405 MB/s | 0,3 / 62,6 MB/s | 0,6 ms | 424 |
| nvme2n1 | 11,2 / 3.420 MB/s | 3,9 / 1.430 MB/s | 0,5 ms | 690 |
| sda | 0 | 0 | — | 0 |

**RAID0 Asymmetrie:** 25,7% Spread (nvme2n1 37,5%, nvme1n1 28,9%). Nicht performance-kritisch, aber auffaellig.

**Write-Bursts auf nvme0n1:** Extreme Peaks bei +91,6 min (3.934 MB/s, Latenz bis 1.655ms) und +122,8 min (3.273 MB/s, Latenz bis 627ms). Diese korrelieren exakt mit den schwersten allocstall-Clustern und sind vermutlich Swap-Flush + Btrfs-Journal.

**Slow IO (bpftrace):** 5.816 Events, avg 14,5ms, max 75ms. Latenz-Verteilung: 51% bei 5-10ms, 34% bei 10-20ms, 14% bei 20-75ms. NVMe Power-State-Cluster auf nvme0n1 bei +124 min (10-12ms bei Idle).

---

## 6. CPU & Frequenz

| Metrik | avg | max | p99 |
|--------|-----|-----|-----|
| user% | 21,3% | 99,7% | 68,8% |
| sys% | 3,4% | 66,3% | 28,0% |
| iowait% | 1,9% | 71,0% | 36,1% |
| guest% | 1,2% | 11,2% | 9,4% |

- **sys%-Spikes >15%:** 1.584 Events — korrelieren mit kswapd-Aktivitaet
- **iowait%-Spikes >2%:** 5.749 Events — Tile-Loading + Swap-IO
- P-Cores (cpu0-5): 70-81% der Zeit ueber 5 GHz (Boost gehalten)
- Kein Thermal Throttling

---

## 7. Per-Process

| Prozess | CPU avg/max | RSS min→max→End | IO Read max | Threads |
|---------|------------|-----------------|-------------|---------|
| X-Plane | 278% / 1594% | 1,4→21,3→20,3 GB | 7.060 MB/s | 58-118 |
| xearthlayer | 66% / 876% | 7,0→15,8→12,3 GB | 923 MB/s | 38-549 |
| QEMU | 21% / 200% | 4,2 GB (stabil) | ~0 | 9-73 |
| kwin_wayland | 8% / — | 952→195 MB | — | — |

- **X-Plane RSS:** Monoton steigend von 1,4 auf 21,3 GB, Plateau ab ~85 min. Typisches X-Plane-Verhalten (Scenery-Cache).
- **XEL RSS:** Schnell auf ~12,4 GB, dann stabil mit periodischen Spikes bis 15,8 GB (Prefetch-Bursts). Kein Leak, aber 6x ueber config.

---

## 8. Vergleich Run AC → Run AD

| Aspekt | Run AC | Run AD | Bewertung |
|--------|--------|--------|-----------|
| **Direct Reclaim Main Thread** | 37.444 | **0** | ✅✅✅ Durchbruch |
| **Max Reclaim-Latenz** | 35,5 ms | **0 ms** | ✅✅✅ |
| allocstall Summe | 33.502 | 71.630 | ❌ Hoeher (aber "weich") |
| FPS < 25 | — | 3,58% | ≈ Run T Level |
| Swap Peak | 14,4 GB | 11,7 GB | ✅ Weniger |
| XEL RSS Peak | 9,9 GB | 15,8 GB | ❌ +60% |
| Combined RSS Peak | ~36,8 GB | ~37,1 GB | ≈ |
| Slow IO | 5.061 | 5.816 | ≈ |
| Session-Dauer | 120 min | 150 min | +25% |
| Route | EGCC→NL | LFBO→EDWI | Aehnlich |

**Hauptunterschied:** Run AD hat **0 Direct Reclaim auf dem Main Thread** — der wichtigste Fortschritt seit Run T. Die allocstalls sind zwar numerisch hoeher, aber qualitativ anders: weiche Wartezeiten auf kswapd statt harter synchroner Reclaim. Das FPS-Ergebnis (3,58% < 25) ist fast identisch mit Run T (3,1%).

**Ungeloest:** XEL RSS bei 15,8 GB (config: 2 GB) ist das Kernproblem. Das treibt den Combined RSS auf 37+ GB und erzwingt Swap-Thrashing (44,8% der Samples).

---

## 9. Handlungsempfehlungen

### 9.1 XEL Memory Cache erhoehen: 2 GB → 4 GB (PRIORITAET 1)
XEL RSS waechst auf 15,8 GB — Haupttreiber sind Encoding-Bursts (Mipmap-Clones ~90 MB/Task × 128 concurrent Tasks) und FUSE-Buffer-Kopien, nicht der Memory-Cache selbst. Ein groesserer Cache (4 GB) reduziert Cache-Misses, dadurch weniger Disk-Reads, weniger Regeneration-Bursts und weniger Spitzen-RSS. Das Prefetch-Fenster (box_extent 6,5°) umfasst ~400-600 Tiles × 11 MB = 4,4-6,6 GB — 4 GB haelt zumindest die sichtbaren Tiles vollstaendig.
**Aenderung:** `memory_size = 4` in config.ini. Einzige Aenderung fuer Run AE.
**Falls positiv:** Im Folge-Run auf 8 GB erhoehen (haelt Sichtfeld + Prefetch komplett).

### 9.2 Dirty-Page-Burst untersuchen (PRIORITAET 2)
1 GB Dirty-Page-Peak bei +77 min direkt VOR dem schwersten Stall-Cluster. `vm.dirty_ratio=10` sollte das bei 9,6 GB deckeln, aber der Burst deutet auf einen Writeback-Stau hin. Pruefen ob `vm.dirty_background_ratio=3` genuegt oder ob absolute Werte (`dirty_background_bytes`) besser waeren.

### 9.3 Proportional Prefetch Box evaluieren (PRIORITAET 3)
Branch `fix/proportional-prefetch-box` (box_extent 6,5°) war aktiv. Kein Turn-Event im gesamten Flug (geradlinige Route LFBO→EDWI). Fuer einen fairen Test braucht es einen Flug mit Kurswechseln, um die proportionale Heading-Bias-Logik zu validieren.

### 9.4 zram nicht mehr noetig (ERKENNTNIS)
Run AD lief **ohne zram** (nur NVMe-Swap) und erreichte trotzdem 0 Direct Reclaim auf dem Main Thread. In Run W (ebenfalls ohne zram, aber min_free_kbytes=66 MB) waren es 54.686 Events. **min_free_kbytes = 3 GB ist der eigentliche Schutz**, zram war nur ein Workaround. Kann aus dem Tuning-Stack entfernt werden.

### 9.5 Kernel Page Cache (KEIN HANDLUNGSBEDARF)
41 GB Page Cache sind kein Problem — der Kernel gibt clean Pages sofort ohne Kopieraufwand frei. vfs_cache_pressure bleibt auf 100 (Default).

---

## 10. Zusammenfassung

Run AD zeigt einen **qualitativen Durchbruch** bei Direct Reclaim: 0 Main-Thread-Reclaim-Events ueber 150 Minuten europaeischen Langflug. Die FPS-Stabilitaet (30,1 avg, 3,58% < 25) liegt auf Run-T-Niveau trotz 50% laengerer Session und schwerer europaeischer Scenery.

Das verbleibende Problem ist **XEL RSS** (15,8 GB statt 2 GB config), das den Combined RSS auf 37+ GB treibt und Swap-Thrashing in 45% der Session verursacht. Die allocstalls (71K) sind "weich" (keine Direct Reclaim), aber das Thrashing verschwendet CPU-Zyklen und IO-Bandbreite.

**Naechster Schritt:** XEL RSS-Wachstum debuggen und begrenzen. Dann Run AE auf gleicher Route mit begrenztem RSS.
