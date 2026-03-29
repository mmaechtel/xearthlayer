# Monitoring Scripts — Refactoring Plan

Analyse der drei Python-Skripte: `sysmon.py`, `xplane_telemetry.py`, `cgwatcher.py`.

Pragmatisch bewertet: Monitoring-Skripte die von einem User auf einem System laufen
brauchen kein Framework. Nur Aenderungen die echte Bugs fixen oder die Analyse
verbessern.

---

## sysmon.py

### 1. Kommentar bei CPU%-Berechnung
**Was:** CPU% zeigt pro-Core-Prozent (wie top/htop). 8-Thread-Prozess = 800%.
Das ist kein Bug, sondern Linux-Konvention — aber undokumentiert im Code.
**Fix:** Kommentar an der Berechnungsstelle. 2 Min.

### 2. IO-Latenz-Spalte umbenennen
**Was:** `avg_r_lat_ms` suggeriert Device-Latenz, ist aber `total_io_time / ops`
(inkl. Queue Wait). Selbe Semantik wie iostat `await` — aber der Spaltenname
ist irrefuehrend.
**Fix:** Umbenennen → `svc_time_ms` oder `await_ms`. 5 Min.

### ~~3. MemAvailable in mem.csv aufnehmen~~ (ENTFAELLT)
Spalte `available_mb` existiert bereits als 4. Spalte in mem.csv.
Refactoring-Vorschlag war falsch — beim Review des Codes uebersehen.

### 4. Subprocess-Cleanup bei Signal (echter Bug)
**Was:** Ctrl+C in sysmon.py → Signal-Handler raised KeyboardInterrupt →
`xplane_telemetry.py` und `gpu_events`-journalctl laufen als Zombies weiter.
Naechster Start schlaegt fehl weil UDP-Port belegt.
**Fix:** Signal-Handler setzt `_shutdown_requested` Flag statt Exception.
`collect()`-Loop prueft Flag. `try/finally` fuer Subprocess-Termination.

```python
_shutdown_requested = False

def _shutdown(signum, frame):
    global _shutdown_requested
    _shutdown_requested = True

# In collect():
while not _shutdown_requested and time.time() - start < DURATION:
    ...
```
30 Min.

### 5. Komma in Prozessnamen escapen
**Was:** `write_proc()` schreibt Namen ohne Quoting. Theoretisch bricht ein
Prozessname mit Komma die CSV-Struktur. Praktisch betrifft es keine der
ueberwachten Prozesse — aber der Fix ist trivial.
**Fix:** Kommas in Prozessnamen durch Underscore ersetzen. 5 Min, bei Gelegenheit.

### 6. Config-Dataclass fuer CLI-Parameter
**Was:** 6 CLI-Parameter (`DURATION`, `INTERVAL`, `OUTDIR`, etc.) sind globale
Variablen die in `main()` mutiert und in `collect()` gelesen werden.
**Fix:** `@dataclass MonitorConfig` mit den 6 Feldern. An `collect()` uebergeben.
GPU-Globals (`_NVML_HANDLE` etc.) bleiben global — die stecken im Hot-Path.

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
20 Min.

### Bewusst NICHT machen

- **collect() in Collector-Klassen aufbrechen:** 1000 Zeilen klingt viel, ist
  aber ~200 Zeilen Logik + ~800 Zeilen identische try/except-Bloecke. 8 Klassen
  mit Interface + Registry erzeugen mehr Code, nicht weniger. Die lineare Struktur
  ist fuer ein Single-User-Skript lesbarer als ein Framework.
- **GPU State Machine:** NVML funktioniert oder nicht. Fallback auf nvidia-smi
  existiert. "Transiente GPU-Glitches mit Recovery" passieren in der Praxis nicht.
- **Stats-Objekt begrenzen (Welford):** ~50 MB fuer 3h Session bei 64 GB RAM.
  Premature Optimization.
- **Type Hints:** Skript wird nicht als Library importiert. Funktionen sind
  self-contained, Typen offensichtlich. Bei Gelegenheit, nicht aktiv.

---

## xplane_telemetry.py

### 7. NaN-Count loggen
**Was:** NaN-Datarefs werden still zu `0.0`. In der Praxis passiert das nur wenn
ein Dataref nicht existiert (alte X-Plane-Version), nie waehrend eines Flugs.
`0.0` als FPS faellt in der Analyse sofort auf. Leere Zellen brechen CSV-Parser.
**Fix:** `0.0` beibehalten, aber NaN-Gesamtcount am Session-Ende loggen. 5 Min.

### 8. Retry-Limit fuer X-Plane-Verbindung (echter Bug)
**Was:** `subscribe(wait=True)` retried endlos. Wenn X-Plane nie gestartet wird,
haengt der Subprocess ewig. sysmon.py erkennt das nicht.
**Fix:** Max 60 Retries (~5 Min). Danach Exit mit Fehlercode. 15 Min.

### ~~9. Benannte Konstanten~~ (ERLEDIGT)
`MPS_TO_KNOTS` extrahiert in D2-Fix. `M_TO_FT` kommt im Code nicht vor
(war Phantom aus der initialen Analyse).

### Bewusst NICHT machen

- **Resubscribe-Backoff:** 3 Timeouts = 1 Resubscribe bei Szenerie-Laden.
  Das ist kein "Sturm". X-Plane verarbeitet UDP robust.
- **Socket Context Manager:** Python's GC schliesst Sockets beim Prozess-Exit.
  Aendert nichts am Verhalten.
- **Dataref-Liste aus Config laden:** 11 Datarefs (2 Duplikate entfernt in D2),
  stabil seit X-Plane 12.0. Config-Datei dafuer ist Indirection ohne Nutzen.
- **CPU-Time Ableitung "fragil":** `frame_time - gpu_time` ist die einzige
  Moeglichkeit. `gpu_time > frame_time` kommt in der Praxis nicht vor.

---

## cgwatcher.py

### 10. --daemon entfernen, systemd nutzen
**Was:** `os.fork()` in Python ist fragil (Thread-Safety, FD-Vererbung, kein
Cleanup). PID-File wird nie geloescht. Kein Schutz gegen Doppelstart.
**Fix:** `--daemon` Flag komplett entfernen. Wer Daemon-Modus will, nutzt
systemd: `systemctl --user start cgwatcher`. PID-Management, Logging,
Restart-Policy alles von systemd erledigt. Stale-PID-Problem (#20) entfaellt.
20 Min.

### 11. Warning bei Scheduler-Fallback
**Was:** Wenn weder `/boot/config-*` noch `/proc/config.gz` lesbar sind, wird
still "cfs" angenommen. Auf einem PDS/BMQ-System wuerden dann Cgroup-Weights
statt Nice-Values gesetzt (wirkungslos).
**Fix:** `log.warning("Scheduler detection failed, assuming CFS")`. 2 Min.

### Bewusst NICHT machen

- **Prozessname-Matching Regex:** Patterns sind `X-Plane`, `xearthlayer`,
  `pipewire`. Substring-Match ist korrekt und ausreichend fuer 5 Eintraege
  in einer Single-User-Config.
- **Hardcoded Pfade extrahieren:** Ein User, ein System. CLI-Argumente fuer
  `/tmp/cgwatcher.log` sind Overengineering.
- **Logging Flush:** Wird durch systemd-Umstellung (#10) irrelevant —
  journald flusht automatisch.

---

## Doku-Abweichungen (FEATURES.md / README.md vs. Code) — ALLE ERLEDIGT

### ~~D1. proc.csv: Spalte `swap_mb` fehlt in FEATURES.md~~ ✓
Gefixt: `swap_mb` in FEATURES.md ergaenzt.

### ~~D2. xplane_telemetry.py: stumme Datarefs~~ ✓
Gefixt: `framerate_period_s` und `tas_ms` entfernt (Duplikat/kein Analysewert).
`sim_speed` in CSV aufgenommen (erkennt Zeitraffer). 11 statt 13 Datarefs.

### ~~D3. cgwatcher `--once` fehlt im README~~ ✓
Gefixt: `--once` in README.md erwaehnt.

---

## Zusammenfassung

| # | Aenderung | Skript | Aufwand | Typ |
|---|-----------|--------|---------|-----|
| 1 | Kommentar CPU%-Berechnung | sysmon | 2 Min | Doku |
| 2 | IO-Spalte umbenennen | sysmon | 5 Min | Doku |
| ~~3~~ | ~~MemAvailable Spalte~~ | ~~sysmon~~ | — | Existiert bereits |
| | **D1-D3 erledigt:** swap_mb Doku, sim_speed CSV, --once Doku | | | |
| 4 | Subprocess-Cleanup | sysmon | 30 Min | **Bugfix** |
| 5 | Komma-Escaping proc.csv | sysmon | 5 Min | Robustheit |
| 6 | Config-Dataclass | sysmon | 20 Min | Wartbarkeit |
| 7 | NaN-Count loggen | telemetry | 5 Min | Transparenz |
| 8 | Retry-Limit X-Plane | telemetry | 15 Min | **Bugfix** |
| ~~9~~ | ~~Benannte Konstanten~~ | ~~telemetry~~ | — | Erledigt (D2) |
| 10 | --daemon entfernen | cgwatcher | 20 Min | **Bugfix** |
| 11 | Scheduler-Warning | cgwatcher | 2 Min | Transparenz |
| | **Gesamt** | | **~2h** | |
