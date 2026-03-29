# Monitoring Scripts — Refactoring Plan

Analyse der drei Python-Skripte: `sysmon.py`, `xplane_telemetry.py`, `cgwatcher.py`.

---

## Prioritaeten

| Prio | Bedeutung |
|------|-----------|
| P0 | Datenverlust oder falsche Messergebnisse — sofort fixen |
| P1 | Produktionsrisiko (Crashes, Ressourcen-Leaks) — naechster Sprint |
| P2 | Wartbarkeit und Testbarkeit — mittelfristig |
| P3 | Nice-to-have (Style, Ergonomie) — bei Gelegenheit |

---

## sysmon.py (1573 Zeilen)

### P0 — Falsche Messergebnisse

#### 1. Process-CPU% ist ueberhoeht
**Problem:** CPU-Berechnung dividiert durch `slow_dt` statt `slow_dt * NUM_CPUS`. Ein Single-Threaded-Prozess auf 16 Cores wird als 600% statt 37% angezeigt.
```python
# IST:
cpu_pct = (d_u + d_s) / CLK_TCK / slow_dt * 100
# SOLL:
cpu_pct = (d_u + d_s) / CLK_TCK / slow_dt * 100  # pro-Core %, bewusste Entscheidung dokumentieren
# ODER:
cpu_pct = (d_u + d_s) / CLK_TCK / (slow_dt * num_cpus) * 100  # system-normalisiert
```
**Warum:** Alle bisherigen Analysen (ANALYSE_RUN_*.md) basieren auf diesen Werten. Wenn bewusst pro-Core-% gemeint ist, als Kommentar dokumentieren. Falls nicht: korrigieren und in ANALYSE_HISTORY.md vermerken.

#### 2. Disk-IO-Latenz vermengt Queue Depth mit Service Time
**Problem:** `avg_r_lat_ms = time_ms / ops` misst nicht die Service-Latenz pro Operation, sondern die kumulierte Zeit aller parallelen Requests.
**Warum:** Bei hoher NVMe-Queue-Depth (z.B. 32 parallel) wird Latenz um Faktor 32 ueberschaetzt. Die "10-11ms NVMe-Latenz-Cluster" in frueheren Analysen koennten ein Artefakt sein.
**Fix:** Kommentar ergaenzen, dass es sich um `total_io_time / ops` handelt (= "average service time including queue wait"), nicht um reine Device-Latenz.

#### 3. Memory-Berechnung weicht vom Kernel ab
**Problem:** `used = total - free - buffers - cached` ignoriert reclaimable Slab und andere Kernel-Heuristiken.
**Fix:** `MemAvailable` aus `/proc/meminfo` direkt als Spalte in mem.csv aufnehmen (ist bereits gelesen aber nicht geschrieben).

### P1 — Produktionsrisiken

#### 4. Subprocess-Leaks bei Crash/Signal
**Problem:** Bei Exception in `collect()` laufen `xplane_telemetry.py` und `gpu_events`-journalctl als Zombies weiter. Signal-Handler raised `KeyboardInterrupt`, aber der Cleanup-Code fuer Subprocesses wird nicht erreicht.
**Fix:** `try/finally` Block um die gesamte `collect()`-Ausfuehrung + Subprocess-Lifecycle. Signal-Handler setzt Flag statt Exception zu raisen.

```python
_shutdown_requested = False

def _shutdown(signum, frame):
    global _shutdown_requested
    _shutdown_requested = True

# In collect():
while not _shutdown_requested and time.time() - start < DURATION:
    ...
```

#### 5. GPU-Backend Zustandsmaschine fehlt
**Problem:** Nach 3 NVML-Fehlern Fallback auf nvidia-smi. Aber kein Weg zurueck und keine Recovery bei transientem GPU-Treiber-Glitch. `get_vram()` gibt leeren String zurueck → malformed CSV-Zeilen.
**Fix:** Enum-basierte State Machine (`NVML → NvidiaSmi → Disabled`) mit periodischem Recovery-Versuch. Leere GPU-Daten als explizite Null-Zeile statt leerem String.

#### 6. Stats-Objekt waechst unbegrenzt
**Problem:** `Stats` sammelt alle Samples in Listen im RAM. Bei 3h Session (200ms Intervall): 54.000 Eintraege × ~50 Felder. Kein Problem fuer Speicher (~50 MB), aber die Summary-Berechnung iteriert ueber alles.
**Fix:** Inkrementelle Statistik (Welford-Algorithmus fuer Mean/Stddev) statt alle Samples zu halten. Oder: Nur die letzten N Samples fuer Percentile behalten.

#### 7. CSV-Corruption bei Prozessnamen mit Kommas
**Problem:** `write_proc()` schreibt Prozessnamen direkt in CSV ohne Quoting. Prozess `nginx: worker, idle` bricht die Spaltenstruktur.
**Fix:** `csv.writer` mit `quoting=csv.QUOTE_MINIMAL` verwenden statt f-String-basiertem Schreiben. Oder: Kommas in Prozessnamen durch Underscore ersetzen.

### P2 — Wartbarkeit

#### 8. Globale Variablen eliminieren
**Problem:** 10+ Module-Level Globals (`DURATION`, `INTERVAL`, `OUTDIR`, `_USE_NVML`, etc.) werden in `main()` mutiert und in `collect()` gelesen. Unit-Tests unmoeglich.
**Fix:** `@dataclass MonitorConfig` mit allen Parametern. An `collect()`, `CSVWriters`, `init_gpu()` als Argument uebergeben.

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

#### 9. collect()-Loop aufbrechen (1000+ Zeilen)
**Problem:** Eine Funktion kombiniert: Sampling, Aggregation, CSV-Schreiben, Prozess-Tracking, Slow/Fast-Interval-Logik.
**Fix:** Extrahieren in Collector-Klassen pro Subsystem:
- `CpuCollector.sample()` → CPU-Daten
- `DiskCollector.sample()` → Disk-IO
- `MemoryCollector.sample()` → RAM/Swap
- `GpuCollector.sample()` → VRAM
- `ProcessCollector.sample()` → Per-Process
- Haupt-Loop ruft nur `collector.sample(ts)` auf

#### 10. Type Hints und Docstrings
**Problem:** Keine einzige Funktion hat Type-Annotations. Return-Typen sind verschachtelte Dicts (`defaultdict(lambda: defaultdict(list))`).
**Fix:** `TypedDict` oder `@dataclass` fuer alle Rueckgabewerte. Type Hints fuer alle oeffentlichen Funktionen. Prioritaet: `collect()`, `CSVWriters`, `Stats`.

---

## xplane_telemetry.py (264 Zeilen)

### P0 — Falsche Messergebnisse

#### 11. NaN wird still zu 0.0
**Problem:** `if val != val: val = 0.0` — NaN-Datarefs werden als 0 geschrieben. In der Analyse nicht von echten Nullwerten unterscheidbar (z.B. GPU-Time = 0 vs. GPU-Time = nicht verfuegbar).
**Fix:** NaN als leere Zelle schreiben (`""`) oder als `NaN`-String. Anzahl NaN-Events loggen.

#### 12. CPU-Time-Ableitung fragil
**Problem:** `cpu_time = max(0, frame_time - gpu_time)`. Wenn GPU-Time > Frame-Time (Float-Praezision, GPU-Treiber-Bug), wird CPU-Time auf 0 geklemmt ohne Warnung.
**Fix:** Warnung loggen wenn `gpu_time > frame_time * 1.01`. Eigenen Dataref `sim/operation/misc/cpu_time_sec` abonnieren falls verfuegbar.

### P1 — Produktionsrisiken

#### 13. Endlos-Retry bei X-Plane-Verbindung
**Problem:** `subscribe(wait=True)` hat keinen Timeout. Wenn X-Plane nie startet, haengt der Subprocess ewig. sysmon.py hat keinen Mechanismus das zu erkennen.
**Fix:** Maximale Retry-Anzahl (z.B. 60 = 5 Min) einfuehren. Danach: Exit mit Fehlercode, den sysmon.py auswerten kann.

#### 14. Resubscribe-Sturm nach Timeouts
**Problem:** Nach 3 Timeouts wird `resubscribe()` aufgerufen. Wenn X-Plane ueberlastet ist, verschlimmert das die Situation.
**Fix:** Exponentielles Backoff fuer Resubscribe-Intervall. Minimum 10s zwischen Resubscribes.

#### 15. Socket-Leak bei abnormalem Exit
**Problem:** `XPlaneUDP.close()` existiert, wird aber nicht in allen Exit-Pfaden aufgerufen.
**Fix:** `XPlaneUDP` als Context Manager (`__enter__`/`__exit__`) implementieren.

### P2 — Wartbarkeit

#### 16. Dataref-Liste hardcoded
**Problem:** 13 Datarefs fest im Code. Weitere hinzufuegen erfordert Code-Aenderung + Index-Verwaltung.
**Fix:** Datarefs aus Config-Datei oder CLI-Argument laden. Index automatisch vergeben.

#### 17. Magic Numbers dokumentieren
**Problem:** `1.94384` (m/s→knots), `3.28084` (m→ft), `0.001` (FPS-Threshold) undokumentiert.
**Fix:** Benannte Konstanten: `MPS_TO_KNOTS = 1.94384`, `M_TO_FT = 3.28084`.

---

## cgwatcher.py (335 Zeilen)

### P1 — Produktionsrisiken

#### 18. os.fork() durch systemd-Service ersetzen
**Problem:** `os.fork()` ist fragil in Python (Thread-Safety, File-Descriptor-Vererbung, kein Cleanup). PID-Datei wird nie geloescht. Kein Schutz gegen Doppelstart.
**Fix:** Statt `--daemon` mit `os.fork()`: als `systemd --user` Service laufen lassen. Die `.service`-Unit existiert bereits im Konzept (cgroup-Slices). PID-Management, Logging, Restart-Policy alles von systemd.

```ini
# cgwatcher.service
[Service]
ExecStart=/usr/bin/python3 /path/to/cgwatcher.py
Restart=on-failure
```

#### 19. Prozessname-Matching zu breit
**Problem:** Substring-Match: Pattern `sim` trifft auch `similar`, `simulator-helper`, `simple-scan`.
**Fix:** Optionales Regex-Matching (`~pattern` Syntax in Config). Oder: Wort-Grenze mit `\b` standardmaessig.

#### 20. Stale PID-File verhindert Neustart
**Problem:** PID-File in `/tmp/cgwatcher.pid` wird geschrieben aber nie geloescht. Nach Crash bleibt die Datei.
**Fix:** Beim Start pruefen ob PID noch lebt (`os.kill(pid, 0)`). Lock-File mit `fcntl.flock()` statt PID-File.

#### 21. Scheduler-Erkennung unsicher
**Problem:** Liest `/boot/config-$(uname -r)` — existiert nicht auf allen Distros (z.B. Arch ohne linux-headers). Fallback auf `/proc/config.gz` braucht `CONFIG_IKCONFIG_PROC=y`. Bei Fehlschlag: stille Annahme "cfs".
**Fix:** Warnung loggen wenn Scheduler nicht sicher erkannt. Zusaetzlich: `/sys/kernel/debug/sched/features` pruefen (CFS-spezifisch).

### P2 — Wartbarkeit

#### 22. Hardcoded Pfade extrahieren
**Problem:** `/tmp/cgwatcher.log`, `/tmp/cgwatcher.pid`, Config-Pfad, Cgroup-Basispfad alles hardcoded.
**Fix:** CLI-Argumente fuer alle Pfade. Defaults beibehalten fuer Kompatibilitaet.

#### 23. Logging Flush im Daemon-Modus
**Problem:** FileHandler ohne explizites Flushing. Bei Crash gehen letzte Log-Zeilen verloren.
**Fix:** `logging.FileHandler` mit `flush` nach jedem `log.info()`, oder `StreamHandler` auf unbuffered File.

---

## Uebergreifende Verbesserungen

### P2 — Alle Skripte

#### 24. Gemeinsame Konfiguration
**Problem:** Jedes Skript hat eigene Konfigurationslogik (Envvars, CLI-Args, Hardcodes). Keine gemeinsame Config-Datei.
**Vorschlag:** `monitoring/config.toml` mit Sektionen `[sysmon]`, `[telemetry]`, `[cgwatcher]`. Alle Skripte lesen daraus, CLI-Args ueberschreiben.

#### 25. Einheitliche Fehlerbehandlung
**Problem:** Fehler werden per `print(f"[ERROR]...", flush=True)` ausgegeben (sysmon), per `logging` (cgwatcher), oder per `print()` (xplane_telemetry).
**Fix:** Alle Skripte auf `logging` umstellen. Log-Level per CLI steuerbar.

#### 26. Type Hints und Docstrings
**Problem:** Kein einziges Skript hat Type Hints.
**Fix:** Schrittweise einfuehren, Prioritaet auf oeffentliche APIs und Rueckgabewerte.

### P3 — Nice-to-have

#### 27. Testbarkeit durch Dependency Injection
Alle /proc- und /sys-Zugriffe hinter Interfaces kapseln. Ermoeglicht Mock-basierte Unit-Tests ohne Root-Rechte.

#### 28. CSV-Schreiben ueber csv.writer statt f-Strings
Verhindert Encoding-Probleme, Komma-in-Werten, und NaN/Inf-Corruption. Betrifft alle `write_*()` Methoden in sysmon.py.

---

## Empfohlene Reihenfolge

| Schritt | Tickets | Aufwand |
|---------|---------|---------|
| 1 | P0: #1 CPU%, #2 IO-Latenz, #3 MemAvailable, #11 NaN, #12 CPU-Time | Klein (Kommentare + 1-Zeiler Fixes) |
| 2 | P1: #4 Subprocess-Cleanup, #7 CSV-Quoting, #13 Retry-Limit | Mittel (Signal-Handling Refactor) |
| 3 | P1: #5 GPU-State-Machine, #14 Resubscribe-Backoff, #18 systemd statt fork | Mittel |
| 4 | P2: #8 Config-Dataclass, #9 Collector-Klassen | Gross (Architektur-Refactor) |
| 5 | P2: #10 Type Hints, #24 Gemeinsame Config | Mittel |
