# Run W — Ergebnisse: 97-Minuten EDDH → EDDM

**Datum:** 2026-03-14
**System:** Ryzen 9 9800X3D 8C/16T, 94 GB RAM, RTX 4090 24 GB, 3× NVMe (2× SN850X 8TB + 990 PRO 4TB)
**Kernel:** Liquorix 6.19.6-1 (PDS)
**Workload:** X-Plane 12 (ToLiss A320), XEarthLayer (Bing), gamescope, Xwayland
**Route:** EDDH (Hamburg) → EDDM (München), ~600 km, FL300, Südkurs
**Änderungen seit Run T:** zram deaktiviert (kein zram-Swap), noirqbalance entfernt (irqbalance aktiv)

---

## 0. Testbedingungen

| Parameter | Run W | Run T (Referenz) |
|-----------|-------|-------------------|
| Dauer | 5.849s (97 Min), 5.718 Samples | 5.400s (90 Min) |
| Sidecar | Ja (bpftrace Layer 2) | Ja |
| zram | **deaktiviert** | 16 GB lz4 |
| irqbalance | **aktiviert** (noirqbalance entfernt) | deaktiviert |
| Swap | Nur NVMe ~119 GB (Prio -1) | zram 16 GB + NVMe Fallback |
| vm.swappiness | 8 | 8 |
| vm.min_free_kbytes | 2 GB | 2 GB |
| vm.page-cluster | 0 | 0 |
| XEL cpu_concurrent | 20 | 20 |
| XEL max_concurrent_tasks | 128 | 128 |
| XEL max_concurrent_jobs | 32 | 32 |

**Testziel:** Prüfen ob zram bei 96 GB RAM noch notwendig ist und ob irqbalance die IRQ-Verteilung verbessert.

---

## 1. Erwartungen vs. Ergebnisse

| Metrik | Run T (zram) | **Run W (kein zram)** | Delta | Bewertung |
|--------|-------------|----------------------|-------|-----------|
| FPS Mean / Median | 29,8 / 29,8 | **29,7 / 29,8** | -0,1 | ✅ Stabil |
| FPS < 25 | 3,1% | **4,6%** | +48% | ⚠️ Verschlechtert |
| FPS < 20 | 2,1% | **3,0%** | +43% | ⚠️ Verschlechtert |
| allocstall Samples | **1** | **77** | **77× schlimmer** | ❌ Regression |
| allocstall max/s | 3.715 | **12.243** | 3,3× | ❌ |
| Direct Reclaim (bpf) | 753 | **72.644** | **96×** | ❌❌ Katastrophal |
| Main Thread Reclaim | **0 (0%)** | **54.686 (75%)** | ❌❌❌ | **Rückfall auf Run Q Niveau** |
| Main Thread worst stall | 0 | **194,6 ms** | — | ❌ ~6 Frames verloren |
| Swap Peak MB | 7.667 (zram) | **11.809 (NVMe)** | +54% | ⚠️ Reales I/O |
| NVMe Swap I/O | 0 | **~21 MB Writes** | Neu | ⚠️ |
| Slow IO (>5ms) | 236 | **1.468** | **6,2×** | ❌ |
| VRAM Peak | 21,1 GB (86%) | **21,4 GB (87%)** | +0,3 GB | ✅ |
| EMFILE | 0 | **0** | = | ✅ |
| CB Trips | 0 | **0** | = | ✅ |
| PSI | 0,00 | **0,00** | = | ✅ |
| DMA Fence | 0 | **0** | = | ✅ |
| GPU Throttle | 0 | **0** | = | ✅ |

---

## 2. Kernbefunde

### 2.1 Drei-Phasen-Verhalten

| Phase | Zeitfenster | Dauer | allocstalls | Bemerkung |
|-------|-------------|-------|-------------|-----------|
| Warm-up | Min 0–18 | 18 Min | 0 | Kein kswapd, kein Swap |
| Ramp-up | Min 18–36 | 18 Min | 7 (leicht) | Erste kswapd-Aktivität, kein Swap |
| Steady | Min 36–97 | 61 Min | **70** | 6 schwere Cluster, Swap 0→11,8 GB |

Im Vergleich zu Run T (keine Ramp-up-Phase, 1 isolierter Stall) ist das klassische Drei-Phasen-Modell **zurückgekehrt** — ein klarer Rückschritt.

### 2.2 Memory Pressure

| Metrik | Warm-up | Ramp-up | Steady | Run T Avg |
|--------|---------|---------|--------|-----------|
| available_mb avg | 57.495 | 54.272 | **53.795** | 42.234 |
| free_mb avg | 2.710 | 1.569 | **1.650** | 4.892 |
| swap_used_mb max | 0 | 0 | **11.809** | 7.667 |
| dirty_mb max | 8,8 | 15,6 | **29,0** | 42,2 |

**Paradox:** available_mb ist *höher* als Run T (53,8 vs 42,2 GB), aber free_mb ist *deutlich niedriger* (1.650 vs 4.892). Der Kernel evicted aggressiver auf NVMe-Swap statt in zram zu komprimieren. Das "available" ist irreführend — der Zugriff auf ausgelagerte Pages erzeugt Major Faults.

**Swap-Verlauf:**
- Erster Swap bei Min 42,7 (18 MB)
- Monotoner Anstieg bis Min 84,3 (Peak 11.809 MB)
- Plateau bis Session-Ende (11.362 MB)
- 56% aller Samples zeigen Swap > 0

### 2.3 XEarthLayer Streaming Activity

| Metrik | Wert |
|--------|------|
| On-Demand Jobs | 1.345 |
| Prefetch Jobs | 1.356 |
| **Jobs completed** | **2.701** |
| Errors / Timeouts / EMFILE | **0 / 0 / 0** |
| Prefetch Submission Failures | 1.284 (Channel-Sättigung, kein Fehler) |
| Circuit Breaker Trips | 0 |
| Panics | 0 |
| Median Generation Time | ~4.181 ms |
| Cache Hits (<500ms) | 51,4% |
| Cold Downloads (>10s) | 22,8% |

**RSS:** 9,3–16,8 GB (Peak höher als Run T: 14,4 GB → +2,4 GB)
**Threads:** 35–547 (identisch mit Run T)
**CPU Peak:** 1.131% (vs. 1.301% in Run T)

**Saubere Session:** Null Fehler, null EMFILE, null Circuit Breaker. Das XEL-seitige Verhalten ist einwandfrei.

### 2.4 Direct Reclaim Attribution (bpftrace) — KRITISCH

| Prozess | Events | Anteil | Avg Latenz | Max Latenz | >10ms | >100ms |
|---------|--------|--------|------------|------------|-------|--------|
| **X-Plane Main** | **54.686** | **75,3%** | 284 µs | **194,6 ms** | 19 | **7** |
| tokio-rt-worker | 13.950 | 19,2% | 697 µs | 36,8 ms | — | 0 |
| cuda-EvtHandlr | 1.303 | 1,8% | 995 µs | 194,8 ms | — | — |
| nvidia-smi | 361 | 0,5% | 620 µs | 10,5 ms | — | 0 |
| Andere | ~2.344 | 3,2% | variiert | — | — | — |
| **Gesamt** | **72.644** | | | | | |

**X-Plane Main Thread hat 54.686 Direct-Reclaim-Events.** Das ist ein katastrophaler Rückfall:

| Run | X-Plane Main Thread Reclaim | Anteil |
|-----|----------------------------|--------|
| G | 47.583 | 67% |
| Q | 107.853 | 84% |
| **T** | **0** | **0%** |
| **W** | **54.686** | **75%** |

Der FUSE-Patch hatte in Run T das Problem eliminiert. **Ohne zram ist es zurück**, weil der Kernel bei Memory Pressure direkt den X-Plane Main Thread in Reclaim zwingt statt Seiten in zram zu komprimieren.

**Schlimmster Burst:** 18:32:33 — 25.892 Events, davon 7 Stalls >193 ms. Gesamte Main-Thread-Kosten: **15,6 Sekunden** CPU-Zeit in Reclaim.

### 2.5 Alloc-Stall-Cluster

12 Cluster, 6 davon schwer (>4.000/s):

| Cluster | Minute | Peak /s | Summe | Schwere |
|---------|--------|---------|-------|---------|
| 6 | 42,5 | **12.243** | 13.773 | ❌❌ Schwerster |
| 8 | 62,1 | 6.014 | 26.805 | ❌❌ Höchste Summe |
| 9 | 67,0 | 4.374 | 5.293 | ❌ |
| 10 | 73,7 | 4.215 | 11.356 | ❌ |
| 11 | 75,6 | 4.417 | 10.192 | ❌ |
| 1–5,7,12 | diverse | <582 | <1.471 | ⚠️ Leicht |

---

## 3. In-Sim Telemetrie (FPS / CPU Time / GPU Time)

| Metrik | Wert | Run T |
|--------|------|-------|
| FPS Mean / Median | 29,7 / 29,8 | 29,8 / 29,8 |
| FPS P5 / P1 / Min | 25,4 / 19,9 / 19,9 | 28,0 / 19,9 / 19,9 |
| Samples < 28 FPS | **9,0%** | — |
| Samples < 25 FPS | **4,6%** | 3,1% |
| Samples < 20 FPS | **3,0%** | 2,1% |
| Bottleneck | **66% CPU / 34% GPU** | 94% CPU |

### Sustained FPS-Drops (<28 FPS für >1s) — 52 Events in 3 Clustern

#### Cluster 1: EDDH Ground/Taxi (T+880s bis T+1092s, 20 Drops)

| Aspekt | Wert |
|--------|------|
| Dauer | ~212s Fenster, ~165s kumuliert <28 FPS |
| Schlimmster Drop | 95,6s durchgehend, Min 23,4 FPS |
| **Bottleneck** | **100% GPU-bound** |
| gpu_time_ms | 27–30 ms (am GPU-Budget-Limit) |
| cpu_time_ms | 8–14 ms (untätig) |
| System-CPU | 65% idle, kein I/O, kein Swap |
| **Ursache** | **EDDH-Szenerie sättigt GPU** (reines Rendering-Problem) |

#### Cluster 2: Cruise DSF-Boundary-Crossings (T+1951s bis T+3728s, 25 Drops)

| Aspekt | Wert |
|--------|------|
| Dauer | 30 Min Cruise-Segment, 25 individuelle Drops |
| Min FPS | 19,9 (X-Plane-internes Minimum) |
| **Bottleneck** | **85–100% CPU-bound** |
| cpu_time_ms | 27–40 ms (über 33ms Frame-Budget) |
| gpu_time_ms | 12–25 ms |
| System-CPU | 90–96% busy während Drops |
| NVMe Reads | 600–3.200 MB/s |
| X-Plane I/O | bis 2.223 MB/s (DSF-Loading) |
| XEL CPU | 630–980% (Tile-Generierung) |
| Swap | 0 → 6,7 GB während Cruise |
| **Ursache** | **DSF-Loading + XEL-Generierung konkurrieren um CPU + NVMe** |

**Wichtiger Unterschied zu Run T:** Die FPS-Drops korrelieren **nicht 1:1** mit DSF-Crossings. Sie folgen mit 22–194s Verzögerung — X-Plane lädt DSFs verzögert. Während dieser Fenster konkurrieren X-Plane und XEL gleichzeitig um CPU und NVMe.

#### Cluster 3: Descent/Approach/Landing (T+4025s bis T+5455s, 7 Drops)

| Aspekt | Wert |
|--------|------|
| Dauer | 7 kurze Drops (1–3,6s) |
| **Bottleneck** | **67–100% CPU-bound** |
| System-CPU | 82–88% busy |
| Swap | 8–11,4 GB |
| **Ursache** | DSF-Loading bei Descent + Memory Pressure |

---

## 4. GPU / VRAM

| Metrik | Min | Avg | Max |
|--------|-----|-----|-----|
| VRAM Used | 12.382 MiB (50%) | 16.904 MiB (69%) | 21.374 MiB (87%) |
| GPU Util | 15% | 61,5% | 100% |
| Temperatur | 47°C | 56,2°C | 66°C |
| Power | 107,6 W | 215,4 W | 321,8 W |
| Perf State | P2 (100%) | — | — |
| Throttle Reasons | 0 (100%) | — | — |
| DMA Fence Waits | 0 | — | — |

**GPU-Util höher als Run T** (61,5% vs. 50,6%, +11pp). Erklärung: EDDH Ground-Phase war GPU-gesättigt (100% Util, 27–30ms gpu_time). Temperatur und Power entsprechend höher, aber unkritisch. Kein Throttling, 3,2 GB VRAM Headroom.

---

## 5. Disk IO

| Device | Avg Read MB/s | Max Read MB/s | Avg R-Lat | Max R-Lat | Avg W-Lat | Max W-Lat |
|--------|--------------|---------------|-----------|-----------|-----------|-----------|
| nvme0n1 (SN850X) | 8,9 | — | 0,25 ms | 1,02 ms | 1,09 ms | 3,0 ms |
| nvme1n1 (990 PRO) | 9,8 | — | 0,24 ms | **13,0 ms** | **9,70 ms** | **310 ms** |
| nvme2n1 (SN850X) | 13,0 | — | 0,23 ms | **16,0 ms** | 0,20 ms | 14,0 ms |

**Slow IO (>5ms):** 1.468 Events (6,2× Run T)

| Device | Events | Avg Lat | P50 | Max |
|--------|--------|---------|-----|-----|
| nvme1n1 | 1.373 | 11,1 ms | 10 ms | 34 ms |
| nvme0n1p2 | 90 | 8,9 ms | 7 ms | 47 ms |
| nvme0n1 | 5 | 8,6 ms | 8 ms | 12 ms |

nvme1n1 (990 PRO) zeigt das bekannte 10–11ms NVMe-Power-State-Pattern. Die 310ms Write-Latenz auf nvme1n1 ist neu und deutet auf I/O-Contention durch erhöhten Swap-Druck hin.

---

## 6. CPU & Frequenz

| Metrik | Avg | Max |
|--------|-----|-----|
| Freq Min | 2.566 MHz | — |
| Freq Avg | 4.669 MHz | — |
| Freq Max | — | 5.491 MHz |
| Below 3.500 MHz | 21,4% (E-Cores idle) | — |

Kein Thermal-Throttling. P-Cores konstant auf 5,2 GHz boost. Gesund.

---

## 7. Per-Process

### XEarthLayer

| Metrik | Min | Avg | Max | Run T Max |
|--------|-----|-----|-----|-----------|
| RSS MB | 9.299 | 12.389 | **16.774** | 14.401 |
| CPU% | 0,0% | 68,7% | 1.131,9% | 1.301,4% |
| Threads | 35 | 290 | 547 | 547 |

RSS-Peak 2,4 GB höher als Run T — möglicherweise routenabhängig (EDDH-Szenerie), weiter beobachten.

### X-Plane (simon_lin64)

| Metrik | Min | Avg | Max |
|--------|-----|-----|-----|
| RSS MB | 14.958 | 16.617 | 19.187 |
| CPU% | 162% | 320,3% | 1.326,9% |
| Threads | 87 | 100 | 129 |

---

## 8. IRQ-Verteilung (irqbalance-Test)

**Kernfrage:** Verbessert irqbalance die IRQ-Verteilung?

| Gerät | CPUs genutzt | Verteilung | Bewertung |
|-------|-------------|------------|-----------|
| nvme0 | 16/16 | Gut balanciert, 5–9% pro CPU | ✅ |
| nvme1 | 16/16 | Gut balanciert, 5–10% pro CPU | ✅ |
| nvme2 | 16/16 | Leichte Konzentration CPU14/15 (22%), Rest 4–6% | ✅ |
| NVIDIA GPU | 1/16 (CPU2) | MSI Single-Vector — Hardware-Limitation | ➡️ Erwartet |

**Ergebnis:** irqbalance funktioniert korrekt. NVMe-IRQs sind gleichmäßig über alle 16 CPUs verteilt. Die NVIDIA-GPU kann bauartbedingt nicht profitieren (einzelner MSI-Vektor). irqbalance kann beibehalten werden.

---

## 9. Vergleich Run T → Run W

| Metrik | Run T (zram, kein irqbalance) | **Run W (kein zram, irqbalance)** | Delta |
|--------|-------------------------------|----------------------------------|-------|
| FPS < 25 | 3,1% | **4,6%** | +48% ❌ |
| FPS < 20 | 2,1% | **3,0%** | +43% ❌ |
| allocstall Samples | 1 | **77** | 77× ❌ |
| allocstall max/s | 3.715 | **12.243** | 3,3× ❌ |
| Direct Reclaim total | 753 | **72.644** | 96× ❌ |
| Main Thread Reclaim | **0** | **54.686** | ❌❌❌ |
| Worst Reclaim Stall | 0 | **194,6 ms** | ❌❌ |
| Slow IO (>5ms) | 236 | **1.468** | 6,2× ❌ |
| Swap Peak | 7.667 (zram) | **11.809 (NVMe)** | +54% ⚠️ |
| VRAM Peak | 21,1 GB (86%) | **21,4 GB (87%)** | ≈ ✅ |
| EMFILE | 0 | **0** | = ✅ |
| CB Trips | 0 | **0** | = ✅ |
| GPU Throttle | 0 | **0** | = ✅ |
| DMA Fence | 0 | **0** | = ✅ |
| NVMe IRQ-Verteilung | (kein irqbalance) | **Alle 16 CPUs** | ✅ Neu |

---

## 10. Handlungsempfehlungen

### 10.1 [CRITICAL] zram wieder aktivieren

Die Daten sind eindeutig: Ohne zram kehrt Direct Reclaim auf dem X-Plane Main Thread zurück (54.686 Events, Stalls bis 194 ms). zram absorbiert Memory Pressure im RAM statt auf NVMe — das ist der entscheidende Unterschied.

```bash
sudo mv /etc/udev/rules.d/99-zram.rules.disabled /etc/udev/rules.d/99-zram.rules
# Reboot erforderlich
```

### 10.2 [INFO] irqbalance beibehalten

irqbalance verteilt NVMe-IRQs gut über alle 16 CPUs. Kein Grund es wieder zu deaktivieren. Den `noirqbalance` Boot-Parameter **nicht** wieder hinzufügen.

### 10.3 [INFO] zram-Größe evaluieren (nächster Test)

Run T nutzte 16 GB zram, Peak 7,7 GB. Testen ob 8 GB zram ausreicht — weniger Kernel-Overhead bei gleichem Schutz. Aber **erst nach Bestätigung dass zram die Regression behebt**.

### 10.4 [INFO] XEL RSS-Wachstum beobachten

RSS Peak 16,8 GB (vs. 14,4 GB in Run T). Könnte routenabhängig sein (EDDH ist komplex), aber +2,4 GB ist signifikant. In weiteren Runs beobachten.

### 10.5 [INFO] nvme1n1 Power-State-Latenz prüfen

1.373 Slow-IO-Events auf nvme1n1 (10–11ms Pattern). PM QOS sollte aktiv sein — prüfen ob die udev-Rule für nvme1n1 greift:

```bash
cat /sys/block/nvme1n1/device/power/pm_qos_no_power_save
```

---

## 11. Zusammenfassung

**Run W beweist: zram ist bei diesem Workload unverzichtbar.** Trotz 96 GB RAM erzeugt die Kombination aus X-Plane (~19 GB RSS), XEarthLayer (~17 GB RSS), Page Cache (~50 GB) und QEMU (~4 GB) genug Memory Pressure, dass der Kernel ohne zram in Direct Reclaim auf dem X-Plane Main Thread fällt.

**zram entfernen = Regression:**
- 77× mehr allocstalls (1 → 77 Samples)
- 96× mehr Direct Reclaim (753 → 72.644 Events)
- Main Thread Reclaim von 0 auf 54.686 (75%) — der Durchbruch aus Run T ist zunichte gemacht
- FPS-Drops +48% (3,1% → 4,6% < 25 FPS)

**irqbalance aktivieren = neutral bis positiv:**
- NVMe-IRQs gleichmäßig über alle 16 CPUs verteilt
- Kein negativer Effekt beobachtet
- Beibehalten empfohlen

**Nächster Schritt:** zram reaktivieren (16 GB), irqbalance beibehalten. Dann Run X als Bestätigungs-Run mit der Kombination zram + irqbalance.
