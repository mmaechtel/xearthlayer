# Run X — Ergebnisse: 115-Minuten EDDM → EDDH

**Datum:** 2026-03-15
**System:** AMD Ryzen 7 9800X3D 8C/16T, 91 GB RAM, RTX 4090 24 GB, 3× NVMe
**Kernel:** Liquorix 6.19.6-1 (PDS)
**Workload:** X-Plane 12 (ToLiss A320) + XEarthLayer v0.3.1 (GPU=integrated) + OBS → YouTube Streaming + QEMU
**Route:** EDDM (München) → EDDH (Hamburg), ~600 km, FL350-360, Kurs NNW

---

## 0. Testbedingungen

| Parameter | Wert | Run T Referenz |
|-----------|------|----------------|
| Dauer | 115 Min (6.730 vmstat Samples) | 90 Min |
| Sidecar | Ja (bpftrace: Reclaim, Slow IO, Fence) | Ja |
| zram | **16 GB lz4** (aktiv) | 16 GB lz4 |
| zswap | **deaktiviert** (Plan war zswap — nicht umgestellt) | — |
| irqbalance | **nicht aktiv** (nicht gestartet) | nicht aktiv |
| vm.swappiness | 8 | 8 |
| vm.min_free_kbytes | **66 MB (Default!)** | **2 GB** |
| vm.watermark_boost_factor | 0 (Liquorix) | 0 |
| vm.watermark_scale_factor | **10 (Default!)** | **125** |
| vm.vfs_cache_pressure | 100 (Default) | 60 |
| vm.dirty_bg/ratio | 10/20 (Default) | 3/10 |
| XEL memory_size | **4 GB** | **2 GB** |
| XEL cpu_concurrent | 20 | 20 |
| XEL max_concurrent_jobs | 32 | 32 |
| XEL compressor | gpu (integrated) | gpu (integrated) |
| OBS Streaming | **Ja (YouTube)** | Nein |

**WICHTIG:** Die sysctl-Werte stehen noch auf den Defaults aus Änderung 17 (Normalisierung nach Run T). `min_free_kbytes=66MB` statt 2 GB und `watermark_scale_factor=10` statt 125 bedeuten minimalen kswapd-Headroom. Kombiniert mit `memory_size=4GB` (statt 2GB) ist mehr Memory Pressure vorprogrammiert.

---

## 1. Erwartungen vs. Ergebnisse

| Metrik | Run T (Referenz) | Run W (kein zram) | **Run X (erwartet)** | **Run X (gemessen)** | Bewertung |
|--------|-----------------|-------------------|----------------------|----------------------|-----------|
| Main Thread Reclaim | **0** | 54.686 | **0** | **12.472** | REGRESSION |
| allocstall Samples | 1 | 77 | ≤ 5 | **38** | REGRESSION |
| allocstall max/s | 3.715 | 12.243 | — | **7.738** | Mittel |
| Slow IO | 236 | 1.468 | < 500 | **30** | BESSER |
| FPS < 25 | 3,1% | 4,6% | ≤ 3,5% | **6,93%** | REGRESSION |
| FPS < 20 | 2,1% | 3,0% | — | **3,71%** | REGRESSION |
| DMA Fence | 0 | 0 | 0 | **7.348 (kworker)** | NEU |
| CB Trips | 0 | 0 | 0 | **0** | OK |
| EMFILE | 0 | 0 | 0 | **0** | OK |

**Hauptursache der Regression:** `min_free_kbytes=66MB` (Default) statt 2 GB. kswapd hat keinen Headroom und kommt bei Lastspitzen nicht nach → Direct Reclaim fällt auf X-Plane Main Thread. Der FUSE-Patch allein reicht nicht — er schützt nur vor FUSE-Read-induziertem Reclaim, nicht vor DSF-Loading-Reclaim.

---

## 2. Kernbefunde

### 2.1 Drei-Phasen-Verhalten

| Phase | Zeitraum | Dauer | allocstalls | Beschreibung |
|-------|----------|-------|-------------|-------------|
| Warm-up | 0 – 0.2 min | ~14s | 2 (isoliert) | System füllt Caches |
| Stabile Phase | 0.3 – 42.2 min | **42 min** | **0** | Kein Reclaim, System im Gleichgewicht |
| Stress-Phase | 42.2 – 97.5 min | **55 min** | **36** | Cluster bei DSF-Crossings/Scene Loads |
| Erholung | 97.5 – 115 min | **17 min** | 0 | System stabilisiert, Swap fällt |

Bemerkenswert: 42 Minuten komplett stall-frei vor dem ersten schweren Event. Run T hatte nur 1 Stall in 90 Min. Die Regression setzt erst ein, wenn der Working Set den kswapd-Headroom übersteigt.

### 2.2 Memory Pressure

| Metrik | Start | Peak Stress | Ende |
|--------|-------|-------------|------|
| available_mb | 57.761 | 42.098 (Min) | 67.849 |
| swap_used_mb | 4.940 | **9.110** (t=70 min) | 3.751 |
| cached_mb | 54.716 | ~43.500 | 30.855 |
| dirty_mb (max) | — | **249 MB** | — |

- **Swap Swing:** 4.885 MB (Max 9.110 - Min 4.225)
- **Page Cache Abbau:** -23.782 MB (54.716 → 30.934) — File Cache wird für Anon-Seiten geopfert
- **Kritischer Moment (t=50 min):** Swap springt in 33s von 5.873 auf 8.084 MB (+2.211 MB)
- **PSI:** Alle Werte 0,00% über die gesamte Session — zram federt den Druck ab

### 2.3 XEarthLayer Streaming Activity

| Metrik | Wert |
|--------|------|
| Jobs gesamt | 34.725 (0 Fehler) |
| ZL12 Tiles | 28.174 (81%) |
| ZL14 Tiles | 6.551 (19%) |
| Median Generierungszeit | 16,7s (Queue-Wartezeit dominiert) |
| P95 Generierungszeit | 25s |
| Prewarm EDDM | 1.055 Tiles in 84s (12.150 patch-excluded) |
| Prefetch Modus | Opportunistic |
| Boundary Prefetch Zyklen | 105 (162 Batch-Submits) |
| ERRORs | 2 (empty DDS data → Placeholder) |
| WARNs "Failed to submit" | **23.271** (47% der Submits) |

**Prefetch-Backpressure:** 23.271 von ~49.287 Prefetch-Submits scheitern mit "Failed to submit job — executor may be shutdown". Der Executor-Channel ist für das Boundary-Prefetch-Volumen zu klein. Dies ist kein Fehler im engeren Sinne (Backpressure funktioniert), aber das Log-Volume ist problematisch.

### 2.4 Direct Reclaim (bpftrace)

**Gesamt: 23.952 Events**

| Prozess | Events | Anteil | Max Latenz |
|---------|--------|--------|------------|
| **Main Thread (X-Plane)** | **12.472** | **52,1%** | **10.036 µs (10,0 ms)** |
| tokio-rt-worker (XEL) | 8.985 | 37,5% | **47.000 µs (47 ms!)** |
| cuda-EvtHandlr | 1.193 | 5,0% | — |
| bpftrace | 171 | 0,7% | — |
| threaded-ml | 151 | 0,6% | — |

**Hauptcluster:** 13:31 UTC (t≈17 min relativ zum XEL-Start, t≈51 min relativ zum Monitoring-Start) — 19.825 Events in einer Minute. Korreliert mit Takeoff/Scene-Loading-Phase.

**Vergleich:**
- Run T: 753 total, **0 auf Main Thread** → FUSE-Patch wirkte perfekt
- Run W: 72.644 total, 54.686 auf Main Thread → ohne zram katastrophal
- **Run X: 23.952 total, 12.472 auf Main Thread** → zram hilft, aber kswapd-Headroom fehlt

### 2.5 Alloc-Stall-Cluster

| Cluster | Zeitraum (rel. Min) | Events | Max stalls/s | Korrelation |
|---------|---------------------|--------|-------------|-------------|
| **C1 (kritisch)** | 42.26 | 2 | 3.814 | DSF-Crossing / Scene Load |
| **C2 (KRITISCH)** | 50.19 – 50.99 | 7 | **7.738** | **Takeoff + Scenery-Neuladung** |
| C3 | 55.94 – 56.45 | 4 | 2,0 | Post-Takeoff stabilisiert |
| C4 | 57.47 – 58.88 | 5 | 2,9 | Cruise-Transition |
| C5 | 60.28 – 63.76 | 6 | 2,9 | Cruise DSF-Crossings |
| C6 | 74.73 – 74.85 | 3 | 9,8 | DSF-Boundary |
| C7 | 77.75 – 80.81 | 5 | 18,6 | DSF-Boundary (schwerer) |
| C8 | 89.90 | 1 | 1,0 | Isoliert |
| C9 | 97.02 – 97.52 | 3 | 9,1 | Letzter Stall |

---

## 3. In-Sim Telemetrie (FPS / CPU Time / GPU Time)

| Metrik | Wert |
|--------|------|
| FPS Mittel | 30,1 |
| FPS Median | 29,8 |
| FPS Min | 19,9 |
| FPS Max | 91,7 |
| FPS P5 | 22,8 |
| FPS P95 | 36,4 |
| FPS < 25 | **2.163 Samples (6,93%)** |
| FPS < 20 | **1.158 Samples (3,71%)** — alle exakt 19,9 (harter Cap) |

**Bottleneck-Verteilung:**
- GPU-bound: 18.085 Frames (58,0%) — GPU Time Avg 18,62 ms
- CPU-bound: 13.120 Frames (42,0%) — CPU Time Avg 15,46 ms
- **Überwiegend GPU-bound** (OBS NVENC + X-Plane Rendering)

**FPS < 20 Cluster:**

| Phase | Zeitraum (rel. Min) | Samples | Ursache |
|-------|---------------------|---------|---------|
| Scene Load | 17–20 | 244 | DSF-Loading + Reclaim-Cluster |
| Climb | 42–57 | 355 | Neue Scenery-Regionen (korreliert mit C1-C4) |
| Cruise | 65–66 | 82 | DSF-Boundary-Crossing |
| Descent | 73–74 | 66 | DSF-Boundary (korreliert mit C6) |
| Approach | 83 | 44 | DSF-Loading |
| Final/Landing EDDH | 96–102 | 161 | Szenerie-Komplexität + DSF |

**Paused:** 5 Samples am Session-Ende (114,2 min)

### Flugphasen (XEL-Log)

| Phase | Zeitraum (UTC) | Dauer |
|-------|----------------|-------|
| Ground EDDM | 11:34 – 12:19 | 45 min |
| Transition (Takeoff) | 12:19:03 | GS=54,9 kt, MSL=1455 ft |
| Cruise | 12:19:58 – 13:23:46 | 64 min |
| Ground EDDH | 13:23:46 – 13:35 | 12 min |

---

## 4. GPU / VRAM

| Metrik | Min | Max | Avg |
|--------|-----|-----|-----|
| VRAM Used | 2.163 MiB | **22.910 MiB (93,3%)** | 17.461 MiB |
| GPU Util | 0% | 99% | 67,0% |
| Temperatur | — | **64°C** | — |
| Power | — | **298,8 W** | 223,6 W |
| Throttle | — | **0** | — |

- **VRAM Peak 22.910 MiB bei t=51,6 min** (93,3% von 24.564 MiB) — höchster Wert aller Runs
- Performance State: 98,4% in P2 (mittlerer 3D-Modus)
- Kein Throttling (weder thermisch noch Power)

---

## 5. Disk IO

### Per-Device Throughput

| Device | Read Max (MB/s) | Read Avg (MB/s) | Write Max (MB/s) | Write Lat Max (ms) |
|--------|----------------|-----------------|------------------|-------------------|
| nvme0n1 | 3.376 | 5,2 | 649 | 27,0 |
| nvme1n1 | 3.364 | 4,7 | 13,5 | 4,0 |
| nvme2n1 | 3.381 | 5,7 | 579 | 27,0 |

### Read-Spikes > 100 MB/s
- **553 Samples** auf 229 Zeitpunkten
- **Größter Burst (t=50 min):** 139 Samples, alle 3 NVMe parallel > 3.300 MB/s — Scenery-Neuladung

### Slow IO (bpftrace, > 5ms)
- **Gesamt: 30 Events** — **BESTER WERT ALLER RUNS**
- 29 Events 5–9 ms, 1 Event 10 ms
- Alle zwischen 13:28–13:47 UTC (Scene Loading)
- Kein 10–11ms Power-State-Pattern erkennbar

### NVMe Power-State Wakeup
- 8 Write-Events > 10ms in io.csv (Kaltstart + Session-Ende)
- NVMe PM QOS scheint zu wirken (kein chronisches Pattern)

---

## 6. CPU & Frequenz

| Metrik | Wert |
|--------|------|
| Frequenz Min | 2.009 MHz |
| Frequenz Max | 6.384 MHz |
| Frequenz Avg | 4.589 MHz |
| Samples < 3.500 MHz | 24,9% (Idle-Cores) |

Kein thermisches Throttling. Die niedrigen Frequenzen sind normales AMD CPPC Power Management auf inaktiven Kernen.

---

## 7. Per-Process

| Prozess | CPU Max | CPU Avg | RSS Max | Threads Max | IO Read Max |
|---------|---------|---------|---------|-------------|-------------|
| **Main Thread (X-Plane)** | 1.546% | 329% | **19.305 MB** | 192 | 6.249 MB/s |
| **xearthlayer** | 789% | 88% | **17.793 MB** | 552 | 1.046 MB/s |
| qemu-system-x86 | 184% | 21% | 4.248 MB | 34 | — |
| gamescope-wl | — | 1,2% | 177 MB | — | — |
| OBS | — | **nicht getrackt** | — | — | — |

**XEL RSS 17.793 MB bei memory_size=4GB** — deutlich über dem konfigurierten Wert. Run T mit memory_size=2GB hatte ~14.4 GB RSS. Der +3.4 GB Anstieg ist konsistent mit der memory_size-Verdopplung.

**XEL Thread-Verlauf:** Start 399 → Peak 552 (Prewarm) → Stable 41 → Ende 25. Keine Thread-Bomb.

**OBS** wurde von sysmon.py nicht getrackt (kein Pattern-Match). GPU-Encoder-Overhead nur indirekt über GPU Util und VRAM sichtbar.

---

## 8. Vergleich Run T → Run X

| Metrik | Run T | **Run X** | Delta | Ursache |
|--------|-------|-----------|-------|---------|
| Main Thread Reclaim | **0** | **12.472** | **↑↑↑↑↑** | min_free_kbytes 2GB→66MB |
| allocstall Samples | 1 | 38 | **↑↑↑↑** | kswapd-Headroom fehlt |
| FPS < 25 | 3,1% | 6,93% | **↑↑ (×2,2)** | Reclaim + OBS-Last |
| FPS < 20 | 2,1% | 3,71% | **↑ (×1,8)** | DSF-Loading + Reclaim |
| Direct Reclaim total | 753 | 23.952 | **↑↑↑↑ (×32)** | Fehlender kswapd-Headroom |
| XEL RSS Peak | ~14,4 GB | 17,8 GB | **+3,4 GB** | memory_size 2→4 GB |
| Swap Peak | 7.667 MB | 9.110 MB | +19% | Mehr Anon-Seiten |
| Slow IO | 236 | **30** | **↓↓↓ (-87%)** | Exzellent |
| EMFILE | 0 | 0 | = | OK |
| CB Trips | 0 | 0 | = | OK |
| DMA Fence | 0 | 7.348 (kworker) | NEU | Harmlos (nur Kernel-Worker) |
| Tiles generiert | ~2.701 | 34.725 | ↑↑↑ | Mehr Cold-Tiles auf Route |

### Ursachenanalyse der Regression

Die Regression hat **zwei kumulative Ursachen:**

1. **min_free_kbytes = 66 MB (Default) statt 2 GB:**
   Der kswapd hat bei 66 MB Headroom keine Chance, proaktiv zu reclaimen. Bei Lastspitzen (DSF-Loading, Scenery-Wechsel) fällt Direct Reclaim sofort auf den anfragenden Thread — oft X-Plane's Main Thread. Dies war genau das Problem aus Run G, das durch Änderung 5 (min_free_kbytes=2GB) gelöst wurde.

2. **memory_size = 4 GB statt 2 GB:**
   XEL RSS steigt von ~14,4 auf 17,8 GB (+3,4 GB). Bei 91 GB Gesamt-RAM sind X-Plane (19,3 GB) + XEL (17,8 GB) + QEMU (4,2 GB) + cached (31-55 GB) + Kernel = ~100 GB Arbeitslast. Der Page Cache wird von 54,7 auf 30,9 GB abgebaut (-23,8 GB), was unter Last zum Reclaim führt.

**OBS/YouTube-Streaming** ist ein **erschwerender Faktor** (zusätzlicher GPU-Encoder + Netzwerk), aber nicht die Hauptursache.

---

## 9. Handlungsempfehlungen

### 9.1 [CRITICAL] min_free_kbytes auf 2 GB zurücksetzen

```bash
echo 2097152 | sudo tee /proc/sys/vm/min_free_kbytes
```

Persistent in `/etc/sysctl.d/99-custom-tuning.conf`:
```
vm.min_free_kbytes = 2097152
```

**Begründung:** Die Normalisierung aus Änderung 17 war verfrüht. Der FUSE-Patch schützt vor FUSE-Read-Reclaim, aber nicht vor DSF-Loading-Reclaim. min_free_kbytes=2GB gibt kswapd den Headroom, proaktiv zu reclaimen bevor Direct Reclaim auf den Main Thread fällt.

### 9.2 [CRITICAL] watermark_scale_factor auf 125 zurücksetzen

```bash
echo 125 | sudo tee /proc/sys/vm/watermark_scale_factor
```

Persistent:
```
vm.watermark_scale_factor = 125
```

**Begründung:** Arbeitet zusammen mit min_free_kbytes. Breitere Watermarks geben kswapd mehr Spielraum.

### 9.3 [WARNING] memory_size auf 2 GB zurücksetzen

In `~/.xearthlayer/config.ini`:
```ini
memory_size = 2 GB
```

**Begründung:** Run T mit 2 GB hatte 0 Main Thread Reclaim. Die 4 GB erzeugen +3,4 GB RSS ohne erkennbaren Benefit für FPS oder Tile-Generierung. Bei 34.725 generierten Tiles (vs. 2.701 in Run T) ist der Cache-Hit-Vorteil durch die Route (mehr Cold-Tiles) verwischt.

### 9.4 [INFO] OBS-Prozess in sysmon.py tracken

sysmon.py mit `-p obs` starten oder OBS-Pattern in den Default-Patterns ergänzen, damit CPU/RSS/GPU-Overhead von OBS quantifiziert werden kann.

### 9.5 [INFO] Prefetch "Failed to submit" Log-Level reduzieren

23.271 WARN-Meldungen (47% aller Submits) fluten das Log. Das ist funktional korrekt (Backpressure), aber das Log wird unbrauchbar. Empfehlung: Log-Level von WARN auf DEBUG senken, oder einen Counter statt Einzel-Meldungen loggen.

### 9.6 [INFO] zswap als zram-Alternative testen (verschoben)

Der geplante zswap-Test hat nicht stattgefunden (zswap war deaktiviert). Für einen validen Vergleich müssten die sysctl-Werte erst auf Run-T-Niveau stehen. Empfehlung: Erst 9.1-9.3 umsetzen, dann einen sauberen zswap-Run planen.

### 9.7 [INFO] XEL Generierungszeit-Verteilung untersuchen

58% der Tiles brauchen > 10s (Median 16,7s). Die eigentliche Generierung ist vermutlich schneller, aber Queue-Wartezeit treibt die Gesamt-Dauer. Bei 34.725 Tiles und max_concurrent_jobs=32 ist Queueing erwartbar, aber die Verteilung (35,5% bei 20-30s) deutet auf systematischen Stau hin.

---

## 10. Zusammenfassung

Run X war als Bestätigungsrun für zram + irqbalance geplant, hat aber weder zswap getestet noch irqbalance gestartet. Stattdessen offenbart er, dass die **sysctl-Normalisierung aus Änderung 17 zu weit ging**: `min_free_kbytes=66MB` (Default) statt 2 GB und `watermark_scale_factor=10` statt 125 entziehen kswapd den nötigen Headroom. In Kombination mit `memory_size=4GB` (statt 2 GB) und OBS-Streaming kehrt Main Thread Reclaim zurück (12.472 Events, 52% aller Reclaims).

**Positiv:**
- Slow IO: 30 Events — bester Wert aller Runs (NVMe PM QOS stabil)
- 0 EMFILE, 0 CB-Trips, 0 GPU-Throttle, 0 PSI
- 34.725 Tiles fehlerfrei generiert
- 42 Minuten komplett stall-frei vor erstem Event
- Erholung am Session-Ende (17 Min stall-frei, Swap fällt)

**Negativ:**
- Main Thread Reclaim: 12.472 (Run T: 0)
- FPS < 25: 6,93% (Run T: 3,1%)
- 23.271 Prefetch-Submit-Warnungen (Log-Flut)
- tokio-rt-worker Reclaim-Spike: 47 ms (14:18 UTC)

**Nächster Schritt:** sysctl-Werte (min_free_kbytes, watermark_scale_factor) und memory_size zurücksetzen, dann einen sauberen Bestätigungsrun (Run Y) mit dem validierten Run-T-Stack + irqbalance durchführen.
