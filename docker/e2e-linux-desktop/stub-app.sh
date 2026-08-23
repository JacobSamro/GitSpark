#!/bin/sh
# Stands in for "a real app the OS handed a file/URL to." Every invocation
# appends its arguments to a marker file the e2e suite polls — this proves
# the real OS mechanism (xdg-open, the shell editor command, ...) actually
# completed the handoff, without the test depending on which real editor or
# file manager happens to be installed in the image.
echo "$*" >> "${GITSPARK_GUI_MARKER_FILE:-/tmp/gitspark-gui-marker.log}"
