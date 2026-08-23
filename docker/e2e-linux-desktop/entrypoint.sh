#!/bin/sh
set -e

Xvfb "$DISPLAY" -screen 0 1280x800x24 &
XVFB_PID=$!
trap 'kill "$XVFB_PID" 2>/dev/null' EXIT

# Give Xvfb a moment to actually start listening before anything tries to
# connect to it.
for _ in 1 2 3 4 5 6 7 8 9 10; do
  if xdpyinfo -display "$DISPLAY" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

fluxbox >/tmp/fluxbox.log 2>&1 &
sleep 1

rm -f "$GITSPARK_GUI_MARKER_FILE"

cargo build --locked
exec node e2e/gui-handoff-linux-suite.mjs
