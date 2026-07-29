#!/usr/bin/env bash
# Renders the overlay inside a headless sway and screenshots it, so the check
# never touches the developer's own screen or its contents.
set -euo pipefail

output=${1:-overlay.png}

if [[ -n ${CLACK_INSIDE_SWAY:-} ]]; then
    cargo test --test overlay -- --ignored --nocapture &
    test_pid=$!
    sleep 3
    grim "$output"
    kill "$test_pid" 2>/dev/null || true
    swaymsg exit
    exit 0
fi

config=$(mktemp)
trap 'rm -f "$config"' EXIT
cat > "$config" <<CONFIG
output HEADLESS-1 resolution 1920x1080 scale 2
exec CLACK_INSIDE_SWAY=1 "$(realpath "$0")" "$(realpath -m "$output")"
CONFIG

WLR_BACKENDS=headless \
WLR_RENDERER=pixman \
WLR_LIBINPUT_NO_DEVICES=1 \
    sway -c "$config" >/dev/null 2>&1

echo "wrote $output"
