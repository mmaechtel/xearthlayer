# Run AA — Ergebnisse: 83-Minuten England→EDDN (Stansted→Nuernberg)

**Datum:** 2026-03-16
**System:** Ryzen 9 9800X3D 8C/16T, 94 GB RAM, RTX 4090 24 GB, 3× NVMe (2× SN850X 8TB + 990 PRO 4TB)
**Kernel:** Liquorix 6.19.6-1 (PDS), Btrfs RAID0 (xplane_data) + RAID1 (home)
**Workload:** X-Plane 12 (ToLiss A320?), XEarthLayer, QEMU/KVM
**Aenderungen seit Run Z:** Keine Tuning-Aenderungen. Andere Route (Europa statt Australien). Zweiter Flug in der Session (Run Z lief vorher).

---

## 0. Testbedingungen

| Parameter | Wert |
|-----------|------|
| Dauer | 83 Min (13:30–14:53 UTC) |
| sysmon Samples | 4.846 (vmstat), 24.184 (mem), 22.850 (telemetry) |
| Sidecar | Ja (3 bpftrace Probes) |
| Tuning-Stack | Run-T-Stack + irqbalance (unveraendert) |
| Besonderheit | **Swap bei Session-Start bereits 7.909 MB!** System nicht frisch nach Run Z |

## 1. Erwartungen vs. Ergebnisse

| Metrik | Run Z | **Run AA** | Delta | Bewertung |
|--------|-------|-----------|-------|-----------|
| Main Thread Reclaim | 326 (1s Burst) | **46.723** (Session-weit) | 143× ❌❌❌ | Katastrophal |
| allocstall Samples | 5 | **19** (Peak 7.907) | 4× ❌ | |
| Direct Reclaim gesamt | ~600 | **53.284** | 89× ❌❌❌ | |
| Main Thread Reclaim-Zeit | ~0,1s | **17,4 Sekunden** | ❌❌❌ | |
| Max Reclaim-Latenz | 9,9 ms | **85,5 ms** | 9× ❌ | |
| FPS < 25 | 4,09% | **3,30%** | ✅ Besser | |
| FPS < 20 | 1,84% | **1,94%** | ≈ gleich | |
| Slow IO (>5ms) | 1.743 | **413** | ✅ 76% besser | |
| Swap Start | ~0 MB | **7.909 MB** | ⚠️ Nicht frisch | |
| Swap Peak | 3.724 MB | **18.236 MB** | 5× ❌ | |
| EMFILE / CB Trips | 0/0 | **0/0** | ✅ | |
| DDS generiert | 61.076 | **~1.470** (nur 26 Min XEL-Log) | ℹ️ | |

---

## 2. Kernbefunde

### 2.1 Drei-Phasen-Verhalten

| Phase | Zeitraum | Indikator |
|-------|----------|-----------|
| **Warm-up** | Min 0–15 | Swap stabil bei ~7,9 GB (Altlast!), kein kswapd bis Min 8, available ~42 GB |
| **Ramp-up** | Min 15–50 | 15 von 19 allocstalls, Peak File Refaults 1,6 Mio/s bei Min 30, Swap waechst auf 16,7 GB |
| **Steady-state** | Min 50–83 | Letzte allocstalls bei Min 60, Swap stabilisiert ~17–18 GB, kswapd fadet aus |

### 2.2 Memory Pressure — SCHWER

| Metrik | Warm-up (0–15) | Ramp-up (15–50) | Steady (50–83) |
|--------|----------------|-----------------|-----------------|
| available_mb avg | 42.176 | 38.543 | 33.306 |
| available_mb min | 41.643 | 31.060 | **30.616** |
| cached_mb avg | 43.901 | 40.637 | 35.363 |
| swap_used_mb avg | 7.896 | 10.770 | **17.167** |

**Kritisch:** Available Memory startete bei nur 42 GB (Run Z: 80 GB!) und Swap bei 7,9 GB. Das System war durch Run Z vorbelastet. Der Working Set ueberstieg den verfuegbaren RAM, was zu permanentem Page-Recycling fuehrte (81% aller vmstat-Samples zeigen Swap-In-Aktivitaet).

**Swap-Wachstum:** +9,4 GB ueber die Session (7,9 → 18,2 GB Peak). Massiver Swap-Out-Burst bei Min 15 (315.208 Pages/s).

### 2.3 Direct Reclaim — KATASTROPHAL

**53.284 Direct Reclaim Events (Run Z: ~600, Faktor 89×)**

| Prozess | Events | Max Latenz | Gesamt-Zeit |
|---------|--------|-----------|-------------|
| **X-Plane Main Thread** | **46.723** | **85,5 ms** | **17,4 Sekunden** |
| cuda-EvtHandlr | 3.487 | 77,9 ms | 2,0 s |
| tokio-rt-worker (XEL) | 530 | 6,8 ms | 0,15 s |
| bpftrace | 419 | 5,9 ms | 0,25 s |
| libobs: graphic | 210 | 5,1 ms | 0,12 s |

**Reclaim-Stuerme nach Minute:**

| Minute | Main Thread Events | Max Latenz | Gesamt-Zeit |
|--------|-------------------|-----------|-------------|
| 8 | 175 | 3,4 ms | 71 ms |
| **15** | **6.909** | 8,3 ms | **4.480 ms** |
| 29 | 208 | 2,1 ms | 103 ms |
| **30** | **19.600** | 6,4 ms | **6.633 ms** |
| 35 | 220 | 3,9 ms | 113 ms |
| 40 | 351 | 1,5 ms | 182 ms |
| **46** | **8.702** | 26,0 ms | **3.505 ms** |
| **59** | **6.063** | 20,5 ms | 746 ms |
| **60** | **4.495** | **85,5 ms** | **1.546 ms** |

### 2.4 XEarthLayer Streaming Activity

| Metrik | Wert |
|--------|------|
| DDS generiert (26 Min XEL-Log) | 1.470 |
| Circuit Breaker Trips | 0 |
| EMFILE Errors | 0 |
| XEL RSS Peak | 13.844 MB |
| XEL Threads | 38–549 (burst-artig) |

**XEL-Log-Luecke:** Log endet bei 13:56 UTC (Min 26). Die beiden Problemfenster (Min 34–44, Min 59–69) sind **nicht durch XEL-Logs abgedeckt**. Ursache unklar — moeglicherweise Log-Rotation oder XEL wurde neu gestartet.

### 2.5 Alloc-Stall-Cluster

| Minute | allocstall_s | Kontext |
|--------|-------------|---------|
| 8,9 | 215 | Frueh, isoliert |
| **15,2** | **7.907 + 150** | Erster Major Burst, Swap-Out 315K/s |
| **30,0–30,8** | **237–7.091** (5 Rows) | Schwerster Cluster, File Refaults 1,6 Mio/s |
| 35,4 | 236 | **Fenster 1** |
| 40,2 | 106 + 235 | **Fenster 1**, + Compaction Stalls |
| **46,8–46,9** | **2.540–6.781** | Spaeter Ramp-up Burst |
| **59,9–60,4** | **2.302–3.395** (4 Rows) | **Fenster 2**, Peak kswapd 1,9 Mio/s |

---

## 3. In-Sim Telemetrie (FPS / CPU Time / GPU Time)

| Metrik | Wert |
|--------|------|
| FPS avg | 30,45 |
| FPS median | 29,9 |
| FPS P5 | 28,1 |
| FPS P1 | 19,9 |
| FPS min | 19,9 (Sim-Floor) |
| FPS < 25 | **3,30%** (754 Samples) |
| FPS < 20 | **1,94%** (443 Samples) |
| Pausiert | Nie |

### CPU vs GPU Bottleneck

| Bedingung | CPU avg (ms) | GPU avg (ms) | Bottleneck |
|-----------|-------------|-------------|------------|
| Gesamt | 17,2 | 16,3 | Balanced |
| Bei FPS < 25 | **32,0** | 15,9 | **CPU-bound** |

**FPS-Drops sind CPU-bound.** Bei Drops verdoppelt sich die CPU-Time waehrend GPU-Time konstant bleibt. Ursache: Direct Reclaim blockiert den Main Thread.

### Markierte Zeitfenster

**Fenster 1 (Min 34–44):**
- FPS < 25: **4,8%**, FPS < 20: **2,9%**
- CPU-Time Spikes bis 38 ms bei Min 35 (GPU: 11–14 ms)
- **95% CPU-bound**
- Korrelation: 593 Direct Reclaim Events auf Main Thread + massive DSF-Loads (+51+007: 40,3s!)

**Fenster 2 (Min 59–69):**
- FPS < 25: **3,2%**, FPS < 20: **2,1%**
- CPU-Time Spikes bis 36 ms bei Min 60 (GPU: 14–15 ms)
- Korrelation: 10.558 Direct Reclaim Events auf Main Thread (max **85,5 ms**) + DSF-Burst + nvme0n1 Write-Storm (840 MB/s, Latenz bis 345 ms)

### Flugprofil

| Parameter | Wert |
|-----------|------|
| Start | 51,89°N, 0,26°E (London Stansted Area) |
| Ende | 49,50°N, 11,08°E (Nuernberg Area) |
| Max Hoehe | 12.186 m (FL400) |
| Max GS | 549 kts |

---

## 4. GPU / VRAM

| Metrik | Min | Avg | Max |
|--------|-----|-----|-----|
| VRAM Used | 14.272 MiB | 18.283 MiB | **21.323 MiB (86,8%)** |
| GPU Util | 4% | 63,8% | 99% |
| Temperatur | 51 C | 56,2 C | 64 C |
| Power | 144 W | 217 W | 308 W |
| Perf State | P2 (100%) | | |
| Throttling | **Kein** | | |

**GPU-Starvation in Fenster 1:** GPU Util sinkt auf avg 55,2% (Session-avg 63,8%). VRAM bei Session-Peak 21,3 GB. GPU wird nicht mit Texturen versorgt, weil IO durch Direct Reclaim blockiert ist.

### DMA Fence Waits

- 2.151 Events, max 29 ms, alle auf Kernel-Worker-Threads
- Kein Anwendungs-Impact

---

## 5. Disk IO

### Per-Device Throughput

| Device | Read avg/max | Write avg/max | Write-Lat avg/max |
|--------|-------------|---------------|-------------------|
| nvme0n1 | 18,9 / 3.415 MB/s | 0,67 / **840** MB/s | 4,4 / **345,6 ms** |
| nvme1n1 | 19,1 / 3.405 MB/s | 0,30 / 93 MB/s | 0,45 / 41,3 ms |
| nvme2n1 | 17,9 / 3.395 MB/s | 0,01 / 26 MB/s | 2,1 / 37,6 ms |

### nvme0n1 Write-Storms

| Zeitraum | Write MB/s | Write-Latenz | Ursache |
|----------|-----------|-------------|---------|
| Min 49,8–50,2 | bis 794 | bis 191 ms | Swap/Writeback Flush |
| **Min 60,4–62,9** | bis **840** | bis **345,6 ms** | **Schwerster IO-Event der Session** |

### Slow IO (bpftrace)

**413 Events (Run Z: 1.743 — 76% Reduktion)**

Groesste Cluster: Min 30 (100 Events), Min 59 (170 Events), Min 60 (47 Events).

---

## 6. CPU & Frequenz

**Gesamt:** user=25,6%, sys=3,4%, iowait=1,4%, idle=67,1%

**iowait-Spikes >10%:** Massiv bei Min 21 (100% iowait auf mehreren Cores), Min 35–36 (Fenster 1), Min 62 (Fenster 2).

**Frequenz:** avg 4.486 MHz, max 5.576 MHz. Kein Throttling.

---

## 7. Per-Process

| Prozess | CPU% avg | RSS avg | RSS peak | Bemerkung |
|---------|----------|---------|----------|-----------|
| X-Plane | 351,7% | 20.103 MB | **24.860 MB** | RSS 7 GB hoeher als Run Z! |
| XEarthLayer | 39,1% | 10.583 MB | 13.844 MB | Burst-artig |
| QEMU | 14,6% | 4.100 MB | 4.299 MB | Hintergrund-VM |

**Total System RSS Peak:** ~42,8 GB (X-Plane 24,8 + XEL 13,8 + QEMU 4,1)

**In Fenster 1 (Min 34–44):** X-Plane CPU bei 501% (vs 352% avg) — intensives Texture-Loading. XEL bei 58% mit IO-Bursts.

---

## 8. Vergleich Run Z → Run AA

| Metrik | Run Z | **Run AA** | Ursache |
|--------|-------|-----------|---------|
| Main Thread Reclaim | 326 | **46.723** | Vorbelastetes System, europaeische Route |
| allocstall Peak | 350,8 | **7.907** | — |
| Reclaim-Zeit Main Thread | ~0,1s | **17,4s** | — |
| Max Reclaim-Latenz | 9,9 ms | **85,5 ms** | — |
| Swap Start | ~0 MB | **7.909 MB** | Run Z nicht bereinigt |
| Swap Peak | 3.724 MB | **18.236 MB** | — |
| X-Plane RSS Peak | 19.477 MB | **24.860 MB** | Europaeische Scenery schwerer |
| Available Start | ~80 GB | **~42 GB** | Vorbelastet |
| FPS < 25 | 4,09% | **3,30%** | Kuerzere Session |
| Slow IO | 1.743 | **413** | Verbesserung |
| VRAM Peak | 93,9% | **86,8%** | Weniger VRAM-Druck |

### Ursachenanalyse

Die Regression hat **zwei Hauptursachen**:

**1. System nicht frisch gestartet:** Run AA startete mit 7,9 GB Swap und nur 42 GB available (Run Z startete mit ~0 GB Swap und 80 GB available). Die Caches und Swap-Bereiche von Run Z waren noch belegt.

**2. Europaeische Route ist schwerer:** X-Plane RSS stieg auf 24,8 GB (Run Z: 19,5 GB). Die europaeische Scenery (UK, Niederlande, Rhein-Ruhr, Mitteldeutschland) hat dichtere DSF-Tiles — einzelne Loads bis 40,3 Sekunden (+51+007.dsf). 434 DSF-Events vs. weniger in Run Z.

---

## 9. X-Plane Events — Markierte Fenster

### Fenster 1 (Min 34–44): 97 Events

- **54 DSF-Loads** — schwere Tiles: +51+007 (40,3s!), +50+007 (15,8s), +52+007 (6,4s)
- **13 Airport-Loads** — EDDL, EDLF, EDLK, EDKN, EDKZ (Rhein-Ruhr Ballungsraum)
- **28 Errors** — 20× EDDK-Scenery-Fehler, 6× Frankfurt-Airport-Texturen (kosmetisch)

### Fenster 2 (Min 59–69): 78 Events

- **69 DSF-Loads** — +48+008 bis +48+012, Einzelloads bis 12s (Mitteldeutschland)
- **7 Airport-Loads** — EDQH, EDQI, EDQX (Nuernberger Region)

---

## 10. Handlungsempfehlungen

### 10.1 System vor Messflug frisch starten (Prioritaet: HOCH)

**Problem:** Run AA startete mit 7,9 GB Swap-Altlast von Run Z. Das System hatte 38 GB weniger available Memory als bei einem frischen Start.

**Aktion:** Vor jedem Messflug:
```bash
sudo swapoff -a && sudo swapon -a   # Swap leeren
sudo sysctl vm.drop_caches=3         # Page Cache leeren
```
Oder besser: System komplett neu starten.

### 10.2 Europaeische Route separat als Baseline etablieren (Prioritaet: MITTEL)

**Problem:** Die europaeische Scenery ist signifikant schwerer als Australien oder Costa Rica. X-Plane RSS 24,8 GB vs 19,5 GB (Run Z). DSF-Loads bis 40s. Ein direkter Vergleich mit Run T/Y/Z ist nur bedingt aussagekraeftig.

**Aktion:** Naechster Run auf gleicher Route (UK→EDDN oder EDDH→EDDM) mit frischem System. Das wird die echte europaeische Baseline.

### 10.3 XEL-Log-Luecke untersuchen (Prioritaet: NIEDRIG)

**Problem:** XEL-Log endet bei Min 26 (13:56 UTC). Die Problemfenster haben keine XEL-Daten.

**Aktion:** Vor naechstem Run pruefen ob Log-Rotation aktiv ist. `ls -la ~/.xearthlayer/xearthlayer.log*`

---

## 11. Zusammenfassung

Run AA ist **nicht als Vergleich mit Run Z geeignet** — das System war durch den vorherigen Run Z vorbelastet (7,9 GB Swap, 42 GB statt 80 GB available). Die 46.723 Main Thread Reclaim Events und 17,4 Sekunden Reclaim-Zeit sind eine direkte Folge davon.

**Positiv:**
- Slow IO weiter verbessert (413 Events, 76% besser als Run Z)
- FPS < 25 bei 3,30% (unter dem 3,5% Ziel!)
- Zero EMFILE, Zero CB Trips
- Kein GPU-Throttling

**Negativ:**
- Massiver Direct Reclaim durch Swap-Altlast
- nvme0n1 Write-Storm bei Min 60–63 (840 MB/s, 345 ms Latenz)
- X-Plane RSS 24,8 GB — europaeische Scenery braucht mehr RAM

**Fazit:** Run AA zeigt, dass ein frischer Systemzustand essentiell ist. Der Tuning-Stack selbst ist nicht das Problem — das System war schlicht vorbelastet. Ein Wiederholungsrun auf gleicher Route mit frischem System ist noetig.
