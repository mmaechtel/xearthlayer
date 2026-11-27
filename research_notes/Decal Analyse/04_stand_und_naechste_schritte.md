# Decal: Stand und nächste Schritte

Stand 27.11.2026. Der Nutzer hat dem vorgeschlagenen Vorgehen zugestimmt ("okay"); die drei
Entwurfsentscheidungen unten sind damit als Arbeitsannahme festgehalten, nicht als getestet.

## Befund

- Die XEarthLayer-Pakete enthalten kein Decal: keine Decal-Zeile in 218.783 `.ter`-Dateien von
  NA-GREENLAND und in 439.849 von EU-WEST (beide Version 0.5.1, vollständig gelesen). Die übrigen
  15 Regionen sind ungeprüft.
- XEarthLayer müsste ein Decal also hinzufügen. OrthoStreamXP schwächt dagegen ein vorhandenes ab.
- `lib/g10/decals/maquify_2_green_key.dcl` ist nach öffentlichen Kopien eine einzige Zeile
  `DECAL_PARAMS_PROJ 15 15 0.8 ...`: Sträucher, wo im Foto Grün überwiegt, sonst Geröll, Muster
  alle 15 m. Die Datei aus X-Plane 12 selbst wurde nicht gelesen.
- `.ter`-Dateien werden heute unverändert durchgereicht; im Code gibt es keine Stelle zum
  Eingreifen und keine Decal-Behandlung.

## Arbeitsannahmen

1. Die Decal-Zeile wird als fester Anhang ans Dateiende gesetzt, vorbehaltlich des
   Simulatortests.
2. `.ter`-Dateien aus Patches bleiben unangetastet.
3. Zuerst nur die einfache Form (Zeile an oder aus). Die regelbare Form folgt, wenn die echte
   `.dcl` aus einer X-Plane-12-Installation vorliegt.

## Nächste Schritte

1. **Handversuch vor jedem Bau.** In einem installierten Paket die Zeile
   `DECAL_LIB lib/g10/decals/maquify_2_green_key.dcl` ans Ende einiger Land-`.ter`-Dateien
   schreiben und das Gebiet tief überfliegen. Geprüft wird dreierlei:
   - wirkt eine Zeile am Dateiende überhaupt;
   - stören Leerzeilen am Ende;
   - gefällt das Decal auf den eigenen Fotos (Felder, Dächer, Straßen).
   Abbruch: wirkt die Zeile am Ende nicht, entfällt der Anhang-Entwurf; gefällt das Decal in
   voller Stärke nicht, lohnt nur die regelbare Form.
2. **Echte Decal-Dateien sichern.** `library.txt` und `maquify_2_green_key.dcl` aus
   `Resources/default scenery/1000 decals` der eigenen Installation ablegen, samt Format der
   Textur (PNG oder DDS).
3. **Erst danach bauen**, nach dem Entwurf in `03_entwurf_ter_umschreibung.md`.

## Noch zu entscheiden

- Meeres-Overlays ein- oder ausschließen (Ortho4XP schreibt die Zeile auch auf Meereskacheln, nur
  nicht auf Binnenwasser).
- Voreinstellung an oder aus.
- Ort der Einstellung: eigener Abschnitt `[decal]` oder unter `[texture]`.
