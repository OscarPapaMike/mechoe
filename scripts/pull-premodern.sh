#!/usr/bin/env bash
# Pull all main premodern-era sets (Alpha through Scourge, 1993–2003).
# Skips box sets, promos, collectors editions, and Unglued.
#
# Each set is followed by a 45-second pause so the full run spreads over ~1 hour
# without hammering Scryfall.  Already-cached files are skipped automatically.
#
# Run from any directory inside the mechoe repo:
#   bash scripts/pull-premodern.sh

set -euo pipefail

MDATA="$(dirname "$0")/../mdata"
BIN="$MDATA/target/release/mdata"

# Build if binary doesn't exist.
if [[ ! -f "$BIN" ]]; then
    echo "[pull-premodern] building mdata ..."
    (cd "$MDATA" && ~/.cargo/bin/cargo build --release -q)
fi

SETS=(
    # ── Old School (1993–1994) ──────────────────
    LEA   # Limited Edition Alpha
    LEB   # Limited Edition Beta
    ARN   # Arabian Nights
    ATQ   # Antiquities
    3ED   # Revised Edition
    LEG   # Legends
    DRK   # The Dark
    FEM   # Fallen Empires

    # ── Ice Age era (1995–1996) ─────────────────
    4ED   # Fourth Edition
    ICE   # Ice Age
    CHR   # Chronicles
    HML   # Homelands
    ALL   # Alliances

    # ── Mirage block (1996–1997) ────────────────
    MIR   # Mirage
    VIS   # Visions
    5ED   # Fifth Edition
    WTH   # Weatherlight

    # ── Portal (1997–1999) ──────────────────────
    POR   # Portal
    P02   # Portal Second Age
    PTK   # Portal Three Kingdoms

    # ── Tempest block (1997–1998) ───────────────
    TMP   # Tempest
    STH   # Stronghold
    EXO   # Exodus

    # ── Urza block (1998–1999) ──────────────────
    USG   # Urza's Saga
    ULG   # Urza's Legacy
    6ED   # Classic Sixth Edition
    UDS   # Urza's Destiny

    # ── Masques block (1999–2000) ───────────────
    MMQ   # Mercadian Masques
    NEM   # Nemesis
    PCY   # Prophecy

    # ── Invasion block (2000–2001) ──────────────
    INV   # Invasion
    PLS   # Planeshift
    7ED   # Seventh Edition
    APC   # Apocalypse

    # ── Odyssey block (2001–2002) ───────────────
    ODY   # Odyssey
    TOR   # Torment
    JUD   # Judgment

    # ── Onslaught block (2002–2003) ─────────────
    ONS   # Onslaught
    LGN   # Legions
    SCG   # Scourge
)

TOTAL=${#SETS[@]}
PAUSE=45      # seconds between sets
RATE_MS=500   # ms between per-card requests (default 150; higher = more polite)

echo "[pull-premodern] ${TOTAL} sets — ${RATE_MS}ms/card, ${PAUSE}s between sets"
echo "[pull-premodern] estimated runtime: ~$((TOTAL * PAUSE / 60 + 25)) minutes (cold cache)"
echo ""

for i in "${!SETS[@]}"; do
    SET="${SETS[$i]}"
    NUM=$((i + 1))
    echo "━━━ [${NUM}/${TOTAL}] ${SET} $(date '+%H:%M:%S') ━━━"
    "$BIN" pull "$SET" --rate-ms "$RATE_MS"

    if [[ $NUM -lt $TOTAL ]]; then
        echo "  [sleeping ${PAUSE}s before next set ...]"
        sleep "$PAUSE"
    fi
done

echo ""
echo "[pull-premodern] done — all ${TOTAL} sets processed"
