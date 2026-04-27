# Monitoring

Startet eine Monitoring-Session fuer X-Plane 12 Flugsimulation, zeichnet System- und Applikations-Metriken auf und erstellt nach dem Ende eine strukturierte Performance-Analyse.

## Argumente

`$ARGUMENTS`: Optionale Dauer in Minuten

| Aufruf | Beschreibung |
|--------|-------------|
| `/monitoring` | Fragt interaktiv nach Dauer und Optionen |
| `/monitoring 90` | 90-Minuten-Session, ueberspringt Dauer-Frage |

---

## Referenzdokumente

Diese Dateien VOR Beginn laden und als Kontext verwenden:

| Datei | Zweck | Wann lesen |
|-------|-------|-----------|
| `monitoring/README.md` | Architektur-Ueberblick (3-Layer-Modell) | Phase 1 |
| `monitoring/FEATURES.md` | CLI-Optionen, CSV-Spalten, Tracepoint-Details | Phase 1 |
| `monitoring/ANALYSIS_RULES.txt` | **Analyse-Regelwerk** (Schwellwerte, Korrelationsketten, Known Signatures) | Phase 3 |
| `monitoring/ANALYSE_HISTORY.md` | Tuning-Historie (Runs A-G) fuer Vergleich mit neuen Runs | Phase 3 |

---

## Phase 1 — Setup & Start

### 1.1 Konfiguration erfragen

Falls `$ARGUMENTS` leer, AskUserQuestion verwenden:

**Frage 1: Dauer**

| Option | Wert |
|--------|------|
| 20 Min (Kurztest) | `-d 1200` |
| 60 Min (Standard) | `-d 3600` |
| 90 Min (Langflug) | `-d 5400` |
| 120 Min (Maximal) | `-d 7200` |
| 150 Min (Extra-Lang) | `-d 9000` |

**Frage 2: bpftrace-Sidecar**

| Option | Beschreibung |
|--------|-------------|
| Ja (empfohlen) | Layer 2 mitlaufen lassen — liefert Direct Reclaim Attribution, Slow IO, DMA Fence |
| Nein | Nur Layer 1 (sysmon.py) — reicht fuer Grundanalyse |

Falls `$ARGUMENTS` eine Zahl enthaelt: Dauer = Argument in Minuten, Sidecar = Ja (Default).

**WICHTIG:** Dauer IMMER grosszuegig waehlen — lieber zu lang als zu kurz. sysmon.py kann jederzeit gestoppt werden, aber fehlende Daten am Ende sind unwiederbringlich (Run Y: sysmon lief nur 20 Min bei 2h-Flug).

### 1.2 Run-Verzeichnis vorbereiten

Run-Verzeichnis direkt im Repository anlegen (nicht unter `/tmp/`):

```bash
RUN_DIR="monitoring/run_<LABEL>"
mkdir -p "$RUN_DIR"
```

### 1.3 sudo-Befehle ausgeben

**ZUERST** dem User ALLE sudo-Befehle kompakt auflisten, die er in einem separaten Terminal ausfuehren muss:

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
SUDO-BEFEHLE (bitte im separaten Terminal ausfuehren)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

# Layer 2: bpftrace tracer (Direct Reclaim, Slow IO, DMA Fence)
sudo bash monitoring/sysmon_trace.sh -o <RUN_DIR>

# Pre-flight kernel dmesg snapshot
sudo dmesg | tee <RUN_DIR>/dmesg_pre.log > /dev/null

# D-state thread sampler (kernel stack traces of blocked threads)
# CRITICAL for diagnosing X-Plane / FUSE hangs (Run AF-3 lesson)
sudo bash monitoring/dstate_sampler.sh -o <RUN_DIR> -d <SEKUNDEN> &

# Rolling dmesg snapshot (catches mid-flight kernel hung-task warnings,
# NVMe errors, BTRFS warnings — invisible from pre-only snapshot)
sudo bash monitoring/dmesg_rolling.sh -o <RUN_DIR> -d <SEKUNDEN> &

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

Hinweis: Die `&` am Ende bei dstate_sampler und dmesg_rolling sind essenziell, sonst blockiert das Terminal. Der User kann sie alle in einem `sudo bash`-Block hintereinander starten.

### 1.4 Nicht-sudo-Skripte selber starten

Direkt vom Skill-Lauf gestartet, alle in den Hintergrund:

```bash
# Hauptsampler (sysmon.py inkl. xplane_telemetry-Subprocess)
python3 monitoring/sysmon.py -d <SEKUNDEN> --xplane -o "$RUN_DIR" &

# BTRFS allocator state — erkennt Allocator-Druck (>95% used) den ohne Tool
# nicht sichtbar ist. Run AF-3 zeigte: Metadata 98.25% war Hauptursache.
bash monitoring/btrfs_state_sampler.sh -o "$RUN_DIR" &

# FUSE pending requests counter — direkter Indikator fuer Kernel-FUSE-Queue-
# Saturation. Erlaubt Korrelation X-Plane-Hang ↔ FUSE-Queue-Tiefe.
bash monitoring/fuse_pending_sampler.sh -o "$RUN_DIR" &
```

Direkt mit `run_in_background=true` ausfuehren — kein nohup noetig.

### 1.5 Verifikation (PFLICHT — 5-10 Sekunden nach Start)

**Alle Datenquellen pruefen, BEVOR der Flug beginnt:**

```bash
# Prozesse laufen?
ps aux | grep -E 'sysmon\.py|xplane_telemetry|bpftrace|btrfs_state_sampler|fuse_pending_sampler|dstate_sampler|dmesg_rolling' | grep -v grep

# CSVs wachsen?
ls -la "$RUN_DIR"/*.csv

# bpftrace-Traces vorhanden? (falls Sidecar gestartet)
ls -la "$RUN_DIR"/trace_*.log

# Neue Sampler aktiv?
head -2 "$RUN_DIR"/btrfs_state.csv "$RUN_DIR"/fuse_pending.csv 2>&1
ls -la "$RUN_DIR"/trace_dstate.log "$RUN_DIR"/dmesg_rolling.log 2>&1

# dmesg_pre.log nicht leer?
wc -c "$RUN_DIR/dmesg_pre.log"
```

**Bei Problemen sofort melden!** Haeufige Fehler:
- `xplane_telemetry.csv` leer/nur Header → X-Plane UDP-Port nicht offen (Settings > Network > Accept incoming connections)
- `proc.csv` zeigt kein X-Plane → X-Plane noch nicht gestartet (OK, kommt spaeter)
- `trace_reclaim.log` leer → User hat Sidecar noch nicht gestartet (Erinnerung ausgeben)
- `dmesg_pre.log` 0 Bytes → Ownership-Problem (sudo hat Verzeichnis als root angelegt → `sudo chown -R $USER <RUN_DIR>`)
- Run-Verzeichnis gehoert root → Sidecar vor sysmon gestartet. Fix: `sudo chown -R $USER <RUN_DIR>`
- `btrfs_state.csv` leer → BTRFS nicht gemountet oder /sys/fs/btrfs Pfade abweichend
- `fuse_pending.csv` leer → keine FUSE-Mounts aktiv (X-Plane noch nicht gestartet, ok — kommt spaeter)
- `trace_dstate.log` leer → User hat sudo-Block noch nicht ausgefuehrt (Erinnerung)

### 1.6 Status-Meldung

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
MONITORING LAEUFT
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  sysmon.py:        PID <PID>, Dauer <N> Min
  Telemetrie:       xplane_telemetry.py (5 Hz UDP)
  BTRFS Sampler:    PID <PID> (every 30s)
  FUSE Sampler:     PID <PID> (every 1s)
  Sidecar:          <Ja/Nein> (3 bpftrace-Tracer)
  D-state Sampler:  <sudo-active/skipped> (every 5s)
  dmesg Rolling:    <sudo-active/skipped> (every 60s)
  Output:           <RUN_DIR>

  Guten Flug!

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### 1.7 Watchdog — alle 20 Minuten pruefen

Nach dem Start einen Loop-Check einrichten: Alle 20 Minuten automatisch pruefen ob alle Monitoring-Prozesse noch laufen und Daten schreiben.

**Check-Logik (alle 20 Min):**

```bash
# 1. Prozesse noch da?
ps aux | grep -E 'sysmon\.py|xplane_telemetry|bpftrace' | grep -v grep | wc -l

# 2. CSVs wachsen noch? (Dateiaenderung < 60s alt?)
find "$RUN_DIR" -name "*.csv" -mmin -1 | wc -l
```

**Ausfuehrung:** Den `/loop`-Skill mit 20-Minuten-Intervall verwenden. Falls `/loop` nicht verfuegbar, manuell mit `run_in_background` und `sleep 1200` zwischen Checks.

**Bei Ausfall:**
- sysmon.py gestoppt → Neu starten mit Restdauer
- xplane_telemetry.py weg → sysmon.py neu starten (startet Telemetrie als Subprocess)
- bpftrace weg → User informieren, sudo-Befehl erneut ausgeben

### 1.8 Warten

Der Skill pausiert hier. Der User fliegt. Zwei Wege zum Fortfahren:

**A) Timer laeuft ab:** sysmon.py beendet sich automatisch nach der konfigurierten Dauer. Der User meldet sich zurueck (z.B. "fertig", "Analyse starten", "Flug beendet").

**B) User bricht vorzeitig ab:** User sagt "stop" oder "fertig" vor Timer-Ende. Dann ALLE Sampler stoppen:

```bash
# Eigene Hintergrund-Sampler (User-uid)
ps aux | grep -E 'sysmon\.py|xplane_telemetry|btrfs_state_sampler|fuse_pending_sampler' | grep -v grep | awk '{print $2}' | xargs kill 2>/dev/null
```

In beiden Faellen: Dem User mitteilen dass er **im anderen Terminal mit Ctrl+C stoppen** soll:
- `sysmon_trace.sh` (3× bpftrace)
- `dstate_sampler.sh`
- `dmesg_rolling.sh`

---

## Phase 2 — Daten einsammeln

### 2.1 Run-Verzeichnis pruefen

```
Glob: <RUN_DIR>/*.csv
Glob: <RUN_DIR>/trace_*.log
```

Pruefen welche Dateien vorhanden sind. Erwartete Dateien:

- **Immer (sysmon.py):** cpu.csv, mem.csv, io.csv, vram.csv, vmstat.csv, psi.csv, irq.csv, proc.csv, freq.csv
  - **proc.csv neue Spalten (seit 2026-04-27):** rss_anon_mb, rss_file_mb, majflt_s
- **Wenn --xplane:** xplane_telemetry.csv (FPS, CPU/GPU Time, Position, 5 Hz), xplane_telemetry.log
  - **WARN:** `cpu_time_ms` ist DERIVED (frame_period − gpu_time), unterschaetzt echtes CPU bei pipelined render. Quelle: X-Plane Stats Overlay nutzen.
- **Wenn X-Plane lief:** xplane_events.csv
- **BTRFS-Sampler:** btrfs_state.csv (allocator-Zustand, 30s)
- **FUSE-Sampler:** fuse_pending.csv (pending-Counter, 1s)
- **Wenn Sidecar lief:** trace_reclaim.log, trace_io_slow.log (>20ms threshold), trace_fence.log
- **Wenn dstate_sampler.sh lief:** trace_dstate.log (D-state thread stacks)
- **Wenn dmesg_rolling.sh lief:** dmesg_rolling.log (mid-flight kernel events)
- **Crash-Diagnostik:** dmesg_pre.log, dmesg_post.log, gpu_events.log

Fehlende Dateien notieren aber nicht als Fehler werten.

### 2.2 Application Logs identifizieren

XEarthLayer-Log suchen:

```
Glob: ~/.xearthlayer/xearthlayer.log
```

X-Plane Log suchen — die RICHTIGE Log finden:

1. Pruefe ob `xplane_events.csv` existiert (sysmon.py hat Log bereits korreliert)
2. Falls nicht: Pruefe `~/X-Plane-12/Log.txt` — ist der Zeitstempel innerhalb des Monitoring-Fensters?
3. Falls Log.txt neuer als die CSV-Dateien: Suche in `~/X-Plane-12/Output/Log Archive/` nach der passenden archivierten Log

### 2.3 Session-Metadaten bestimmen

Aus den CSV-Dateien extrahieren:

- **Start-Timestamp:** Erste Zeile von cpu.csv, Spalte 1 (Unix Epoch)
- **End-Timestamp:** Letzte Zeile von cpu.csv, Spalte 1
- **Dauer:** End - Start in Sekunden
- **Sample-Count:** Zeilenanzahl cpu.csv (minus Header)

---

## Phase 3 — Analyse

**ANALYSIS_RULES.txt ist das massgebliche Regelwerk.** Dieses Dokument lesen und die 6 Phasen der Analyse durchlaufen.

```
Read: monitoring/ANALYSIS_RULES.txt
```

### 3.1 Daten laden

Die CSV-Dateien sind potentiell gross (90-Min-Session = ~25.000 Zeilen in cpu.csv). Strategie:

- **Uebersicht:** Erste und letzte 50 Zeilen jeder CSV lesen fuer Start/End-Zustand
- **Schluessel-Metriken:** vmstat.csv komplett lesen (1s-Resolution, ~5.400 Zeilen fuer 90 Min — handhabbar)
- **Gezielte Tiefe:** Nur die Zeitfenster detailliert lesen in denen allocstall_s > 0 oder andere Anomalien auftreten
- **XEL-Log:** Nach Event-Typ filtern statt komplett lesen (Log kann 28.000+ Zeilen haben)

### 3.2 Analyse-Phasen ausfuehren

Gemaess ANALYSIS_RULES.txt Sections 1-5:

1. **Overview & Session Context** — Workload, RAM, Phasen-Erkennung
2. **Per-Subsystem Deep Dive** — Memory, CPU, GPU, Disk IO, XEL, X-Plane, **In-Sim Telemetrie** (FPS-Drops, CPU/GPU-Bottleneck aus xplane_telemetry.csv)
3. **Cross-Correlation** — Stutter Chain, IO Latency Chain, Swap Storm Chain, XEL Tile-Loading Storm, **FPS-Drop ↔ allocstall/Reclaim-Korrelation**
4. **Temporal Pattern Analysis** — Drei-Phasen-Modell, Stall-Cluster
5. **Comparison** — Falls fruehere Runs vorhanden (ANALYSE_HISTORY.md laden)

### 3.3 XEL-Log Korrelation

Aus dem XEL-Log die performance-relevanten Events extrahieren und mit den System-Metriken korrelieren:

- **Circuit Breaker Events** (resource-pool-basiert seit PR #61) → vmstat allocstall_s Zeitfenster
- **Turn Events** → io.csv Read-Spikes 5-15s spaeter
- **Prefetch Cache Hit Rate** → mem.csv available_mb Verlauf
- **Flight Phase Transitions** → Drei-Phasen-Grenzen
- **DDS Generation Bursts** → cpu.csv user% Spikes
- **DDS Generation TIMEOUT Events** ("possible executor stall") → Korrelation mit `fuse_pending.csv` peaks und `trace_dstate.log` D-state Zeitfenster
- **`Job started` Bursts** → btrfs_state.csv Metadata-Used%-Sprung (XEL Cache-Writes triggern BTRFS-Allocator)

### 3.4 IO-Druck-Korrelations-Phasen (NEU seit Run AF-3)

Diese Korrelations-Ketten sind PFLICHT bei jedem Run mit Hangs oder hohem iowait:

**A) X-Plane-Hang ↔ Kernel-Stack-Trace**
1. `proc.csv` X-Plane-Threads im D-State zur Hang-Zeit lokalisieren
2. `trace_dstate.log` lesen: welche Kernel-Funktion (wchan + Stack)?
3. Klassifikation: FUSE-Read / Swap-In / BTRFS-Commit / Page-Writeback?

**B) FUSE-Saturation-Cascade**
1. `fuse_pending.csv` peaks identifizieren (waiting > fuse.max_background config)
2. `vmstat.csv` ctxt + pgmajfault zur gleichen Sekunde
3. `xearthlayer.log` Job-Throughput zur gleichen Zeit
4. Falls FUSE-Pending hoch UND XEL Job-Throughput tief → Kernel queued, aber Pipeline blockiert weiter unten

**C) BTRFS-Allocator-Druck**
1. `btrfs_state.csv` Metadata-Used%-Verlauf (Schwellen: > 90 % WARNING, > 95 % CRITICAL)
2. `io.csv` write-Latenz zur gleichen Zeit
3. `trace_io_slow.log` (>20ms) Cluster-Verteilung
4. Korrelation: hoher Metadata%-Druck → höhere write_lat = bestätigt Allocator-Pathology

**D) Per-Process Major Faults (NEU in proc.csv)**
1. `proc.csv` Spalte `majflt_s` pro Prozess lesen
2. Vergleich: vmstat `pgmajfault_s` (system-weit) vs Summe pro Prozess
3. Attribution: welcher Prozess verursacht die Major Faults? Bei X-Plane = Plugin-Pages aus Swap. Bei XEL = Cache-Pages.

**E) Anonymous-Memory-Footprint (NEU in proc.csv)**
1. `proc.csv` Spalte `rss_anon_mb` vs `rss_file_mb` pro Prozess
2. Bei XEL: anon hoch (Heap, moka-Cache) vs file hoch (Disk-Cache via mmap)?
3. Korrelation mit Swap-Druck — anonymous wird gewappet, file kommt aus Page Cache

### 3.5 Parallele Subagents fuer grosse Datenmengen

Bei langen Sessions (> 60 Min) die Analyse auf parallele Subagents aufteilen:

```
Task (subagent_type=general-purpose): "Analysiere vmstat.csv + mem.csv:
  Finde alle allocstall_s > 0 Events, berechne Drei-Phasen-Grenzen,
  Memory-Statistiken pro Phase. Datei: <RUN_DIR>/vmstat.csv"

Task (subagent_type=general-purpose): "Analysiere io.csv:
  Per-Device Throughput/Latenz-Statistiken, IO-Spikes > 100 MB/s,
  NVMe-Latenz-Cluster (10-11ms Pattern). Datei: <RUN_DIR>/io.csv"

Task (subagent_type=general-purpose): "Parse XEL Log fuer Zeitfenster
  <START> bis <END>: Circuit Breaker Events (resource-pool), Turn Events, Prefetch
  Cache Hit Rates, Flight Phases. Datei: ~/.xearthlayer/xearthlayer.log"
```

### 3.6 Quality Checklist

Gemaess ANALYSIS_RULES.txt Section 7 vor Abschluss pruefen:

- [ ] Alle CSV-Dateien gelesen oder als fehlend notiert
- [ ] xplane_telemetry.csv ausgewertet: FPS-Drops identifiziert, CPU/GPU-Bottleneck bestimmt
  - [ ] **WICHTIG:** cpu_time_ms ist DERIVED, X-Plane Stats Overlay als Source-of-Truth nutzen
- [ ] Drei-Phasen-Grenzen identifiziert mit Minutenangaben
- [ ] Jeder allocstall > 0 Event hat eine wahrscheinliche Ursache
- [ ] Cross-Korrelation durchgefuehrt (Stalls ↔ IO ↔ Swap ↔ GPU ↔ XEL)
- [ ] XEL-Log geparst und korreliert
- [ ] Flight Phases aus XEL dem Drei-Phasen-Modell zugeordnet
- [ ] Circuit Breaker Events mit Resource-Pool-Auslastung abgeglichen (post-PR #61: sollte selten sein)
- [ ] Trace-Logs geparst und integriert (falls vorhanden)
- [ ] **Bei Hangs:** trace_dstate.log analysiert, Kernel-Stack des blockierten Threads identifiziert
- [ ] **btrfs_state.csv:** Metadata-Used%-Verlauf geprueft, Korrelation mit IO-Latenz
- [ ] **fuse_pending.csv:** Saturation-Peaks identifiziert (waiting > config max_background)
- [ ] **proc.csv:** Per-Process majflt_s, rss_anon_mb, rss_file_mb fuer XEL und X-Plane analysiert
- [ ] **dmesg_rolling.log:** auf "task hung", NVMe-Errors, BTRFS-WARN durchsucht
- [ ] Findings nach Schweregrad sortiert
- [ ] Empfehlungen spezifisch und umsetzbar
- [ ] Vergleich mit frueheren Runs (falls Daten vorhanden)

---

## Phase 4 — Bericht erstellen

### 4.1 Bericht-Datei schreiben

Bericht als Markdown in das Monitoring-Verzeichnis schreiben:

```
monitoring/ANALYSE_RUN_<LABEL>_<DATUM>.md
```

`<LABEL>` aus dem Run-Verzeichnisnamen ableiten oder den User fragen (z.B. "H", "I", "test_01").

### 4.2 Bericht-Format

Gemaess ANALYSIS_RULES.txt Section 4:

```markdown
# Run <LABEL> — Ergebnisse: <Dauer>-Minuten <Flugtyp>

**Datum:** <YYYY-MM-DD>
**System:** <CPU>, <RAM> GB RAM, <GPU> <VRAM> GB, <Storage>
**Kernel:** <Kernel-Version>
**Workload:** <Prozesse aus proc.csv>
**Aenderungen seit Run <vorheriger>:** <was sich geaendert hat>

---

## 0. Testbedingungen
(Dauer, Samples, Tuning-Parameter, Sidecar-Status)

## 1. Erwartungen vs. Ergebnisse
(Tabelle mit Key Metrics, Vergleich zu vorherigem Run)

## 2. Kernbefunde
### 2.1 Drei-Phasen-Verhalten
### 2.2 Memory Pressure
### 2.3 XEarthLayer Streaming Activity
### 2.4 Direct Reclaim (falls Trace-Daten)
### 2.5 Alloc-Stall-Cluster

## 3. In-Sim Telemetrie (FPS / CPU Time / GPU Time)

## 4. GPU / VRAM

## 5. Disk IO

## 6. CPU & Frequenz

## 7. Per-Process

## 8. Vergleich Run <vorheriger> → Run <aktuell>

## 9. Handlungsempfehlungen
### 9.1-N Konkrete, priorisierte Massnahmen

## 10. Zusammenfassung
```

### 4.3 Run-Daten pruefen

Run-Daten liegen bereits im Repository (seit Phase 1.2 direkt unter `monitoring/run_<LABEL>/` angelegt). Pruefen ob alle erwarteten Dateien vorhanden und nicht leer sind.

### 4.4 ANALYSE_HISTORY.md aktualisieren

Den neuen Run als Eintrag in `monitoring/ANALYSE_HISTORY.md` anfuegen:

- Neuer Abschnitt am Ende mit Run-Label, Datum, Key Metrics
- Ggf. Tuning-Aenderungen dokumentieren

---

## Phase 5 — Abschluss

### 5.1 Zusammenfassung ausgeben

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
MONITORING-ANALYSE ABGESCHLOSSEN
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

SESSION:
├─ Dauer:        <N> Minuten (<Samples> Samples)
├─ Run-Dir:      <RUN_DIR>
├─ Sidecar:      <Ja/Nein>
└─ XEL-Log:      <Ja/Nein>

KEY FINDINGS:
├─ [CRITICAL] <Finding 1>
├─ [WARNING]  <Finding 2>
└─ [INFO]     <Finding 3>

PHASEN:
├─ Warm-up:     Min 0–<N>
├─ Ramp-up:     Min <N>–<M>  (<X> Alloc Stalls)
└─ Steady:      Min <M>–Ende (<Y> Alloc Stalls)

BERICHT:
└─ monitoring/ANALYSE_RUN_<LABEL>_<DATUM>.md

TOP-3 EMPFEHLUNGEN:
  1. <Empfehlung>
  2. <Empfehlung>
  3. <Empfehlung>

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### 5.2 Dateien NICHT committen

Monitoring-Analysen werden NICHT automatisch committed. Der User entscheidet ob und wann committed wird (ggf. via `/abschluss`).

---

## Hinweise

- **Sampler-Inventar (Stand 2026-04-27):**
  - **No sudo (skill startet automatisch):**
    - `sysmon.py` (mit `--xplane`) — alle Hauptmetriken + xplane_telemetry.py-Subprocess
    - `btrfs_state_sampler.sh` — BTRFS-Allocator-Status alle 30s
    - `fuse_pending_sampler.sh` — FUSE-Kernel-Queue alle 1s
  - **Sudo erforderlich (User startet):**
    - `sysmon_trace.sh` — 3× bpftrace (reclaim, io_slow >20ms, fence)
    - `dmesg | tee dmesg_pre.log` — Pre-Flight Snapshot
    - `dstate_sampler.sh` — D-state Thread Stack Traces alle 5s (kritisch fuer Hang-Diagnose)
    - `dmesg_rolling.sh` — periodisches dmesg alle 60s
- **XEL-Log wird NICHT rotiert:** `~/.xearthlayer/xearthlayer.log` enthaelt moeglicherweise mehrere Sessions. Nur Events innerhalb des Monitoring-Zeitfensters auswerten.
- **XEL-Log nutzt UTC, System-Daten lokal:** Bei Korrelation umrechnen! 17:00 lokal Berlin = 15:00 UTC.
- **X-Plane Log Archive:** Falls `Log.txt` neuer als die CSV-Dateien ist, die archivierte Log aus `~/X-Plane-12/Output/Log Archive/` verwenden.
- **Grosse CSV-Dateien:** Nicht komplett in den Kontext laden. Gezielt Zeitfenster und Schluessel-Spalten lesen.
- **trace_io_slow.log Schwelle:** 20ms (seit Run AF-3) — vorher 5ms war zu noisy (1.7 GB/57min).
- **ANALYSIS_RULES.txt ist bindend:** Alle Schwellwerte, Korrelationsketten und Known Signatures aus diesem Dokument verwenden. Keine eigenen Schwellwerte erfinden.
- **Vergleich mit frueheren Runs:** `monitoring/ANALYSE_HISTORY.md` und vorhandene `ANALYSE_RUN_*.md` laden fuer Kontext.
- **Run-Verzeichnisse:** Werden direkt unter `monitoring/run_<label>/` angelegt (kein /tmp/ mehr).
- **Watchdog:** Alle 20 Min pruefen ob sysmon.py + xplane_telemetry + btrfs_state_sampler + fuse_pending_sampler + bpftrace + dstate_sampler + dmesg_rolling noch laufen. Bei Ausfall: User-uid Sampler neu starten, fuer Sudo-Sampler User informieren.
