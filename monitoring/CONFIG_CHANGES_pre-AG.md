# XEarthLayer Config-Aenderungen — vor Run AG

**Datum:** 2026-04-27
**Anlass:** Run AF-3 zeigte 2 unrecoverable X-Plane-Hangs durch IO-Queue-Saturation. Code-Analyse identifizierte konkrete Settings die IO-Druck multiplizieren ohne Backpressure.
**Backup:** `~/.xearthlayer/config.ini.bak.2026-04-27_pre-AG`
**Quell-Analyse:** `monitoring/ANALYSE_RUN_AF-3_2026-04-27.md`

## Fuenf Aenderungen

### 1. `executor.cpu_concurrent`: 8 → 16

**Grund:** Default fuer 16-Core-System ist `max(num_cpus * 1.25, num_cpus + 2) = 20`. Der vorige Wert 8 war Default/2.5 — dadurch teilten sich Encode-Pipeline, Chunk-Assembly und Prefetch denselben schmalen Pool. Bei IO-Saturation kamen Tile-Requests nicht durch, weil die CPU-Stage einen Permit braucht.

**Effekt:** Pipeline kann fertige Downloads schneller durch Encode → Cache-Write schieben. Queue-Tiefe sinkt. **Nicht** weniger CPU-Last (System ist nicht CPU-bound), sondern weniger interne Wait-Times.

### 2. `prefetch.cycle_interval_ms`: 2000 → 4000

**Grund:** Prefetch-Coordinator plante alle 2 s eine neue Tile-Liste mit ~54 Tiles. Diese 54 Tiles wurden **gleichzeitig** in die Job-Queue submitted = 54-Tile-Burst alle 2 s. Bei IO-Saturation stauten sich die in den Resource-Pool-Semaphoren.

**Effekt:** Halbiert Burst-Frequenz. Sustained-Load 27 → 13.5 Tiles/s. Disk-IO-Permits frei fuer On-Demand-FUSE-Reads von X-Plane.

**Tradeoff:** Prefetch reagiert 2 s spaeter auf Heading-Aenderungen. Im Cruise (250 kt) sind 2 s = 0.14° Bewegung — vernachlaessigbar.

### 3. `prefetch.box_extent`: 9 → 6.5

**Grund:** 9° Box-Kantenlaenge → 81 DSF-Tiles maximal, mit Bias 0.8 effektiv 54 unique Tiles pro Cycle. 6.5° → 42 DSF-Tiles → 28 unique pro Cycle (−48 % Plan-Volume).

**Begruendung 6.5°:**
- X-Plane Scenery-Window ist ~3° → 6.5° gibt 1.5° Sicherheits-Buffer ringsum
- Bei Cruise-Geschwindigkeiten 200–450 kt bewegt sich das Flugzeug pro Cycle (4 s) max 0.5° → 6.5° = 13× Vorlauf, ueberreichend
- Default in XEL `defaults.rs` ist 7.0 → 6.5 ist nahe Default, leicht konservativer

**Effekt:** Halbiert die geplanten Tiles pro Cycle. Kombiniert mit Aenderung 2: Tile-Submission-Druck **−75 %** gegen Run AF-3.

### 4. `fuse.max_background`: 256 → 128

**Grund:** Der wichtigste Teil. 256 erlaubt dem Kernel 256 pending Background-FUSE-Requests bevor X-Plane gethrottelt wird. **Aber** die Storage-Pipeline kann nicht 256 parallele Operationen bedienen — saturated BTRFS RAID0 verarbeitet vielleicht 50–80 simultan. Mit 256+ pending Requests **plus** 128 `disk_io_concurrent` ergeben sich theoretisch 384 in-flight Ops → die Queue bleibt voll, X-Plane wartet seitenweise.

**Bei 128:** Kernel queued maximal 128. Sobald die Pipeline diesen Wert erreicht, **bremst der Kernel X-Plane proaktiv** statt Frames stillen zu lassen waehrend nichts vorwaerts geht.

**Effekt:** Cascade-Failure-Hangs verhindert. X-Plane fuehlt das Backpressure und reagiert (kurze Wartezeit, dann weiter) statt komplett zu freezen.

### 5. `fuse.congestion_threshold`: 192 → 96

**Grund:** Konvention: 75 % von `max_background`. Wenn #4 auf 128 sinkt, muss congestion_threshold mit auf 96 (= 128 × 0.75). Sonst greift Backpressure nie, weil die Pending-Queue die 192-Schwelle nicht erreichen kann.

**Effekt:** Backpressure setzt frueher ein, glatter Uebergang von normalem in throttled-Modus.

## Was bewusst NICHT geaendert wurde

| Setting | Wert (unveraendert) | Begruendung Nichtaenderung |
|---|---|---|
| `executor.network_concurrent` | 128 | Downloads sind nicht der Bottleneck. Externe Provider (Bing/Google) limitieren. |
| `executor.disk_io_concurrent` | 128 | Reduktion kuenstlich. Echter Fix waere O_DIRECT in disk.rs (Code, nicht Config). |
| `executor.max_concurrent_jobs` | 8 | Job-Limit ist sinnvoll bei 16 Cores. Mehr Jobs = mehr parallele Pipelines = mehr IO-Druck. |
| `cache.memory_size` | 4 GB | Auf 96 GB RAM irrelevant. Vorige Empfehlung "auf 512 MB senken" war fehlgeleitet. |
| `cache.dds_disk_ratio` | 0.6 | 60/40 DDS/Chunks ist optimal — DDS sind das Endprodukt, Chunks ephemer. |
| `prewarm.grid_rows/cols` | 3/4 | Einmaliges 12-Tile-Setup beim Start, kein Dauer-Druck. |
| Sysctl-Parameter | unveraendert | Bereits exakt auf Author-Baseline (xplane-on-linux/docs/sysctl-xearthlayer.md). |

## Erwartete Wirkung

| Metrik | Run AF-3 Wert | Erwartung Run AG |
|---|---|---|
| Run-Durchschnitt iowait | 42 % | < 25 % |
| Slow-IO-Events Tail (>1000ms) | 37.4 % | < 15 % |
| pgmajfault peak | 8997/s | < 3000/s |
| Burst-Volume pro Cycle | 54 Tiles | ~28 Tiles |
| Hangs | 2 unrecoverable | 0 |
| FPS-Drop < 25 (Anteil) | 9.4 % | < 5 % |

## Restrisiken / Was der Fix nicht abdeckt

1. **BTRFS-Auslastung 95 % Data + 98 % Metadata** ist kein Config-Problem, sondern Filesystem-State. User hat 1.2 TB freigegeben + Balance gemacht (326 Chunks reloziert) — neuer State noch nicht voll erfasst. Vor Run AG den finalen Zustand pruefen.

2. **Cache-Write-Strategie ohne O_DIRECT** (`disk.rs:296`). Page-Cache-Druck durch `tokio::fs::write()` bleibt — das ist der grosse strukturelle Fix der noch fehlt. Implementierungs-Aufwand: mittel (1–2 Tage). **Wenn Run AG mit Config-Aenderungen die Hangs nicht eliminiert**, ist O_DIRECT der naechste Schritt.

3. **Swap auf nvme0n1p6** ist immer noch auf demselben physischen Device wie Teile des xplane_data RAID0. Bei voller Pipeline-Auslastung bleibt IO-Konkurrenz an dieser Stelle. Mitigation: `swapoff -a` (radikal, aber bei 96 GB RAM machbar).

## Rollback

Falls Run AG-Daten zeigen dass die Aenderungen schaedlich sind:

```bash
cp ~/.xearthlayer/config.ini.bak.2026-04-27_pre-AG ~/.xearthlayer/config.ini
```

Restart xearthlayer.
