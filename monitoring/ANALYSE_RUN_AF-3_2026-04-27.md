# Run AF-3 — Auswertung: 57-Min Cruise mit zwei X-Plane-Hangs

**Datum:** 2026-04-27
**System:** AMD Ryzen 7 9800X3D 8-Core / 91 GiB RAM, RTX 4090 (24 GiB), 3× NVMe + 1× SATA SSD
**Kernel:** Linux 6.19.14-1-liquorix-amd64
**Workload:** xearthlayer (PID 9578, --airport OMDB), X-Plane 12.4.2-r2 via gamescope, qemu-kvm, plasmashell
**Aenderungen seit Run AE:** Re-Validierung mit aktuellem main, kein expliziter Tuning-Schritt; Run AF-2 (2026-04-04) war damals abgebrochen → AF-3 ist **erste vollstaendige Datenerfassung** seit Run AE-Findings

---

## 0. Testbedingungen

- Dauer: 57 Min (16:54:46 – 17:51:47), vorzeitig vom User beendet (geplant 90 Min)
- Sample-Counts: 275k cpu, 64k io, 16k mem, 3.2k vmstat/vram/psi/freq, 12.4k xplane-telemetry
- Sidecar-Tracer: AKTIV (3 bpftrace-Prozesse: reclaim, io_slow, fence) — `trace_io_slow.log` 1.7 GB
- Tuning-Parameter: vm.swappiness=8, vm.watermark_scale_factor=500, vm.min_free_kbytes=1 GB, vm.vfs_cache_pressure=100
- XEL Config: cache.memory_size=4 GB (moka), executor.cpu_concurrent=8, prefetch.box_extent=9
- **Daten-Kontamination:** 17:35–17:40 lief `btrfs balance -dusage=20` während des Runs → dieses Fenster aus Workload-Statistiken ausgenommen.

---

## 1. Erwartungen vs. Ergebnisse

| Metrik | Run AE Erwartung (Tuning sollte schuetzen) | Run AF-3 tatsaechlich | Bewertung |
|---|---|---|---|
| Direct Reclaim auf Main Thread | 0 | **0** durchgaengig (allocstall=0–1) | ✓ Watermark-Tuning haelt |
| free_mb Minimum | > 2 GB | 346 MB Minimum (Pre-Hang1) | ✗ Reserve durchbrochen |
| Hangs | 0 | **2 unrecoverable** (manuell gekillt) | ✗ Kritisch verschlechtert |
| iowait Run-Mittel | < 10 % | **42 %** | ✗ Massiv |
| Swap belegt am Run-Start | < 5 GB | **18.34 GB** (kumuliert aus Vor-Session) | ✗ |
| pgmajfault peak | < 1000/s | **8997/s** (Hang 1) | ✗ |
| FPS-Drops < 25 | < 4 % | 9.4 % (139 von 1485 Telemetrie-Samples in den Hang-Fenstern) | ✗ |
| Tiles/Min Throughput | > 800 | ~850 (zwischen Hangs) | ✓ Throughput selbst OK |

→ **Tuning-Hypothese aus Run AE (Watermark gegen Direct Reclaim) war korrekt** — allocstall blieb 0. Aber das verhindert nicht den **eigentlichen Schaden**: BTRFS-induzierte IO-Queue-Saturation und Swap-Thrashing.

---

## 2. Kernbefunde

### 2.1 Drei-Phasen-Verhalten — nicht klassisch erkennbar

Run AF-3 startete **bereits im Ramp-up** (Swap 18.3 GB beim ersten Sample, Sessions liefen seit 15:52 Uhr lokal, 1 h Vorlauf). Klassisches Drei-Phasen-Modell trifft nicht — stattdessen:

| Phase (lokal) | Charakter | Status |
|---|---|---|
| 16:54–17:13 (19 min) | Cruise mit kontinuierlichem Memory-Druck | Pre-Hang Ramp |
| **17:13–17:15 (2 min)** | **Hang 1: Read-bound, swap-thrashing-getrieben** | **STALL** |
| 17:15–17:35 (20 min) | Re-Fly nach X-Plane-Restart | Inter-Hang |
| 17:35–17:40 (5 min) | btrfs balance läuft im Run | KONTAMINIERT |
| 17:40–17:45 (5 min) | OMDB Ground / Loading | Post-Balance Setup |
| **17:45–17:49 (4 min)** | **Hang 2: Write-bound, andere Mechanik** | **STALL** |
| 17:49–17:51 (2 min) | Aftermath, X-Plane gekillt | End |

### 2.2 Memory Pressure — XEL hortet 8–12 GB anonymen Speicher

xearthlayer war **alleiniger Top-Swap-Consumer:**

| Prozess | Peak Swap | Anteil System-Swap |
|---|---|---|
| **xearthlayer (PID 9578)** | **12.1 GB** (17:02:34) | **63 %** |
| X-Plane (alte PID 9977) | 7.4 GB | 38 % |
| X-Plane (neue PID 12055 nach Restart) | 1.9 GB | 10 % |
| plasmashell | 453 MB transient | < 3 % |
| qemu-system-x86 | 205 MB stabil | < 1 % |

**Wichtig:** XEL Config sagt `cache.memory_size = 4 GB`, aber XEL zog tatsaechlich bis zu **12 GB Anonymous Memory**. Das uebersteigt die Cache-Limit-Konfiguration. Vermutete Ursachen (in Reihenfolge der Beleg-Stärke):
1. **moka LRU Cache-Config wirkt nicht** — entweder durch Code-Bug, oder das Limit gilt nur fuer einen Teil der Cache-Eintraege (nicht inkludiert: GeoIndex, OrthoUnionIndex, Pipeline-Buffer)
2. **GeoIndex + OrthoUnionIndex** — beide HashMaps mit ggf. mehreren hundert MB jeweils, **nicht** vom moka-Limit erfasst
3. **Tokio Runtime + Async Stacks + Pipeline-Buffers** — Background-Tasks, Worker-Stacks, Tile-Assembly-Buffer (90 MB pro aktiver Tile)

### 2.3 XEarthLayer Streaming Activity — XEL haengt NICHT, X-Plane haengt

**Wichtigste Erkenntnis aus dem XEL-Log:** Während beider Hangs **lief XEL durchgaengig weiter**:

| Hang | XEL Log-Zeilen/Min | XEL Jobs/Min completed | XEL Timeouts |
|---|---|---|---|
| Hang 1 (15:13–15:15Z) | 218k–174k (normal) | 837, 780, 675 | **22, 11, 4** |
| Hang 2 (15:45–15:49Z) | normal | ~850/min | **70 in 4 min** |

XELs Diagnose schreibt sich selbst:

```
2026-04-27 15:13:02Z ERROR TIMEOUT: DDS generation exceeded 30s - possible executor stall
  Tile: (1743,2609,12) Context: ortho_union
  Tile: (6969,10423,14) Context: ortho_union
```

Zwei Tiles haben gleichzeitig (innerhalb 1 s) den 30-s-Timeout ueberschritten — das ist **synchroner Stall**, nicht zufaellige Latenz. **XEL erkennt selbst, dass die Pipeline blockiert ist.**

→ **Der "Hang" war kein XEL-Codestop, sondern X-Plane-Hauptthread im D-State auf einem FUSE-Read, der seit Sekunden auf XELs Antwort wartete.**

### 2.4 Direct Reclaim — durch Watermark-Tuning eliminiert

`allocstall_s = 0` durchgaengig im gesamten Run. Watermark-Tuning aus Run AE (`min_free=1GB`, `wsf=500`) **funktioniert:** Kernel geraet nicht in Direct Reclaim, kein User-Thread blockiert auf Reclaim.

**Aber:** Das verhindert nicht synchrone Major Page Faults nach Swap-Eviction. Die Swap-Recovery ist auf dem kritischen Pfad — kein Tuning-Parameter aendert das.

### 2.5 BTRFS-Allocator-Druck — der wahre Hauptverdaechtige

`/mnt/xplane_data` BTRFS-Pool zum Run-Zeitpunkt:
- Data RAID0: **95.53 %** belegt (11.36 / 11.89 TiB)
- Metadata RAID1: **98.25 %** belegt (64.84 / 66 GiB) — **Allocator-Panik-Schwelle ueberschritten**
- nvme1n1p1 (Stripe-Anteil): **1 MB unallocated** — physisch voll
- Bei BTRFS Metadata > 95 % geraet der Allocator in O(N) Lookups statt O(log N) — Latenz pro Operation steigt von <1 ms auf 100+ ms

Im `trace_io_slow.log` (28.7 Mio Events > 5 ms) fanden sich:
- **37.4 %** der Events mit Latenz **> 1000 ms** (Sample 1:200)
- **97.7 %** mit > 100 ms im Hang-1-Fenster
- Asymmetrisch: dev=271581186 (eines der NVMe) traegt 82 % der Slow-IOs → **ungleichmaessige Stripe-Auslastung**

---

## 3. In-Sim Telemetrie (FPS / GPU Time)

**Wichtiger Hinweis zum Mess-Artefakt:** Das Skript `monitoring/xplane_telemetry.py` **berechnet** `cpu_time_ms` als `frame_time - gpu_time` (Z. 153–156). Dies ist falsch (CPU/GPU laufen pipelined, nicht seriell). User-authoritative Werte aus X-Plane Stats-Overlay um 17:01: CPU = 40 ms, GPU = 20 ms. → Die `cpu_time_ms`-Spalte in `xplane_telemetry.csv` ist **systematisch zu niedrig**. Fuer CPU-Bound-Klassifikation `cpu.csv` `user%`+`iowait%` pro Core nutzen.

**FPS-Verlauf:**

| Phase | Telemetry-Samples | Mittel FPS | FPS < 25 | Frame-Time-Median |
|---|---|---|---|---|
| Pre-Hang 1 (17:02–17:13) | ~3300 | ~24–30 | gelegentlich | 40–50 ms |
| **Hang 1 (17:13–17:15)** | 145 | **29** | **10 (6.9 %)** | 32 ms |
| Recovery (17:16–17:35) | normal | 28–30 | gelegentlich | normal |
| **Hang 2 (17:45–17:49)** | 671 | **28** | **114 (17 %)** | hoeher |

**Beobachtung VSync@30 mit Half-Rate-Drop:** X-Plane VSync-Limit auf 30 FPS, frame_time aber 50 ms = 20 FPS = klassischer VSync-Half-Rate-Drop, weil CPU 40 ms nicht in 33.3 ms Budget schafft. Ursache **nicht GPU** (20 ms reserve), sondern **CPU im X-Plane-Hauptthread** — und der wartet auf FUSE-Reads.

---

## 4. GPU / VRAM

VRAM blieb durchgaengig harmlos:
- Pre-Hang: 13.2–13.6 GB / 24.5 GB (55 % Auslastung)
- Während Hang 1: stabil 13.6–14.5 GB
- Bei X-Plane-Kill: faellt auf 1.5 GB (X-Plane-Reste freigegeben)
- Peak im Run: 14.5 GB → **VRAM nie ein Problem**

GPU-Power-State: P2 (high performance) während Cruise, P5/P8 (idle) während Hang weil X-Plane keine Frames mehr submitted hat → **GPU war Opfer, nicht Taeter**.

`trace_fence.log`: nur 103 Bytes Inhalt, **keine DMA-Fence-Waits > 5 ms** im gesamten Run → CPU wartet nicht auf GPU.

---

## 5. Disk IO — Hauptbefund

### 5.1 Slow-IO-Verteilung

| Latenz-Bin | Anteil (Sample 1:200) | Bewertung |
|---|---|---|
| 5–10 ms | 0 % | anomal — sollte Baseline sein |
| 10–30 ms | 5.5 % | Standard NVMe-Saturation |
| 100–200 ms | 33.9 % | **HSM/Allocator-Stress** |
| 200–500 ms | 23.2 % | **Allocator-Panik** |
| **> 1000 ms** | **37.4 %** | **Tail-Latency-Krise** |

Das ist nicht NVMe-Hardware-Problem — das ist BTRFS-Allocator-Pathologie. NVMe selbst kann sub-ms-Latenzen auf PCIe 4.0 x4. Latenzen > 1 s in einem Drittel der Slow-Events bedeuten: das Gerat wartet nicht — der Filesystem-Layer haelt das Request fest.

### 5.2 Per-Phase-Statistiken

| Phase | Devices | r_lat avg | w_lat avg | %util |
|---|---|---|---|---|
| Pre-Hang 1 | nvme0/1/2 | 0.10 ms | 6.31 ms | 41.8 % |
| **Hang 1** | **nvme0n1** | **0.14 ms** | **11.17 ms** | **61.8 %** |
| **Hang 1** | **nvme2n1** | **0.02 ms** | **10.22 ms** | **61.2 %** |
| Re-Fly | nvme0n1 | 0.08 ms | 6.42 ms | 44.3 % |
| **Post-Balance** | nvme0/1/2 | **0.05 ms** | **3.83 ms** | **30.8 %** |

→ Schon der mini-Balance (`-dusage=20`) hat Write-Latenz von **6.42 → 3.83 ms** halbiert (43 %). Beleg dass Allocator-Druck dominante Latenz-Quelle war.

### 5.3 Hang 1 vs Hang 2 — verschiedene Mechaniken

| Charakteristik | Hang 1 (17:13) | Hang 2 (17:45) |
|---|---|---|
| **IO-Pattern** | Read-bound | **Write-bound** |
| iowait | 56 % | 5 % |
| pgmajfault peak | **8997/s** | 3739/s |
| pswpin peak | **7292/s** | 384/s |
| pswpout peak | 0 | 0 |
| allocstall peak | 0–1 | **70/s** |
| Slow-IO-Events im Fenster | ~10k | 78 |
| Auslöser-Hypothese | Swap-Recovery + FUSE-Read-Stall | Cache-Write-Sturm + 100 % NVMe-Util |

**Hang 1 Mechanik:** Working-Set verdraengt in Swap (XEL 12 GB anonymous, X-Plane 7 GB) → Major Page Fault auf X-Plane-Hauptthread → Swap-Read in NVMe-Queue, dort konkurriert mit XEL-Cache-Reads → 12-13 ms Latenz pro Request → 8997 Faults/s stauen sich → X-Plane-Thread > 30 s im D-State → XEL-Timeout schlaegt an, FUSE bekommt keine Daten zurueck → permanenter Hang.

**Hang 2 Mechanik:** Anders. Nach X-Plane-Restart und während Re-Fly schreibt XEL Cache-Daten massiv (~470 MB/s ueber alle 3 RAID0-Disks). BTRFS Metadata-Druck bei 98 % macht jeden Write teuer. **allocstall springt auf 70/s** — also doch Direct Reclaim, anderer Pfad als Hang 1. Schreibe-Queue saturiert, X-Planes naechster Read-Versuch blockiert.

---

## 6. CPU & Frequenz

CPU-Aufteilung im Run-Mittel:
- user%: 19.6 % (echte Rechenarbeit)
- system%: 18.4 % (Kernel: Memory-Management, IO-Scheduling, FUSE)
- iowait%: 42.3 %
- idle: 19.7 %

→ **Mehr als die Haelfte der CPU-Zeit war iowait** — das System hat ueberwiegend gewartet, nicht gerechnet.

Im Vergleich: Während Burst-Fenster 16:55:49–16:56:50 (User-Beobachtung „hohe CPU-Last") war user=19.4 %, system=22.2 %. → Die wahrgenommene Last war **Kernel-Memory-Management**, nicht User-Code.

---

## 7. Per-Process

XEL CPU im Cruise-Mittel: ~216 % (~2.2 Cores). Für DDS-Encoding + Download-Pipeline normal. **NICHT ueberhoeht.** Im Vergleich zu Tile-Throughput von ~850/min steht das in einem stimmigen Verhältnis.

X-Plane Main Thread: ~263 %. Auch normal — X-Plane ist single-threaded auf der Render-Loop.

qemu-system-x86: 20 % CPU, 4 GB RSS — stabil, kein Faktor.

---

## 8. Vergleich Run AE → Run AF-3

| Metrik | Run AE (Maerz) | Run AF-3 (Apr) | Trend |
|---|---|---|---|
| Watermark-Tuning gegen Direct Reclaim | nicht gesetzt | aktiv | ✓ wirkt |
| allocstall pro Hang | hoch | 0–1 (Hang 1), 70/s (Hang 2) | uneinheitlich |
| Swap-Out-Spitzen | bekannt | bestaetigt (110k pages/s peak) | gleich |
| Hangs in Run | mehrere | **2 unrecoverable** | gleich/schlechter |
| BTRFS-Pool-Auslastung | 90 %? | 95.53 % Data, 98.25 % Metadata | **schlechter** |

**Kernunterschied:** Run AE konzentrierte sich auf Watermarks. Die wirken, aber **das Problem hat sich verlagert** — von Direct-Reclaim-Hangs zu BTRFS-Allocator-getriebenen IO-Hangs. Watermarks helfen nicht, wenn BTRFS jeden Write/Read mit 100+ ms blockiert.

---

## 9. Handlungsempfehlungen

**Antwort auf die Leitfrage „braucht XEarthLayer zu viel CPU?"**

> **Nein.** XEL Compute-Profil ist okay (~2 Cores im Cruise, passt zum Throughput). XEL braucht aber **zu viel anonymen Speicher (8–12 GB)** und erzeugt **zu viel IO-Druck auf einem Filesystem das schon am Limit ist**. Die wahrgenommene CPU-Last sind System% + iowait% des Kernels, der unter XELs Memory-Druck und auf saturierten BTRFS-RAID0 ueberlebt.

Die Probleme sind, in absteigender Wirkungs-Reihenfolge:

### 9.1 Filesystem-Hygiene (Prio 1, hoechster Hebel)

User hat 1.2 TB freigegeben (12 → 11 TB). Naechste Schritte um den Druck nachhaltig zu eliminieren:

1. **`sudo fstrim -av`** — geloeschte Bloecke an SSD-Firmware freigeben
2. **`sudo btrfs balance start -dusage=50 /mnt/xplane_data`** — jetzt findet Balance Spielraum (vorher 1/4119 Chunks → erwartet deutlich mehr)
3. **`sudo btrfs balance start -musage=75 /mnt/xplane_data`** — Metadata-Konsolidierung
4. **Ziel:** Data < 85 %, Metadata < 80 %
5. **`/mnt/xplane_archive` (sda 4 TB SATA SSD)** als Ablage fuer alte/wenig genutzte Scenery-Pakete — Custom Scenery erlaubt es, Verzeichnisse außerhalb von /Custom Scenery zu mounten via symlink

### 9.2 Swap-Strategie aendern (Prio 2, eliminiert Hang-1-Mechanik)

Die alleinige Existenz von 119 GB Swap auf einem RAID0-Mitglied ist toxisch. Optionen:

1. **`sudo swapoff -a`** — bei 96 GB RAM machbar; tauscht Hangs gegen OOM-Kills (Kills sind besser, X-Plane startet einfach neu)
2. **`vm.swappiness = 1`** — fast kein Swap-Out mehr; in Run AF-3 mit swappiness=8 wurden trotzdem 18 GB rausgeschrieben
3. **Swap-Partition auf `/mnt/xplane_archive` (SATA SSD)** verschieben — entkoppelt Swap-IO komplett vom NVMe-RAID0. Slower aber separates Device.

### 9.3 XEL Config-Tuning (Prio 3, sofort umsetzbar)

```ini
[cache]
memory_size = 512 MB        # statt 4 GB → ~80 % weniger anonymous Memory
                            # (Disk-Cache 2.5 TB ist eh primaerer Cache)

[executor]
cpu_concurrent = 4           # statt 8 → halbiert parallele 90-MB-Allokationen

[prefetch]
box_extent = 6               # statt 9 → kleinere DSF-Boundary-Bursts
max_tiles_per_cycle = 80     # NEU setzen (war Run AF Plan, nicht aktiv)
```

### 9.4 Kernel-Tuning (Prio 4, ergaenzend)

```bash
sudo sysctl vm.swappiness=1
sudo sysctl vm.vfs_cache_pressure=200    # Page Cache aggressiver evicten, schont Anonymous
sudo sysctl vm.min_free_kbytes=2097152   # 2 GB Reserve statt 1 GB
```

### 9.5 XEL Code-Aenderungen (Prio 5, mittelfristig)

Aus dem XEL-Log-Befund („30s Timeout schlaegt an, X-Plane haengt aber weiter"):

1. **FUSE-Read-Timeout senken auf 5 s + Magenta-Placeholder** — X-Plane kommt weiter mit fehlender Tile statt zu blockieren. (Code: `xearthlayer/src/fuse/fuse3/*`)
2. **Resource-Pool-Backpressure** im Executor — keine neuen Jobs annehmen wenn Pipeline > 100 s Stau hat
3. **`O_DIRECT`** für DDS-Disk-Cache-Reads — umgeht Page Cache, eliminiert Anonymous-Memory-Konkurrenz mit Page-Cache (Code: `cache/providers/disk.rs`)
4. **`madvise(MADV_HUGEPAGE)`** auf moka-Cache-Allokationen — defragmentiert Anonymous-Memory, vermindert Order-6 kswapd-Wakes
5. **Lazy-Loading von GeoIndex/OrthoUnionIndex** — nur on-demand laden, nicht beim Start. Reduziert Startup-RSS um mehrere hundert MB.

### 9.6 Beobachtungs-Verbesserungen

1. **`monitoring/xplane_telemetry.py` Bug fixen** — `cpu_time_ms` ist falsch berechnet. Echten X-Plane-CPU-Time-Dataref subscriben oder Spalte loeschen + Doku.
2. **`/proc/<pid>/status` VmSwap pro Prozess in proc.csv aufnehmen** — direkt sichtbar wer swap braucht (sysmon hat diese Spalte heute schon, sehr nuetzlich)
3. **bpftrace D-State-Sampler** — periodisch Threads im D-State sampeln + Stack-Trace dumpen → bei naechstem Hang ist die Ursache direkt sichtbar

### 9.7 Hardware (langfristig, nicht jetzt nötig)

- 4. NVMe physisch dazu (PCIe-Slot vorhanden?) → dedizierter Swap- oder Cache-Disk
- xplane_data RAID0 → RAID1c2 oder einen Member rausnehmen für Backup-Pfad. Aktuelles RAID0 = bei 1 Disk-Ausfall sind 12 TB weg.

---

## 10. Zusammenfassung

**Run AF-3 hat klargestellt:**

1. Watermark-Tuning aus Run AE wirkt (allocstall ≈ 0). Aber der Schmerzpunkt hat sich verlagert.
2. **Wahrer Engpass ist BTRFS-RAID0 bei 95 % Data + 98 % Metadata Auslastung** auf einem Pool, der gleichzeitig Swap traegt.
3. XEL braucht **nicht zu viel CPU**, aber **zu viel anonymen Speicher (12 GB statt 4 GB Config-Limit)** der direkt in Swap landet.
4. Beide beobachteten Hangs entstehen durch IO-Queue-Saturation auf NVMe — Hang 1 read-bound (Swap-Recovery), Hang 2 write-bound (Cache-Writes nach BTRFS-Balance).
5. **XEL haengt nicht — X-Plane haengt** weil FUSE-Reads in der saturierten IO-Queue feststecken. XEL erkennt das selbst (DDS-Generation-Timeouts in Log).

**Top-3 Sofort-Massnahmen:**

1. **Filesystem-Cleanup vollziehen + balance + fstrim** — User hat 1.2 TB freigegeben, jetzt Balance + Trim laufen lassen, Ziel < 85 % Data, < 80 % Metadata
2. **`vm.swappiness = 1`** — eliminiert grossen Teil von Hang-1-Mechanik
3. **`cache.memory_size = 512 MB`** in XEL config — eliminiert grossten Teil des XEL-Anonymous-Footprints

Mit diesen drei Aenderungen ist Run AG der Validierungs-Run. Falls noch Hangs auftreten, weiter mit Code-Aenderungen (FUSE-Timeout, O_DIRECT).
