# Run AG — Mess-Plan: XEarthLayer IO-Druck-Validierung

**Geplantes Datum:** TBD (nach Filesystem-Hygiene-Abschluss)
**Vorgaenger:** Run AF-3 (2026-04-27)
**Hauptziel:** Validieren dass die 5 Config-Aenderungen + Filesystem-Cleanup die Hangs eliminieren und IO-Druck quantifizierbar reduzieren.

## Hypothesen die getestet werden

| # | Hypothese | Belegt durch (Metrik) |
|---|---|---|
| 1 | `prefetch.box_extent=6.5` + `cycle_interval_ms=4000` reduzieren Burst-Volume um ~75 % | XEL-Log: Job-starts/min sollten von 837/min auf < 250/min sinken |
| 2 | `cpu_concurrent=16` entstarvt die Encode-Pipeline | Job-Latenz p95 sollte sinken; CPU user% leicht steigen |
| 3 | `fuse.max_background=128` verhindert Cascade-Hangs | iowait Run-Mittel < 25 %, keine Hangs |
| 4 | Filesystem-Cleanup eliminiert BTRFS-Allocator-Pathology | Slow-IO Tail (>1000ms) < 15 %, w_lat avg < 5 ms |
| 5 | Memory-Druck nimmt automatisch ab durch weniger Bursts | swap_used wachst weniger, pswpout-Bursts unter 50k/s |

## Pre-Conditions (PFLICHT vor Start)

### A. Filesystem-State validieren

```bash
sudo btrfs filesystem usage /mnt/xplane_data | grep -E 'Metadata|Data,RAID|Unallocated'
df -h /mnt/xplane_data /home
```

**Anforderung:**
- Data RAID0 < 90 % (idealerweise < 85 %)
- Metadata RAID1 < 95 % (idealerweise < 85 %)
- xplane_data Free > 1.5 TB (idealerweise > 2 TB)

Falls noch nicht: `sudo fstrim -av`, weitere `sudo btrfs balance start -dusage=70 /mnt/xplane_data`.

### B. Config-Aenderungen aktiv

```bash
grep -E '^cpu_concurrent|^cycle_interval_ms|^box_extent|^max_background|^congestion_threshold' ~/.xearthlayer/config.ini
```

**Erwartung:**
```
cpu_concurrent = 16
cycle_interval_ms = 4000
box_extent = 6.5
max_background = 128
congestion_threshold = 96
```

### C. Sysctls unveraendert (Author-Baseline)

```bash
/sbin/sysctl vm.swappiness vm.min_free_kbytes vm.watermark_scale_factor vm.dirty_ratio vm.dirty_background_ratio vm.page-cluster
```

**Erwartung:**
```
vm.swappiness = 8
vm.min_free_kbytes = 1048576
vm.watermark_scale_factor = 500
vm.dirty_ratio = 10
vm.dirty_background_ratio = 3
vm.page-cluster = 0
```

### D. xearthlayer und X-Plane neu gestartet

Damit XEL die neue Config laedt + Anonymous-Memory-Footprint frisch ist (kein Carryover aus Stunden-Vor-Session). Pre-Run sollte Swap-Inhalt nahe 0 sein.

```bash
pkill -f 'xearthlayer run' && sleep 5 && pgrep xearthlayer || echo "stopped"
free -h | head -2
# Erwartung: Swap-used < 2 GB (sonst System neu booten oder swapoff && swapon -a)
```

## Mess-Setup

### Dauer

**90 Min** (5400 s) — gleicher Wert wie AF-3 fuer Vergleichbarkeit. Falls Hangs frueher: trotzdem volle Daten erfassen, nicht abbrechen (auch fuer Vergleich gegen AF-3 wichtig).

### Layer

- **Layer 1 sysmon.py** mit `--xplane` (Pflicht, 5 Hz Telemetrie)
- **Layer 2 sysmon_trace.sh** (Pflicht — Direct-Reclaim, Slow-IO, DMA-Fence)
- **dmesg_pre.log** Pflicht (Crash-Forensik bei evtl. Hang)

### Ergaenzende Mess-Punkte fuer XEL-Fokus

1. **XEL Job-Activity-Snapshot** alle 5 Min in NOTES.md:
   ```bash
   echo "=== $(date) ==="
   tail -5 ~/.xearthlayer/xearthlayer.log
   awk -v cutoff=$(date -d '5 minutes ago' +%s) '
     /Job started/ {jobs++}
     /Job completed/ {done++; durations[NR] = $NF}
     /TIMEOUT/ {timeouts++}
     END {
       print "Jobs started:", jobs, "completed:", done, "timeouts:", timeouts
     }
   ' ~/.xearthlayer/xearthlayer.log | tail
   ```

2. **Resource-Pool-Live-Check** alle 10 Min:
   ```bash
   ps -o %cpu,rss,vsize,nlwp -p $(pgrep -f 'xearthlayer run')
   cat /proc/$(pgrep -f 'xearthlayer run')/io 2>/dev/null
   ```

3. **iostat-Sample am Ende des Runs**:
   ```bash
   iostat -x 1 5 > monitoring/run_AG/iostat_endsample.txt
   ```

## Test-Route

**Empfehlung 1 (Reproduktion AF-3):** Gleiche Cruise-Position Persian Gulf (lat 26.3 N, lon 52 E), ueberfliege DSF-Boundaries die in AF-3 Hang ausloesten. Direkter Vergleich AF-3 ↔ AG.

**Empfehlung 2 (anspruchsvoller):** Lange Route ueber neue Tiles, z.B. EDDF→OMDB Cruise. Maximaler Cache-Miss-Druck, simuliert worst-case fuer Tile-Generation. **Aber** unfair zu AF-3 fuer Vergleich.

→ **Empfohlen: Empfehlung 1** fuer A/B-Vergleich. Nach Validierung kann Run AH dann die anspruchsvolle Route fahren.

## Live-Beobachtung waehrend Flug — Dashboards

### Watchdog (alle 20 Min — Sysmon-Skill macht das automatisch)
- sysmon.py + xplane_telemetry.py + 3× bpftrace alive
- CSVs wachsen
- xearthlayer + X-Plane alive

### Manuelle Sanity-Checks zwischendurch (im laufenden Cruise alle 10 Min)
- **iowait < 30 %** dauerhaft? Falls > 50 % anhaltend → Indiz dass Aenderungen nicht greifen
- **xearthlayer CPU < 250 %**? Falls hoch und steigend → Pipeline starvet evtl wieder
- **Swap-Wachstum < 1 GB/10 Min**? Falls hoeher → Memory-Druck eskaliert
- **FPS-Drop-Frequenz < 1/Min**? Falls oefter → Hang-Vorbote

### Bei Verdacht auf Hang
1. **NICHT sofort killen** — Daten sammeln:
   ```bash
   # D-State Threads dumpen
   ps -eL -o stat,pid,tid,comm | awk '$1 ~ /D/'
   # IO-Live
   iostat -x 1 5
   # vmstat-Burst
   vmstat 1 10
   # XEL-Log letzte 100 Zeilen
   tail -100 ~/.xearthlayer/xearthlayer.log
   ```
2. **Position notieren** (lat/lon aus xplane_telemetry.csv tail)
3. **Erst dann** kill, NOTES.md ergaenzen

## Auswertungs-Schwerpunkte fuer den Run-AG-Bericht

### Pflicht-Vergleichs-Tabelle Run AF-3 ↔ Run AG

| Metrik | AF-3 | AG (Soll) | AG (Ist) |
|---|---|---|---|
| Hangs | 2 | 0 | ? |
| iowait Run-Mittel | 42 % | < 25 % | ? |
| Slow-IO > 1000 ms Anteil | 37.4 % | < 15 % | ? |
| pgmajfault peak | 8997/s | < 3000/s | ? |
| swap-used max | 22 GB | < 12 GB | ? |
| pswpout peak | 184k/s | < 50k/s | ? |
| FPS Drops < 25 | 9.4 % | < 5 % | ? |
| Tile Job starts/min Cruise | ~837 | < 350 | ? |
| Tile Job p95 Latenz | ~26 s | < 15 s | ? |
| TIMEOUT Events | ~70 in 4 min | 0 | ? |

### Wenn Hangs ausbleiben — Erfolgsmessung

1. Konkretisieren welche Aenderung **am meisten** beigetragen hat (Code-Pfade live nachvollziehen, ggf. einzelne Aenderung wieder rueckdrehen in Run AH)
2. Storage-Latenz-Histogramme erstellen (pre-Cleanup vs post-Cleanup)
3. Empfehlung: in CLAUDE.md / Doku schreiben

### Wenn Hangs auftreten — Eskalations-Pfad

| Beobachtung | Naechster Schritt |
|---|---|
| Hang trotz Config-Aenderungen, IO-Pattern wie AF-3 | **O_DIRECT in disk.rs** implementieren — der grosse strukturelle Fix |
| Hang trotz Config + O_DIRECT, Swap > 10 GB | `swapoff -a` testen (Run AH ohne Swap) |
| Hang trotz allem, Disk-Latenz hoch | xplane_data weiter entlasten unter 80 % |
| Hang nur bei DSF-Boundary-Crossings | Prefetch-Backpressure-Loop in coordinator.rs (Code-Aenderung) |

## Was nach Run AG kommt (Tentativ)

- **Run AH:** Nur eine zusaetzliche Aenderung gegenueber AG, um zu isolieren welche XEL-Settings den groessten Effekt hatten (z.B. nur box_extent zurueck auf 9, andere bleiben). Bestaetigt Wirkung pro Setting.

- **Run AI:** O_DIRECT Code-Aenderung integrieren (falls AG noch Restdruck zeigt), gleicher Test wie AG, Vergleich mit/ohne O_DIRECT.

- **Run AJ:** Long-Cruise (3+ h) zur Validierung dass Memory-Footprint nicht ueber Stunden eskaliert.

## Aktive Code-Hotspots zum Beobachten (fuer ev. Code-Fix)

Nach Run AG ggf. genauer pruefen:
- `xearthlayer/src/cache/providers/disk.rs:273-316` — fn `set` ohne O_DIRECT, `tokio::fs::write` + rename
- `xearthlayer/src/prefetch/adaptive/coordinator/core.rs` — Coordinator main loop, kein IO-Backpressure-Hook
- `xearthlayer/src/executor/resource_pool.rs:88-104` — DiskIO Pool mit DISK_IO_CAPACITY_NVME=256, evtl. zu hoch trotz config 128

## Erfolgs-Definition

**Run AG ist ein Erfolg wenn:**
1. **0 unrecoverable Hangs** in 90 Min
2. **iowait Run-Mittel < 30 %** (vs 42 % in AF-3)
3. **Tile-Throughput stabil ≥ 600/Min** (Performance nicht stark gesunken)
4. **Bei DSF-Boundary-Crossings keine FPS-Drops > 2 s**
5. **Slow-IO Tail-Latency (>1000 ms) reduziert um ≥ 50 %**

**Bei 4 von 5 erfuellten Kriterien:** Aenderungen freigeben, in CLAUDE.md dokumentieren als Standard.
**Bei 3 oder weniger:** O_DIRECT-Code-Aenderung wird Run AH.
