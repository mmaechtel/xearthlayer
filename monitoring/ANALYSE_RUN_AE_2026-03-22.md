# Run AE — Ergebnisse: 120-Minuten Europa-Langflug LSZH→EHAM

**Datum:** 2026-03-22
**System:** Ryzen 9 9800X3D 8C/16T, 96 GB RAM, RTX 4090 24 GB, 3× NVMe (2× SN850X 8TB + 990 PRO 4TB)
**Kernel:** Liquorix 6.19.6-1 (PDS), Btrfs RAID0 (xplane_data) + RAID1 (home)
**Workload:** X-Plane 12 (ToLiss A320), XEarthLayer (GPU/Vulkan), QEMU/KVM, Firefox, gamescope/kwin

**Aenderungen seit Run AD:**
1. XEL `memory_size` 2 GB → **4 GB** (groesserer Memory Cache)
2. `min_free_kbytes` 3 GB → **1 GB** (reduzierte Emergency Reserve)
3. `watermark_scale_factor` 125 → **500** (praeziseres kswapd-Tuning)
4. zram weiterhin deaktiviert

---

## 0. Testbedingungen

| Parameter | Wert |
|-----------|------|
| Dauer | 120 Min (7.042 vmstat-Samples, 35.212 mem-Samples, 32.438 Telemetrie-Samples) |
| Route | LSZH (Zuerich) → EHAM (Amsterdam Schiphol) |
| Flughoehe | FL370, max 470 kt GS |
| Provider | Bing Maps |
| XEL Modus | Opportunistic (kalibriert) |
| Sidecar | Ja (3 bpftrace-Tracer) |
| Tiles generiert | 13.457 (9.119 Prefetch + 4.338 On-Demand) |
| Disk Cache | 244 GB / 420 GB (19,6 Mio Dateien) |
| Memory Cache | 4 GB (neu) |
| min_free_kbytes | 1 GB (neu, von 3 GB) |
| watermark_scale_factor | 500 (neu, von 125) |

---

## 1. Erwartungen vs. Ergebnisse

| Metrik | Ziel Run AE | Run AD | **Run AE** | Bewertung |
|--------|-------------|--------|------------|-----------|
| Main Thread Reclaim | **0** | 0 | **10.057** | REGRESSION |
| Max Reclaim-Latenz | 0 ms | 0 ms | **14,5 ms** | REGRESSION |
| allocstall Sum | < 71.630 | 71.630 | **11.425** | -84% BESSER |
| FPS < 25 | ≈ 3,5% | 3,58% | **3,4%** | ≈ |
| XEL RSS Peak | < 15 GB | 15.828 MB | **17.449 MB** | +10% SCHLECHTER |
| Swap Peak | < 11.715 MB | 11.715 MB | **18.064 MB** | +54% SCHLECHTER |
| wset_refault_anon | < 30% | 44,8% | **87%** | VERSCHLECHTERT |
| available_mb min | > Run AD | ~27.000 | **27.454 MB** | ≈ |
| Slow IO (>5ms) | < 5.816 | 5.816 | **5.511** | -5% ≈ |
| Fence Events | < 11.135 | 11.135 | **4.953** | -56% BESSER |
| CB Trips | 0 | 0 | **0** | OK |

**Gesamtbewertung: REGRESSION** — Direct Reclaim kehrte auf den Main Thread zurueck (10K Events). Die Watermark-Umstellung (min_free_kbytes 3GB→1GB + wsf 500) schuetzt NICHT so gut wie 3 GB min_free_kbytes.

---

## 2. Kernbefunde

### 2.1 Drei-Phasen-Verhalten

| Phase | Zeitfenster | Dauer | Charakteristik |
|-------|-------------|-------|----------------|
| Warm-up | Min 0–45 | 45 Min | Swap stabil ~9.900 MB, kein Pressure, Ground+Cruise-Start |
| Ramp-up | Min 45–95 | 50 Min | Swap waechst 9.950→17.200 MB (+7,3 GB), XEL RSS springt auf 15 GB |
| Plateau | Min 95–120 | 25 Min | Swap oszilliert 16.900–17.700 MB, Approach-Stutter |

**Inflektionspunkt:** Minute 50 — XEL beginnt Tile-Generierung (ground→cruise Transition bei Min 48).

### 2.2 Memory Pressure — KRITISCH

**Die Watermark-Umstellung ist fehlgeschlagen:**

- `min_free_kbytes=1GB + wsf=500` ergibt theoretisch WMARK_HIGH ≈ 10,6 GB
- In der Praxis: **10.057 Direct Reclaim Events auf dem Main Thread** (Run AD: 0)
- Zwei schwere Reclaim-Stuerme:
  1. **13:41:47 UTC** — Burst, max 5,8 ms, waehrend laengstem kswapd-Wach-Intervall (6s)
  2. **14:00:41-42 UTC** — **6.059 Events in ~1 Sekunde**, max 14,5 ms, 10 Events > 10 ms
- 87,6% aller Reclaim-Events treffen den X-Plane Main Thread

**Swap-Thrashing ist chronisch:**
- 87% aller Samples haben pswpin > 0 (Run AD: aehnlich)
- 87% aller Samples haben wset_refault_anon > 0 (Ziel war < 30%)
- Swap Swing: 9.956 MB (kritisch, > 5 GB Schwelle)
- pswpout Peak: 369.759 pages/s (Panik-Swapping)

**Ursache:** Combined RSS (X-Plane 22 GB + XEL 17,4 GB + QEMU 4,3 GB ≈ 43,7 GB) plus 35-43 GB Page Cache = deutlich ueber 94 GB Physical RAM. Der Kernel muss permanent zwischen Anon und File Cache balancieren.

### 2.3 XEarthLayer Streaming Activity

| Metrik | Wert |
|--------|------|
| Tiles gesamt | 13.457 |
| Peak-Burst | 2.169 Tiles / 5 Min (12:50 UTC, Cruise ueber neuem Gebiet) |
| Circuit Breaker | 0 Trips |
| Download Retries | 0 |
| Cache Hit Rate (Ground EHAM) | 75,1% (1063/1263 Tiles gecacht) |
| Errors | 0 |

**RSS-Verlauf:**
- Phase 1 (Min 0–50): RSS sinkt von 8,6 auf 6,7 GB (idle, Cache schrumpft)
- Phase 2 (Min 50–60): Sprung von 6,7 auf 14,3 GB (+7,6 GB in 10 Min)
- Phase 3 (Min 60–120): Oszilliert 14.000–17.449 MB, kein monotones Wachstum

**Peak RSS: 17.449 MB bei Min 111** (Approach EHAM) — SCHLECHTER als Run AD (15.828 MB).
Die 4-GB-Cache-Erhoehung hat das RSS-Problem nicht geloest, sondern verschaerft.

**Thread-Count:** Baseline 38, Spitzen bis 549 (gleich wie Run AC/AD).

### 2.4 Direct Reclaim — REGRESSION

| Metrik | Run AD | Run AE | Delta |
|--------|--------|--------|-------|
| Main Thread Events | **0** | **10.057** | REGRESSION |
| Max Latenz | 0 ms | 14,5 ms | REGRESSION |
| tokio-rt-worker Events | — | 1.105 | NEU |
| Gesamt-Events | ~0 | 11.476 | REGRESSION |

**Verteilung der Main Thread Reclaim:**
- < 1 ms: 9.030 (89,8%)
- 1–2 ms: 730 (7,3%)
- 2–5 ms: 260 (2,6%)
- 5–10 ms: 27 (0,27%)
- > 10 ms: 10 Events (max 14,5 ms) — Frame-killend

**Hauptburst um 14:00:41-42 UTC:** 6.059 Events in ~1s. Gleichzeitig: 416 Slow-IO-Events bei 10-11 ms (NVMe Power-State Transition) + kswapd-Wach-Burst. Dirty Pages Writeback trifft NVMe-Aufwach-Latenz.

### 2.5 Alloc-Stall-Cluster

Nur **3 Einzelspikes** (keine sustained Cluster):

| # | Timestamp | allocstall_s | pgscan_direct_s | Kontext |
|---|-----------|-------------|-----------------|---------|
| 1 | 1774178711 (Min 0,1) | 334 | 78.702 | Startup |
| 2 | 1774183308 (Min 77) | 5.076 | 980.530 | Ramp-up Peak |
| 3 | 1774184442 (Min 96) | 6.015 | 717.380 | Approach-Beginn |

Im Vergleich zu Run AD (71.630 allocstalls ueber viele Samples): deutlich weniger Stalls, aber HAERTERE Einzelspikes mit Direct Reclaim.

---

## 3. In-Sim Telemetrie (FPS / CPU Time / GPU Time)

| Metrik | Wert |
|--------|------|
| FPS avg | 29,8 |
| FPS min | 19,9 |
| FPS p5 | 27,2 |
| FPS p25/p50/p75 | 29,3 / 29,8 / 30,1 |
| FPS < 25 | 3,4% (1.098 Samples) |
| FPS < 20 | 2,1% (681 Samples) |
| cpu_time avg | 17,2 ms |
| gpu_time avg | 17,0 ms |
| Bottleneck | Balanced (marginal CPU-bound) |

**Flugprofil:**
- Ground LSZH: Min 0–47 (lang, Taxi + Startup)
- Cruise: Min 48–109, FL370, avg 394 kt
- Approach EHAM: Min 109–114
- Ground EHAM: Min 114–119

**FPS-Drops konzentriert auf Approach (Min 103–114):**
- Min 110: 58 Sub-20-Samples (schwerster Cluster)
- Min 105: 23 Sub-20-Samples
- Dies korreliert mit dem Abstieg durch 3.000–1.600 m ueber den Niederlanden

**Letzte 20 Min:** FPS avg 29,4, min 19,9, 4,9% < 25, 2,5% < 20 — leicht schlechter als Session-Durchschnitt, konsistent mit Approach-Phase.

---

## 4. GPU / VRAM

| Metrik | Wert |
|--------|------|
| VRAM Peak | 22.020 MiB (89,6% von 24 GiB) |
| VRAM avg | 16.944 MiB |
| GPU Util avg | 59,1% |
| GPU Util max | 99% |
| Temp max | 62°C |
| Power avg | 208 W |
| Throttling | **Keine** |
| Perf State | P0 durchgehend |
| Fence Events | 4.953 (alle auf kworker, keine auf Main Thread) |

GPU ist gesund — keine Throttling-Events, moderate Temperatur, ausreichend VRAM-Headroom.

---

## 5. Disk IO

| Device | Read avg | Read max | Read p95 Lat | Write p95 Lat | IO Util avg |
|--------|----------|----------|-------------|--------------|-------------|
| nvme0n1 | 16,1 MB/s | 3.424 MB/s | 1,0 ms | 110,8 ms | 6,8% |
| nvme1n1 | 12,5 MB/s | 3.415 MB/s | 0,9 ms | 8,3 ms | 3,6% |
| nvme2n1 | 14,8 MB/s | 3.415 MB/s | 0,8 ms | 1,0 ms | 5,0% |

**NVMe 10-11ms Cluster:** 820 Events, davon 416 in einem einzigen Burst um 14:00:42 (korreliert mit Reclaim-Storm). Ansonsten unauffaellig.

**nvme0n1 Write-Latenz** auffaellig (p95 110 ms, max 1.184 ms) — vermutlich OS-Journal/Sync-Flushes, nicht XEL-bedingt.

---

## 6. CPU & Frequenz

- X-Plane: avg 383%, max 1.542% (multi-threaded Bursts), 100–138 Threads
- XEL: avg 36%, max 806% (Encoding-Bursts), 38–549 Threads
- QEMU: avg 21%, stabil
- Balanced CPU/GPU-Bottleneck (17,2 ms vs 17,0 ms)

---

## 7. Per-Process

| Prozess | RSS Start | RSS End | RSS Peak | Swap Peak | Threads |
|---------|-----------|---------|----------|-----------|---------|
| X-Plane | 10.933 MB | 21.819 MB | 22.980 MB | 2.096 MB | 104–138 |
| xearthlayer | 8.351 MB | 15.330 MB | 17.449 MB | 4.092 MB | 38–549 |
| QEMU | 4.274 MB | 4.151 MB | 4.290 MB | 5 MB | 9–73 |
| Firefox | — | — | 3.310 MB | — | 272–424 |

**Combined RSS (X-Plane + XEL + QEMU) Verlauf:**

| Minute | X-Plane | XEL | QEMU | Combined |
|--------|---------|-----|------|----------|
| 0 | 11.323 | 8.632 | 4.275 | **24.230** |
| 30 | 17.198 | 6.790 | 4.288 | **28.276** |
| 60 | 19.337 | 14.269 | 4.269 | **37.875** |
| 90 | 22.870 | 14.623 | 4.118 | **41.611** |
| 120 | 21.820 | 15.330 | 4.151 | **41.301** |

Peak Combined: ~43,7 GB (bei Min 111).

---

## 8. Vergleich Run AD → Run AE

| Metrik | Run AD | Run AE | Delta | Bewertung |
|--------|--------|--------|-------|-----------|
| Dauer | 150 Min | 120 Min | -20% | |
| Route | LFBO→EDWI | LSZH→EHAM | Vergleichbar (Europa) | |
| Main Thread Reclaim | **0** | **10.057** | REGRESSION | |
| Max Reclaim-Latenz | 0 ms | 14,5 ms | REGRESSION | |
| allocstall Sum | 71.630 | 11.425 | **-84%** BESSER | |
| allocstall Samples >0 | viele | 3 (0,04%) | **BESSER** | |
| FPS < 25 | 3,58% | 3,4% | -5% ≈ | |
| XEL RSS Peak | 15.828 MB | 17.449 MB | +10% schlechter | |
| X-Plane RSS Peak | 21.300 MB | 22.980 MB | +8% | |
| Combined RSS Peak | ~37.146 MB | ~43.700 MB | +18% schlechter | |
| Swap Peak | 11.715 MB | 18.064 MB | +54% SCHLECHTER | |
| Swap Swing | — | 9.956 MB | KRITISCH | |
| wset_refault_anon | 44,8% | 87% | +94% SCHLECHTER | |
| Slow IO | 5.816 | 5.511 | -5% ≈ | |
| Fence Events | 11.135 | 4.953 | **-56%** BESSER | |
| CB Trips | 0 | 0 | ≈ | |
| Tiles | — | 13.457 | — | |

### Analyse der Diskrepanz

**Warum weniger allocstalls ABER mehr Direct Reclaim?**

Run AD hatte `min_free_kbytes=3GB`: kswapd startet sehr frueh → viele "weiche" allocstalls (Threads warten kurz auf kswapd), aber kswapd schafft es immer rechtzeitig → 0 Direct Reclaim.

Run AE hat `min_free_kbytes=1GB + wsf=500`: Theoretisch WMARK_HIGH ≈ 10,6 GB, aber:
- Die tatsaechliche kswapd-Aufwach-Schwelle ist NIEDRIGER als bei 3 GB min_free_kbytes
- Bei grossen Allokations-Bursts (X-Plane DSF-Loading, XEL Encoding) reicht der kswapd-Vorlauf nicht
- Ergebnis: Weniger allocstalls (kswapd wird seltener noetig), aber wenn er noetig wird, kommt er zu spaet → Direct Reclaim

**Die Hypothese "wsf=500 ersetzt min_free_kbytes=3GB" ist widerlegt.**

---

## 9. Handlungsempfehlungen

### 9.1 [KRITISCH] min_free_kbytes auf 3 GB zuruecksetzen

Die Watermark-Umstellung war der primaere Fehler. `min_free_kbytes=3GB + wsf=125` (Run AD Stack) hatte 0 Direct Reclaim ueber 150 Min.

```bash
sudo sysctl vm.min_free_kbytes=3145728
sudo sysctl vm.watermark_scale_factor=125
```

**Why:** wsf=500 aktiviert kswapd frueher, aber min_free_kbytes=1GB laesst den Notfall-Puffer zu klein. Bei Burst-Allokationen (X-Plane DSF-Load) wird der 1-GB-Puffer schneller durchbrochen als kswapd nachliefern kann.

### 9.2 [HOCH] memory_size=4GB beibehalten

Die 4-GB-Cache-Erhoehung hat das RSS-Problem nicht geloest (17,4 GB vs 15,8 GB), aber auch nicht signifikant verschlechtert. Der Anstieg koennte routenabhaengig sein (EHAM hat dichtere Scenery als EDWI). Fuer eine faire Bewertung: Run mit 4 GB + zurueckgesetzten Watermarks wiederholen.

### 9.3 [MITTEL] XEL RSS-Wachstum weiter untersuchen

XEL RSS waechst auf 17,4 GB bei 4 GB Config (4,4× Faktor). Ursachen:
- Encoding-Bursts (~90 MB/Task × bis zu 128 concurrent Tasks)
- FUSE-Buffer-Copies
- Thread-Stack-Memory (549 Threads × ~8 MB Stack = bis zu 4,4 GB)

Thread-Pool-Ceiling auf z.B. 128 Threads begrenzen koennte helfen.

### 9.4 [NIEDRIG] NVMe Power-State untersuchen

Der 10-11ms IO-Cluster (820 Events, 416 davon im Reclaim-Storm) deutet auf NVMe PS3/PS4-Aufwach-Latenz. Unter Memory Pressure verstaerkt dies den Stall, da Dirty-Page-Writeback auf NVMe-Aufwachen warten muss.

---

## 10. Zusammenfassung

Run AE testet zwei Aenderungen: XEL Cache 2→4 GB und Watermark-Tuning (min_free_kbytes 3GB→1GB + wsf 500). **Das Watermark-Experiment ist gescheitert** — Direct Reclaim kehrte auf den Main Thread zurueck (10.057 Events, max 14,5 ms), obwohl die Gesamt-allocstalls um 84% sanken.

Die FPS blieben auf Run-AD-Niveau (3,4% < 25), das GPU-Subsystem ist gesund (kein Throttling, P0 durchgehend), und die IO-Latenz ist unauffaellig. Die Ruckler vor dem Aufsetzen (EHAM Approach) korrelieren mit:
1. Swap-Thrash-Stuerme (1,4 Mio pgfaults/s Peak)
2. XEL-RSS bei 17,4 GB waehrend Approach-Tile-Generation
3. Combined RSS ~43,7 GB + Page Cache = deutlich ueber Physical RAM

**Naechster Schritt (Run AF):** Gleiche Route, aber `min_free_kbytes=3GB + wsf=125` zurueck (Run-AD-Stack), `memory_size=4GB` beibehalten. Ziel: Bestaetigen dass die Watermark-Aenderung der einzige Regressionsgrund war.
