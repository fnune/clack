#!/usr/bin/env bash
# Renders the application icons from packaging/icon.svg.
#
# The tray icon is not here: it is drawn at runtime in the panel's own colour,
# because a tray icon has to be monochrome and Clack cannot know which
# monochrome until it asks the desktop. These are the bundled ones, which do sit
# on a plate.
set -euo pipefail

cd "$(dirname "$0")/.."

source="packaging/icon.svg"
icons="apps/desktop/src-tauri/icons"

rsvg-convert --width 512 --height 512 "$source" --output "$icons/icon.png"
rsvg-convert --width 128 --height 128 "$source" --output "$icons/128x128.png"

# A hairline is a proportion of the drawing, so at 32 pixels it lands on 1.6 of
# them and greys out. The weight is set for the size rather than scaled down to
# it, which is the same reason the tray icon is drawn instead of shrunk.
heavier="$(mktemp --suffix=.svg)"
trap 'rm -f "$heavier"' EXIT
sed 's/stroke-width="1.6"/stroke-width="1.95"/' "$source" >"$heavier"
rsvg-convert --width 32 --height 32 "$heavier" --output "$icons/32x32.png"

# The two the README shows: a dark plate for a light page, and a light one for a
# dark page. A near-black plate on GitHub's dark background is a mark floating in
# nothing.
rsvg-convert --width 256 --height 256 "$source" --output assets/icon.png

paper="$(mktemp --suffix=.svg)"
trap 'rm -f "$heavier" "$paper"' EXIT
sed -e 's/#1e2124/#faf9f7/' -e 's/#101112/#e6e4e0/' -e 's/#e9eaec/#1a1c1e/' \
    "$source" >"$paper"
rsvg-convert --width 256 --height 256 "$paper" --output assets/icon-on-dark.png

# Windows takes every size it might want in the one file, and picks.
magick "$icons/icon.png" -define icon:auto-resize=256,128,64,48,32,16 "$icons/icon.ico"

echo "Wrote $icons and assets/icon.png from $source"
