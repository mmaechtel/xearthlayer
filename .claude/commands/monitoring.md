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

Waehrend des Flugs werden alle Daten nach `/tmp` geschrieben (tmpfs = RAM-backed, kein Disk-I/O, minimale Systembelastung). Erst bei der Auswertung (Phase 2) werden die Daten ins Repository kopiert.

```bash
TMP_DIR="/tmp/sysmon_run_<LABEL>"
mkdir -p "$TMP_DIR"
```

Das Repo-Verzeichnis wird erst in Phase 2 angelegt.

### 1.3 sudo-Befehle ausgeben

**ZUERST** dem User ALLE sudo-Befehle kompakt auflisten, die er in einem separaten Terminal ausfuehren muss:

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
SUDO-BEFEHLE (bitte im separaten Terminal ausfuehren)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

sudo bash monitoring/sysmon_trace.sh -o <TMP_DIR>
sudo dmesg | tee <TMP_DIR>/dmesg_pre.log > /dev/null

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Alle Daten landen in /tmp (RAM) — kein Disk-I/O waehrend des Flugs.
```

### 1.4 Nicht-sudo-Skripte selber starten

sysmon.py im Hintergrund starten (IMMER mit `--xplane` fuer FPS/CPU/GPU-Telemetrie):

```bash
python3 monitoring/sysmon.py -d <SEKUNDEN> --xplane -o "$TMP_DIR" &
```

Direkt mit `run_in_background=true` ausfuehren — kein nohup noetig.

### 1.5 Verifikation (PFLICHT — 5-10 Sekunden nach Start)

**Alle Datenquellen pruefen, BEVOR der Flug beginnt:**

```bash
# Prozesse laufen?
ps aux | grep -E 'sysmon\.py|xplane_telemetry|bpftrace' | grep -v grep

# CSVs wachsen?
ls -la "$TMP_DIR"/*.csv

# bpftrace-Traces vorhanden? (falls Sidecar gestartet)
ls -la "$TMP_DIR"/trace_*.log

# dmesg_pre.log nicht leer?
wc -c "$TMP_DIR/dmesg_pre.log"
```

**Bei Problemen sofort melden!** Haeufige Fehler:
- `xplane_telemetry.csv` leer/nur Header → X-Plane UDP-Port nicht offen (Settings > Network > Accept incoming connections)
- `proc.csv` zeigt kein X-Plane → X-Plane noch nicht gestartet (OK, kommt spaeter)
- `trace_reclaim.log` leer → User hat Sidecar noch nicht gestartet (Erinnerung ausgeben)
- `dmesg_pre.log` 0 Bytes → Ownership-Problem (sudo hat Verzeichnis als root angelegt → `sudo chown -R $USER <TMP_DIR>`)
- /tmp-Verzeichnis gehoert root → Sidecar vor sysmon gestartet. Fix: `sudo chown -R $USER <TMP_DIR>`

### 1.6 Status-Meldung

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
MONITORING LAEUFT
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  sysmon.py:  PID <PID>, Dauer <N> Min
  Telemetrie: xplane_telemetry.py (5 Hz UDP)
  Sidecar:    <Ja/Nein> (3 bpftrace-Tracer)
  Output:     <TMP_DIR> (tmpfs/RAM)

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
find "$TMP_DIR" -name "*.csv" -mmin -1 | wc -l
```

**Ausfuehrung:** Den `/loop`-Skill mit 20-Minuten-Intervall verwenden. Falls `/loop` nicht verfuegbar, manuell mit `run_in_background` und `sleep 1200` zwischen Checks.

**Bei Ausfall:**
- sysmon.py gestoppt → Neu starten mit Restdauer
- xplane_telemetry.py weg → sysmon.py neu starten (startet Telemetrie als Subprocess)
- bpftrace weg → User informieren, sudo-Befehl erneut ausgeben

### 1.8 Warten

Der Skill pausiert hier. Der User fliegt. Zwei Wege zum Fortfahren:

**A) Timer laeuft ab:** sysmon.py beendet sich automatisch nach der konfigurierten Dauer. Der User meldet sich zurueck (z.B. "fertig", "Analyse starten", "Flug beendet").

**B) User bricht vorzeitig ab:** User sagt "stop" oder "fertig" vor Timer-Ende. Dann sysmon.py und xplane_telemetry.py stoppen:

```bash
# PIDs finden und stoppen
ps aux | grep -E 'sysmon\.py|xplane_telemetry' | grep -v grep | awk '{print $2}' | xargs kill
```

In beiden Faellen: Dem User mitteilen dass er `sysmon_trace.sh` im anderen Terminal mit Ctrl+C stoppen soll (falls gestartet).

---

## Phase 2 — Daten einsammeln

### 2.1 Daten von /tmp ins Repository kopieren

Nach dem Flug die gesammelten Daten von tmpfs ins Repo-Verzeichnis kopieren. Ab hier wird nur noch aus dem Repo-Verzeichnis gelesen.

```bash
RUN_DIR="monitoring/run_<LABEL>"
mkdir -p "$RUN_DIR"
cp -a "$TMP_DIR"/* "$RUN_DIR"/
```

Anschliessend pruefen ob die Kopie vollstaendig ist:

```bash
echo "tmp:  $(ls "$TMP_DIR" | wc -l) Dateien, $(du -sh "$TMP_DIR" | cut -f1)"
echo "repo: $(ls "$RUN_DIR" | wc -l) Dateien, $(du -sh "$RUN_DIR" | cut -f1)"
```

Falls Trace-Logs root gehoeren (vom sudo-Sidecar), Ownership korrigieren:

```bash
sudo chown -R $USER "$RUN_DIR"/trace_*.log "$RUN_DIR"/dmesg_*.log 2>/dev/null
```

### 2.2 Run-Verzeichnis pruefen

```
Glob: <RUN_DIR>/*.csv
Glob: <RUN_DIR>/trace_*.log
```

Pruefen welche Dateien vorhanden sind. Erwartete Dateien:

- **Immer:** cpu.csv, mem.csv, io.csv, vram.csv, vmstat.csv, psi.csv, irq.csv, proc.csv, freq.csv
- **Wenn --xplane:** xplane_telemetry.csv (FPS, CPU/GPU Time, Position, 5 Hz), xplane_telemetry.log
- **Wenn X-Plane lief:** xplane_events.csv
- **Wenn Sidecar lief:** trace_reclaim.log, trace_io_slow.log, trace_fence.log
- **Application Logs (Phase 2.3):** xearthlayer.log, xplane_log.txt
- **Crash-Diagnostik:** dmesg_pre.log, dmesg_post.log, gpu_events.log

Fehlende Dateien notieren aber nicht als Fehler werten.

### 2.3 Application Logs einsammeln und ins Run-Verzeichnis kopieren

Die Application Logs liegen ausserhalb von `/tmp` und muessen aktiv eingesammelt werden, damit das Run-Verzeichnis alle Daten fuer die Analyse enthaelt.

**XEarthLayer-Log:**

```bash
# XEL-Log kopieren (enthaelt moeglicherweise mehrere Sessions — wird spaeter zeitlich gefiltert)
cp ~/.xearthlayer/xearthlayer.log "$RUN_DIR"/xearthlayer.log 2>/dev/null && \
  echo "XEL-Log: $(wc -l < "$RUN_DIR"/xearthlayer.log) Zeilen kopiert" || \
  echo "WARNUNG: Kein XEL-Log gefunden"
```

**X-Plane Log — die RICHTIGE Log finden:**

1. Pruefe `~/X-Plane-12/Log.txt` — ist der Zeitstempel innerhalb des Monitoring-Fensters?
2. Falls Log.txt neuer als die CSV-Dateien (andere Session): Suche in `~/X-Plane-12/Output/Log Archive/` nach der passenden archivierten Log
3. Fallback-Pfade: `~/X-Plane-12-Native/Log.txt`, `~/.local/share/X-Plane-12/Log.txt`

```bash
# X-Plane Log kopieren (Pfad je nach Ergebnis der Suche oben anpassen)
XPLANE_LOG="$HOME/X-Plane-12/Log.txt"
cp "$XPLANE_LOG" "$RUN_DIR"/xplane_log.txt 2>/dev/null && \
  echo "X-Plane Log: $(wc -l < "$RUN_DIR"/xplane_log.txt) Zeilen kopiert" || \
  echo "WARNUNG: Kein X-Plane Log gefunden (pruefe Log Archive)"
```

**Hinweis:** `xplane_events.csv` (von sysmon.py erzeugt) ist bereits im Run-Verzeichnis. Die vollstaendige X-Plane Log liefert aber zusaetzlichen Kontext (Vulkan-Errors, Plugin-Meldungen), der in `xplane_events.csv` nicht enthalten ist.

### 2.4 Session-Metadaten bestimmen

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

### 3.4 Parallele Subagents fuer grosse Datenmengen

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

### 3.5 Quality Checklist

Gemaess ANALYSIS_RULES.txt Section 7 vor Abschluss pruefen:

- [ ] Alle CSV-Dateien gelesen oder als fehlend notiert
- [ ] xplane_telemetry.csv ausgewertet: FPS-Drops identifiziert, CPU/GPU-Bottleneck bestimmt
- [ ] Drei-Phasen-Grenzen identifiziert mit Minutenangaben
- [ ] Jeder allocstall > 0 Event hat eine wahrscheinliche Ursache
- [ ] Cross-Korrelation durchgefuehrt (Stalls ↔ IO ↔ Swap ↔ GPU ↔ XEL)
- [ ] XEL-Log geparst und korreliert
- [ ] Flight Phases aus XEL dem Drei-Phasen-Modell zugeordnet
- [ ] Circuit Breaker Events mit Resource-Pool-Auslastung abgeglichen (post-PR #61: sollte selten sein)
- [ ] Trace-Logs geparst und integriert (falls vorhanden)
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

Run-Daten liegen im Repository (seit Phase 2.1 von `/tmp` kopiert nach `monitoring/run_<LABEL>/`). Pruefen ob alle erwarteten Dateien vorhanden und nicht leer sind.

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

- **sysmon.py laeuft als User:** Kein sudo noetig. Wird direkt gestartet.
- **sysmon_trace.sh braucht sudo:** Wird NICHT automatisch gestartet. Der Befehl wird dem User angezeigt.
- **Application Logs werden in Phase 2.3 kopiert:** XEL-Log und X-Plane Log werden ins Run-Verzeichnis kopiert, damit alle Daten beisammen sind. XEL-Log enthaelt moeglicherweise mehrere Sessions — nur Events innerhalb des Monitoring-Zeitfensters auswerten.
- **X-Plane Log Archive:** Falls `Log.txt` neuer als die CSV-Dateien ist (andere Session), die archivierte Log aus `~/X-Plane-12/Output/Log Archive/` verwenden und stattdessen diese kopieren.
- **Grosse CSV-Dateien:** Nicht komplett in den Kontext laden. Gezielt Zeitfenster und Schluessel-Spalten lesen.
- **ANALYSIS_RULES.txt ist bindend:** Alle Schwellwerte, Korrelationsketten und Known Signatures aus diesem Dokument verwenden. Keine eigenen Schwellwerte erfinden.
- **Vergleich mit frueheren Runs:** `monitoring/ANALYSE_HISTORY.md` und vorhandene `ANALYSE_RUN_*.md` laden fuer Kontext.
- **Zwei-Phasen-Speicherung:** Waehrend des Flugs schreiben alle Tools nach `/tmp` (tmpfs, RAM-backed, kein Disk-I/O). Erst nach dem Flug (Phase 2.1) werden die Daten ins Repo kopiert (`monitoring/run_<label>/`). Das minimiert Systembelastung waehrend der Messung.
- **/tmp RAM-Verbrauch:** ~70-100 MB fuer 3h Session (8 Cores), ~120-150 MB bei 16 Cores. Vernachlaessigbar gegenueber X-Plane (16-32 GB).
- **Watchdog:** Alle 20 Min pruefen ob sysmon.py + bpftrace noch laufen. Bei Ausfall: sysmon neu starten, User fuer bpftrace informieren.
