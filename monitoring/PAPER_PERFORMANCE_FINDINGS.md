# Eliminating Micro-Stutters in Linux Flight Simulation with On-Demand Ortho Streaming

**System:** Ryzen 9 9800X3D (8C/16T), 96 GB RAM, RTX 4090 24 GB, 3x NVMe RAID0
**Kernel:** Liquorix 6.19 (PDS Scheduler), Btrfs
**Workload:** X-Plane 12 + XEarthLayer (FUSE-basiertes DDS-Streaming) + QEMU/KVM
**Zeitraum:** 16 Runs, Februar–Maerz 2026

---

## Das Problem

X-Plane 12 mit on-demand Ortho-Streaming verbraucht waehrend eines Fluges kontinuierlich Speicher: X-Plane selbst (bis 21 GB RSS), der Streaming-Layer XEarthLayer (bis 16 GB RSS) und Hintergrund-VMs (4 GB). Bei 96 GB physischem RAM fuehrt das nach 30–60 Minuten zu Memory Pressure.

Wenn der Linux-Kernel unter Druck Speicher zurueckfordern muss, geschieht das im schlimmsten Fall **synchron auf dem Thread, der den Speicher anfordert** — sogenanntes Direct Reclaim. Trifft das X-Planes Render-Loop, entsteht ein Mikro-Stutter: der Frame verzoegert sich um 10–80 ms, sichtbar als kurzes Ruckeln. Diese Events sind mit klassischen Tools (htop, top) unsichtbar, da sie in Millisekunden-Bursts auftreten.

## Messmethodik

Drei-Schichten-Monitoring:
- **Layer 1:** Polling von /proc und NVML alle 200 ms (CPU, RAM, Disk IO, GPU, Per-Process) plus X-Plane FPS/Frame-Timing via UDP RREF (5 Hz)
- **Layer 2:** bpftrace auf Kernel-Tracepoints (mm_vmscan_direct_reclaim, block_rq_issue, dma_fence_wait) — zeigt welcher Prozess und Thread vom Reclaim betroffen ist
- **Layer 3:** Post-Crash-Diagnostik (dmesg, NVIDIA Bug Report)

Die Kombination aus vmstat-Zaehler (allocstall_s) und bpftrace-Attribution ist entscheidend: vmstat zeigt *dass* Stalls auftreten, bpftrace zeigt *wer* betroffen ist. Sessions unter 60 Minuten verpassen die Ramp-up-Phase und sind fuer die Analyse wertlos.

## Erkenntnis 1: kswapd-Watermarks sind der primaere Schutz

**Das Prinzip:** Der Kernel verwaltet drei Watermarks pro Speicherzone:
- **WMARK_HIGH:** kswapd schlaeft ein (genuegend freier Speicher)
- **WMARK_LOW:** kswapd wacht auf und beginnt asynchrones Background-Reclaim
- **WMARK_MIN:** Allokierende Threads werden blockiert und muessen selbst reclaimen (Direct Reclaim)

Der Abstand zwischen LOW und MIN ist der kswapd-Vorlauf: je groesser, desto unwahrscheinlicher ist Direct Reclaim.

**Die Messung:**

| Watermark-Tuning | Direct Reclaim Main Thread | Max Latenz | FPS < 25 |
|------------------|---------------------------|------------|----------|
| Default (min_free=66 MB) | 12.472 Events | 80 ms | 6,9% |
| min_free=2 GB, wsf=125 | 0 (kurze Fluege) | 0 ms | 3,1% |
| min_free=2 GB, wsf=125 | 20.515 (Europa 90 min) | 80 ms | 3,8% |
| min_free=3 GB, wsf=125 | 0 (Europa 150 min) | 0 ms | 3,6% |

**Das richtige Werkzeug:** `min_free_kbytes` steuert gleichzeitig die Emergency Reserve (WMARK_MIN) und die kswapd-Watermarks. Bei 3 GB werden 3 GB RAM komplett fuer Userspace gesperrt — Verschwendung. Der Kernel-Parameter `watermark_scale_factor` (eingefuehrt durch Kernel-Commit 795ae7a0 von Johannes Weiner) steuert den kswapd-Vorlauf *unabhaengig* von der Emergency Reserve:

```
VORHER: min_free_kbytes=3GB, watermark_scale_factor=125
  WMARK_MIN  = 3,0 GB  (gesperrt — verschwendet)
  WMARK_LOW  = 4,2 GB  (kswapd wacht auf)
  WMARK_HIGH = 5,4 GB

BESSER: min_free_kbytes=1GB, watermark_scale_factor=500
  WMARK_MIN  = 1,0 GB  (nur 1 GB gesperrt)
  WMARK_LOW  = 5,8 GB  (kswapd wacht FRUEHER auf)
  WMARK_HIGH = 10,6 GB (MEHR Vorlauf)
```

Mehr Schutz bei 2 GB weniger Verschwendung.

## Erkenntnis 2: zram ist unnoetig

zram (komprimierter In-Memory-Swap) wurde in fruehen Runs als Loesung eingefuehrt und schien Direct Reclaim zu eliminieren. Die Hypothese war: zram haelt ausgelagerte Pages im RAM, wodurch Swap-IO entfaellt und Reclaim schneller wird.

**Die Widerlegung:** Run AD lief ohne zram (nur NVMe-Partition als Swap) und erreichte trotzdem 0 Direct Reclaim ueber 150 Minuten. Der Vergleich:

| Konfiguration | Direct Reclaim | Erklaerung |
|---------------|---------------|------------|
| Ohne zram, min_free_kbytes = 66 MB | 54.686 | Kein Schutz |
| Mit zram, min_free_kbytes = 2 GB | 0 | Schutz durch min_free, nicht durch zram |
| Ohne zram, min_free_kbytes = 3 GB | 0 | Bestaetigung: min_free allein reicht |

zram hat das Problem maskiert, nicht geloest. Es reduzierte die Swap-Latenz, aber der eigentliche Mechanismus (kswapd-Vorlauf durch min_free_kbytes) war bereits ausreichend. zram kann aus dem Tuning-Stack entfernt werden — eine Komplexitaetsreduktion ohne Funktionsverlust.

## Erkenntnis 3: FOPEN_DIRECT_IO verhindert FUSE-induzierten Reclaim

XEarthLayer stellt DDS-Texturen ueber ein FUSE-Dateisystem bereit. Ohne DIRECT_IO cached der Kernel jede FUSE-Antwort im Page Cache — das verdoppelt den Speicherverbrauch (einmal im XEL Memory-Cache, einmal im Kernel Page Cache) und erzeugt zusaetzlichen Reclaim-Druck.

Mit `FOPEN_DIRECT_IO` auf den virtuellen DDS-Dateien umgeht der Kernel den Page Cache fuer FUSE-Reads. Die Daten fliessen direkt: XEL Userspace → FUSE Kernel-Modul → X-Plane. Das eliminiert eine komplette Klasse von Reclaim-Events.

## Erkenntnis 4: vmstat allocstall_s und bpftrace messen verschiedene Dinge

Run AD zeigte 71.630 allocstalls (vmstat) bei gleichzeitig 0 Direct Reclaim Events (bpftrace). Der `allocstall`-Zaehler im Kernel wird inkrementiert wenn ein Thread den Slow Path von `__alloc_pages_slowpath` betritt — das ist per Definition der Direct-Reclaim-Pfad. Die Diskrepanz zu bpftrace (0 Events auf `mm_vmscan_direct_reclaim_begin/end`) deutet auf Reclaim-Throttling hin: moderne Kernel (5.15+) koennen Direct Reclaimers drosseln und auf Writeback-Completion warten lassen, ohne dass die vmscan-Tracepoints feuern.

**Konsequenz fuer Monitoring:** vmstat allocstall_s allein ist ein unzureichender Stutter-Indikator. Die bpftrace-Attribution (mm_vmscan_direct_reclaim_begin/end) zeigt, ob tatsaechlich synchrones Page-Scanning auf dem Render-Thread stattfindet. Das FPS-Ergebnis (3,6% unter 25 FPS bei 71K allocstalls) bestaetigt: nicht jeder allocstall fuehrt zu spuerbarem Stutter.

## Erkenntnis 5: Der Kernel Page Cache ist kein Problem

Bei 41 GB Page Cache waehrend des Fluges lag die Vermutung nahe, dass aggressiveres Cache-Raeumen (vfs_cache_pressure > 100) helfen wuerde. Die Analyse zeigte:

- Der Page Cache besteht aus X-Planes Scenery-Reads (DSF, Meshes) und XELs Disk-Cache-Writes
- Clean Page-Cache-Pages brauchen kein Disk-IO zum Freigeben (aber LRU-Scanning, Lock-Contention und TLB-Shootdowns verursachen messbare CPU-Kosten)
- FUSE-DDS-Daten sind dank DIRECT_IO NICHT im Page Cache
- vfs_cache_pressure steuert primaer Dentry/Inode-Caches, nicht den Page Cache

Der Page Cache beschleunigt wiederholte Reads und wird bei Bedarf ohne Disk-IO freigegeben. `vfs_cache_pressure`-Tuning ist unnoetig, da dieser Parameter nur Dentry/Inode-Caches steuert, nicht den Page Cache selbst. Bei sehr grossen Page Caches (>40 GB) koennen LRU-Scanning-Kosten allerdings relevant werden — ein Argument fuer `POSIX_FADV_DONTNEED` auf One-Shot-Reads.

## Erkenntnis 6: RSS-Wachstum kommt aus Encoding-Bursts

XEarthLayer zeigt 15,8 GB RSS bei 2 GB konfiguriertem Memory-Cache. Die Code-Analyse identifizierte die Ursachen:

| Quelle | Speicher | Mechanismus |
|--------|----------|-------------|
| Memory Cache (moka LRU) | 2 GB | Konfiguriert, funktioniert korrekt |
| Mipmap-Clones beim DDS-Encoding | bis 11 GB | 5 RgbaImage-Kopien pro Tile × 128 concurrent Tasks |
| FUSE Buffer-Kopien | bis 6 GB | Bytes::copy_from_slice() bei jedem X-Plane read() |
| Thread Stacks + Overhead | ~1 GB | Bis 549 Threads bei Prefetch-Bursts |

Der Memory-Cache ist nicht das Problem — die transienten Encoding-Buffers sind es. Ein groesserer Cache (4 statt 2 GB) reduziert Cache-Misses und damit die Anzahl der Encoding-Bursts, was paradoxerweise den Spitzen-RSS senken kann.

## Resultierender Tuning-Stack

```
vm.min_free_kbytes        = 1048576   (1 GB — Emergency Reserve)
vm.watermark_scale_factor = 500       (kswapd-Vorlauf ~4,8 GB)
vm.swappiness             = 8         (Swap nur bei echtem Druck)
vm.page_cluster           = 0         (einzelne Pages swappen, nicht Cluster)
vm.vfs_cache_pressure     = 100       (Default, kein Tuning noetig)
vm.dirty_background_ratio = 3
vm.dirty_ratio            = 10
IO-Scheduler              = none      (alle NVMe)
WBT                       = 0         (Write-Back-Throttling aus)
Readahead                 = 256 KB
irqbalance                = aktiv     (NVMe-IRQs auf alle CPUs)
FUSE                      = FOPEN_DIRECT_IO auf virtuellen DDS-Dateien
zram                      = nicht noetig
```

Sechs Parameter sind funktional relevant. Der Rest ist Default oder entbehrlich.

## Offene Fragen

1. **Memory-Cache-Groesse:** 4 GB statt 2 GB koennte Encoding-Bursts reduzieren (naechster Test)
2. **Mipmap-Encoding:** In-place statt Clone wuerde den groessten RSS-Treiber eliminieren
3. **Prefetch-Geometrie:** Proportionale Heading-Bias (box_extent 6,5°) wurde nur auf geradliniger Route getestet
