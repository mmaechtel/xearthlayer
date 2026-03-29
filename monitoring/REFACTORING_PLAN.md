# Refactoring — Umsetzungsplan

8 offene Punkte aus REFACTORING.md, gruppiert in 4 Arbeitspakete mit Testzyklus.

---

## Arbeitspaket A — Signal-Handling & Subprocess-Lifecycle (sysmon.py)

**Punkte:** #4 (Subprocess-Cleanup), #6 (Config-Dataclass)
**Aufwand:** ~50 Min
**Risiko:** Hoch — aendert die zentrale Kontrollfluss-Logik

### A1. Signal-Handler: Flag statt Exception (#4)

```python
# NEU: Modul-Level
_shutdown_requested = False

def _request_shutdown(signum, frame):
    global _shutdown_requested
    _shutdown_requested = True
    print(f"\nReceived {signal.Signals(signum).name}, finishing current sample...")
```

In `main()`:
```python
signal.signal(signal.SIGTERM, _request_shutdown)
signal.signal(signal.SIGINT, _request_shutdown)
```

### A2. collect() prueft Flag statt KeyboardInterrupt

```python
def collect(writers, stats, config):
    ...
    try:
        while not _shutdown_requested and time.time() - start < config.duration:
            time.sleep(config.interval)
            ...
    except KeyboardInterrupt:
        pass  # Fallback falls Signal-Handler nicht greift
    ...
```

### A3. Subprocess-Cleanup in try/finally

In `main()` den gesamten Block nach Subprocess-Start in try/finally wrappen:

```python
try:
    with CSVWriters(OUTDIR) as writers:
        mon_start, elapsed, sample_count, irq_sample_count = \
            collect(writers, stats, config)
finally:
    # Immer aufraeumen, auch bei Exception/Signal
    if xplane_telem_proc and xplane_telem_proc.poll() is None:
        xplane_telem_proc.terminate()
        try:
            xplane_telem_proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            xplane_telem_proc.kill()

    if gpu_mon_proc and gpu_mon_proc.poll() is None:
        gpu_mon_proc.terminate()
        try:
            gpu_mon_proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            gpu_mon_proc.kill()
```

### A4. Config-Dataclass (#6)

```python
@dataclass(frozen=True)
class MonitorConfig:
    duration: int
    interval: float
    outdir: Path
    proc_patterns: list[str]
    xplane_log: Path | None
    disable_gpu: bool
```

Globale `DURATION`, `INTERVAL`, `OUTDIR`, `PROC_PATTERNS`, `XPLANE_LOG` durch
`config`-Parameter in `collect()` ersetzen. GPU-Globals bleiben (Hot-Path).

### Testzyklus A

```bash
# T-A1: Normaler Start + Ctrl+C nach 5 Sekunden
python3 monitoring/sysmon.py -d 60 -o /tmp/test_a1
# → Nach Ctrl+C: Pruefen dass KEINE Zombie-Prozesse laufen:
ps aux | grep -E 'sysmon|xplane_telemetry|journalctl' | grep -v grep
# Erwartung: Leer

# T-A2: Start mit --xplane + Ctrl+C (ohne laufendes X-Plane)
python3 monitoring/sysmon.py -d 60 --xplane -o /tmp/test_a2
# → Ctrl+C nach 5s. xplane_telemetry.py muss gestoppt werden.
ps aux | grep xplane_telemetry | grep -v grep
# Erwartung: Leer

# T-A3: SIGTERM statt Ctrl+C
python3 monitoring/sysmon.py -d 60 -o /tmp/test_a3 &
PID=$!; sleep 3; kill $PID; wait $PID
# → Pruefe Exit-Code und dass CSVs geschrieben wurden:
ls -la /tmp/test_a3/*.csv | wc -l
# Erwartung: 9 CSV-Dateien, alle > 0 Bytes

# T-A4: Timer laeuft normal ab (kein Signal)
python3 monitoring/sysmon.py -d 5 -o /tmp/test_a4
# → Laeuft 5 Sekunden, beendet sich sauber
ls -la /tmp/test_a4/*.csv
# Erwartung: 9 CSVs, cpu.csv hat ~25 Zeilen (5s / 0.2s)

# T-A5: Config-Dataclass — verify collect() nutzt config statt Globals
grep -n 'DURATION\|INTERVAL\|OUTDIR' monitoring/sysmon.py | grep -v 'config\.\|#\|def \|class '
# Erwartung: Keine Treffer mehr in collect() (nur noch in main() beim Parsen)
```

---

## Arbeitspaket B — xplane_telemetry.py Haertung

**Punkte:** #7 (NaN-Count), #8 (Retry-Limit)
**Aufwand:** ~20 Min
**Risiko:** Niedrig — isoliertes Skript

### B1. Retry-Limit bei subscribe() (#8)

```python
MAX_CONNECT_RETRIES = 60  # ~5 Minuten mit Backoff

def subscribe(self, freq, wait=True):
    ...
    attempt = 0
    while True:
        attempt += 1
        self._send_subscriptions(freq)
        try:
            self._recv_one()
            self.connected = True
            return True
        except socket.timeout:
            if not wait:
                return False
            if attempt >= MAX_CONNECT_RETRIES:
                print(f"  Giving up after {attempt} attempts (~5 min).",
                      flush=True)
                return False
            backoff = min(5, 1 + attempt * 0.5)
            print(f"  No response (attempt {attempt}/{MAX_CONNECT_RETRIES}), "
                  f"retrying in {backoff:.0f}s...", flush=True)
            time.sleep(backoff)
```

In `main()` den Return-Wert pruefen:
```python
if not xp.subscribe(args.rate, wait=True):
    print("ERROR: Could not connect to X-Plane. Exiting.", file=sys.stderr)
    xp.close()
    sys.exit(2)
```

### B2. NaN-Counter (#7)

```python
# In _recv_one():
self._nan_count = getattr(self, '_nan_count', 0)
for i in range(n):
    idx, val = struct.unpack("<if", payload[i*8:(i+1)*8])
    if val != val:  # NaN
        val = 0.0
        self._nan_count += 1
    self.values[idx] = val
```

Am Session-Ende in `main()`:
```python
if xp._nan_count > 0:
    print(f"  Warning: {xp._nan_count} NaN values replaced with 0.0")
```

### Testzyklus B

```bash
# T-B1: Start OHNE X-Plane — muss nach ~5 Min mit Exit-Code 2 beenden
timeout 360 python3 monitoring/xplane_telemetry.py -d 10 -o /tmp/test_b1
echo "Exit code: $?"
# Erwartung: Exit-Code 2, Laufzeit ~5 Min (nicht ewig)

# T-B2: Start MIT X-Plane — verbindet und schreibt Daten
python3 monitoring/xplane_telemetry.py -d 10 -r 5 -o /tmp/test_b2
# Erwartung: xplane_telemetry.csv mit ~50 Zeilen (10s * 5Hz)
wc -l /tmp/test_b2/xplane_telemetry.csv

# T-B3: NaN-Count wird am Ende ausgegeben (nur visuell pruefen)
# → Im normalen Betrieb: "0 NaN" oder keine Meldung
# → Kann nicht einfach provoziert werden ohne X-Plane-Mock

# T-B4: sysmon.py --xplane ohne X-Plane — Subprocess beendet sich sauber
python3 monitoring/sysmon.py -d 10 --xplane -o /tmp/test_b4
# → sysmon laeuft 10s, xplane_telemetry beendet sich nach Retry-Limit
ps aux | grep xplane_telemetry | grep -v grep
# Erwartung: Leer
```

---

## Arbeitspaket C — CSV/Doku Korrekturen (sysmon.py)

**Punkte:** #1 (CPU%-Kommentar), #2 (IO-Spalte umbenennen), #5 (Komma-Escaping)
**Aufwand:** ~12 Min
**Risiko:** Niedrig, aber #2 bricht bestehende Analyse-Skripte die auf Spaltennamen zugreifen

### C1. CPU%-Kommentar (#1)

In `collect()` bei der CPU-Berechnung:
```python
# CPU% is per-core (like top/htop): 800% = 8 cores fully used.
# This is the Linux convention — do NOT divide by NUM_CPUS.
cpu_pct = (d_u + d_s) / CLK_TCK / slow_dt * 100
```

### C2. IO-Spalte umbenennen (#2)

In `CSVWriters.__init__()`:
```python
# Vorher:
"avg_r_lat_ms,avg_w_lat_ms"
# Nachher:
"r_await_ms,w_await_ms"
```

Analog in `write_io()` und FEATURES.md aktualisieren.

**ACHTUNG:** Bestehende ANALYSIS_RULES.txt und Analyse-Skripte referenzieren
`avg_r_lat_ms`. Muessen ebenfalls angepasst werden, sonst brechen kuenftige
Analysen. Aeltere Run-Daten (run_AA etc.) behalten die alten Spaltennamen.

**TODO (C2-Abhängigkeit):** Bei Umsetzung von C2 MUSS ANALYSIS_RULES.txt
Zeile ~277 (`avg_r_lat_ms` / `avg_w_lat_ms`) auf `r_await_ms` / `w_await_ms`
umbenannt werden. Gleichzeitig mit der CSV-Header-Änderung durchführen!

### C3. Komma-Escaping in Prozessnamen (#5)

In `write_proc()`:
```python
# Vorher:
f"{ts:.3f},{pid},{name},{cpu_pct:.1f},..."
# Nachher:
safe_name = name.replace(",", "_")
f"{ts:.3f},{pid},{safe_name},{cpu_pct:.1f},..."
```

### Testzyklus C

```bash
# T-C1: IO-Spaltenname in CSV pruefen
python3 monitoring/sysmon.py -d 3 -o /tmp/test_c1
head -1 /tmp/test_c1/io.csv
# Erwartung: "timestamp,device,r_per_s,w_per_s,rMB_per_s,wMB_per_s,r_await_ms,w_await_ms,io_util_pct,ios_in_progress"

# T-C2: Proc-CSV mit normalem Prozess pruefen
head -3 /tmp/test_c1/proc.csv
# Erwartung: Keine Kommas in Prozessnamen

# T-C3: FEATURES.md stimmt mit CSV-Headern ueberein
for csv in /tmp/test_c1/*.csv; do
    echo "=== $(basename $csv) ==="; head -1 "$csv"
done
# → Manuell gegen FEATURES.md abgleichen

# T-C4: ANALYSIS_RULES.txt referenziert nicht mehr avg_r_lat_ms
grep -c 'avg_r_lat_ms\|avg_w_lat_ms' monitoring/ANALYSIS_RULES.txt
# Erwartung: 0 (nach Anpassung)
```

---

## Arbeitspaket D — cgwatcher.py Cleanup

**Punkte:** #10 (--daemon entfernen), #11 (Scheduler-Warning)
**Aufwand:** ~22 Min
**Risiko:** Mittel — aendert CLI-Interface (Breaking Change fuer --daemon User)

### D1. --daemon entfernen (#10)

- `os.fork()`-Block komplett entfernen
- `PID_PATH` und `LOG_PATH` Konstanten entfernen
- `setup_logging(to_file=...)` vereinfachen (immer stdout)
- `--daemon` aus `sys.argv`-Parsing entfernen
- FEATURES.md Modes-Tabelle anpassen: nur `foreground` und `--once`
- Ergaenzen in FEATURES.md: systemd-Unit-Beispiel fuer Daemon-Betrieb

### D2. Scheduler-Warning (#11)

Am Ende von `detect_scheduler()`:
```python
# Vorher:
return "cfs"
# Nachher:
log.warning("Scheduler detection failed (no /boot/config-*, no /proc/config.gz). "
            "Assuming CFS — nice values will NOT be used.")
return "cfs"
```

### Testzyklus D

```bash
# T-D1: --daemon ist nicht mehr akzeptiert
python3 monitoring/cgroups/cgwatcher.py --daemon 2>&1
# Erwartung: Fehler oder ignoriert (je nach Implementierung)

# T-D2: --once funktioniert weiterhin
python3 monitoring/cgroups/cgwatcher.py --once
# Erwartung: Scannt einmal, zeigt Ergebnis, beendet sich

# T-D3: Foreground-Modus + Ctrl+C
timeout 5 python3 monitoring/cgroups/cgwatcher.py
# Erwartung: Laeuft 5s, wird sauber beendet

# T-D4: Scheduler-Warning (nur auf Systemen ohne /boot/config-*)
# → Manuell pruefen: Wenn /boot/config-$(uname -r) existiert, keine Warning.
#   Zum Testen: temporaer umbenennen und pruefen ob Warning erscheint.

# T-D5: Kein PID-File mehr
ls /tmp/cgwatcher.pid 2>&1
# Erwartung: "No such file" (wird nicht mehr angelegt)
```

---

## Reihenfolge und Abhaengigkeiten

```
  A (Signal + Config)     Keine Abhaengigkeit
  │
  B (Telemetry)           Keine Abhaengigkeit, kann parallel zu A
  │
  C (CSV/Doku)            Nach A (nutzt config-Objekt aus A4)
  │
  D (cgwatcher)           Keine Abhaengigkeit, kann parallel
```

**Empfohlene Reihenfolge:**

| Schritt | Paket | Grund |
|---------|-------|-------|
| 1 | **A** | Hoechstes Risiko, aendert Kernlogik. Frueh testen. |
| 2 | **B** | Unabhaengig, kann sofort nach A. |
| 3 | **C** | Baut auf A4 (config) auf. IO-Spaltenumbenennung braucht Doku-Updates. |
| 4 | **D** | Breaking Change (--daemon). Am Ende, separat testbar. |

**Commits:** Ein Commit pro Arbeitspaket. Testzyklus nach jedem Commit.
