#!/bin/bash
# Run H Tuning — persistent machen
# Basierend auf Run G Analyse (2026-02-22)
#
# Änderungen:
#   1. sysctl: swappiness 1→10 (graduelles Background-Swap)
#   2. sysctl: min_free_kbytes 1G→2G (mehr Puffer)
#   3. sysctl: watermark_boost_factor 0→15000 (kswapd Boost reaktivieren)
#   4. sysctl: watermark_scale_factor 10→50 (breitere Watermark-Lücke)
#   5. udev: NVMe PM QOS latency → 0 (Power-State-Exit-Latenz eliminieren)
#
# Usage: sudo bash monitoring/apply_run_h_tuning.sh

set -euo pipefail

echo "=== Run H Tuning ==="
echo ""

# ─── 1-4: sysctl ──────────────────────────────────────────────────────

SYSCTL=/etc/sysctl.d/99-custom-tuning.conf

echo "[1/3] Patching $SYSCTL ..."
cp "$SYSCTL" "$SYSCTL.bak.run-g"
echo "  Backup: $SYSCTL.bak.run-g"

# swappiness 1 → 10
sed -i '/^# Quasi nur Notfall-Swap/,/^vm\.swappiness = 1$/{
  /^# Quasi/c\# Erlaubt kswapd, kalte Anon-Pages graduell nach zram auszulagern,\n# statt sie bis zum Notfall zu halten und dann als Panik-Burst zu swappen.\n# Mit zram (Kompression im RAM) ist Swap-IO billig.\n# Run G zeigte: swappiness=1 → 538k pages/s Swap-Storm bei Minute 42.\n# war: 1 (quasi Notfall-only), davor: 10
  /^# RSS/d
  s/^vm\.swappiness = 1$/vm.swappiness = 10/
}' "$SYSCTL"

# min_free_kbytes 1G → 2G
sed -i 's/^# 1 GB Watermark gibt/# 2 GB Watermark gibt/' "$SYSCTL"
sed -i 's/^# 1 GB = ~1% von 96 GB, unbedenklich\./# 2 GB = ~2% von 96 GB, unbedenklich. Run G: 1 GB war zu knapp (free=1.4 GB während Storm)./' "$SYSCTL"
sed -i 's/^# war: 524288 (512 MB)$/# war: 1048576 (1 GB), davor: 524288 (512 MB)/' "$SYSCTL"
sed -i 's/^vm\.min_free_kbytes = 1048576$/vm.min_free_kbytes = 2097152/' "$SYSCTL"

# watermark_boost_factor + watermark_scale_factor (nach min_free_kbytes einfügen)
sed -i '/^vm\.min_free_kbytes = 2097152$/a\
\
# kswapd Boost: reclaimed extra Pages pro Wakeup (150% Boost).\
# Liquorix setzt dies auf 0 (deaktiviert!). Run G zeigte: kswapd kam\
# in Lastspitzen nicht hinterher → Direct Reclaim auf X-Plane Main Thread.\
# war: 0 (Liquorix Default)\
vm.watermark_boost_factor = 15000\
\
# Abstand zwischen min/low/high Watermarks vergrößern.\
# 50 = 0,05% von RAM = ~48 MB Lücke. Gibt kswapd mehr Vorlauf\
# bevor Direct Reclaim einsetzt.\
# war: 10 (0,01% = ~10 MB)\
vm.watermark_scale_factor = 50' "$SYSCTL"

echo "  swappiness: 1 → 10"
echo "  min_free_kbytes: 1G → 2G"
echo "  watermark_boost_factor: 0 → 15000"
echo "  watermark_scale_factor: 10 → 50"

# ─── 5: NVMe PM QOS via udev ──────────────────────────────────────────

UDEV=/etc/udev/rules.d/60-nvme-tuning.rules

echo ""
echo "[2/3] Patching $UDEV ..."
cp "$UDEV" "$UDEV.bak.run-g"
echo "  Backup: $UDEV.bak.run-g"

# Append PM QOS rule if not already present
if ! grep -q "pm_qos_latency_tolerance_us" "$UDEV"; then
    cat >> "$UDEV" << 'UDEV_RULE'

# NVMe Power-State-Latenz eliminieren — APST erlaubt den Drives,
# in Deep Sleep zu gehen (PS3/PS4). Exit-Latenz der Samsung 990 PRO: ~10 ms.
# Run G bpftrace zeigte: 90% der Slow-IO-Events (>5ms) auf nvme1n1 (990 PRO)
# bei exakt 10-11 ms = Power-State-Exit. Setze Toleranz auf 0 → Drive
# bleibt in PS0 (aktiv). Wirkt auf alle NVMe (schadet SN850X nicht).
# war: 100000 (100 ms Toleranz, erlaubt Deep Sleep)
ACTION=="add|change", SUBSYSTEM=="nvme", ATTR{power/pm_qos_latency_tolerance_us}="0"
UDEV_RULE
    echo "  PM QOS latency tolerance: 100000 → 0 (alle NVMe)"
else
    echo "  PM QOS rule already present, skipping"
fi

# ─── Aktivieren ───────────────────────────────────────────────────────

echo ""
echo "[3/3] Aktiviere Änderungen ..."

# sysctl sofort laden
sysctl -p "$SYSCTL"

# udev neu laden
udevadm control --reload-rules
udevadm trigger --subsystem-match=nvme

# PM QOS sofort setzen (udev-Trigger wirkt evtl. nicht auf laufende Devices)
for dev in /sys/class/nvme/nvme*/power/pm_qos_latency_tolerance_us; do
    echo 0 > "$dev" 2>/dev/null && echo "  Set $dev → 0" || true
done

echo ""
echo "=== Fertig ==="
echo ""
echo "Verifizierung:"
sysctl vm.swappiness vm.min_free_kbytes vm.watermark_boost_factor vm.watermark_scale_factor
echo ""
for dev in /sys/class/nvme/nvme*/power/pm_qos_latency_tolerance_us; do
    echo "$dev = $(cat $dev)"
done
