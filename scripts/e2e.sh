#!/usr/bin/env bash
# End to end: the built binary reads a device, names the keys with a real xkb
# keymap, and draws on a real compositor. The check is that typing changed
# pixels near the edge the overlay is anchored to, and nowhere else.
#
# Nothing here can reach the developer's session. The compositor is a nested
# headless sway with its own output, so the overlay is never on their screen and
# grim only ever captures that compositor. The "device" is a pipe rather than a
# uinput keyboard, because a uinput device is a real kernel keyboard and the
# host compositor would deliver its keys into whatever window has focus.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
readonly BUDGET=180
readonly ANCHORED_STRIP="1920x300+0+780"
readonly REST_OF_SCREEN="1920x780+0+0"
readonly MINIMUM_BRIGHTNESS_CHANGE=0.001

if [[ -z ${CLACK_INSIDE_SWAY:-} ]]; then
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT

    # Sway config lines have no continuation syntax, so this one stays whole.
    cat > "$work/sway.conf" <<CONFIG
output HEADLESS-1 resolution 1920x1080
exec CLACK_INSIDE_SWAY=1 CLACK_NESTED_COMPOSITOR=1 CLACK_HOST_DISPLAY=${WAYLAND_DISPLAY:-none} CLACK_E2E_LOG=$work/log CLACK_E2E_VERDICT=$work/verdict "$root/scripts/e2e.sh"
CONFIG

    WLR_BACKENDS=headless \
    WLR_RENDERER=pixman \
    WLR_LIBINPUT_NO_DEVICES=1 \
        timeout "$BUDGET" sway -c "$work/sway.conf" >/dev/null 2>&1 || true

    if [[ ! -s $work/log ]]; then
        echo "FAIL: the nested compositor never ran the test" >&2
        exit 1
    fi

    cat "$work/log"
    exit "$(cat "$work/verdict" 2>/dev/null || echo 1)"
fi

exec >"$CLACK_E2E_LOG" 2>&1
echo 1 > "$CLACK_E2E_VERDICT"

if [[ ${WAYLAND_DISPLAY:-} == "${CLACK_HOST_DISPLAY:-}" ]]; then
    echo "FAIL: this is the host compositor, not a nested one"
    exit 1
fi

work=$(mktemp -d)
cd "$root"
cargo build --quiet --bin clack --example feed-keys

keyboard="$work/keyboard"
mkfifo "$keyboard"
# Opening read-write means neither end ever blocks waiting for the other.
exec 3<> "$keyboard"

# Empty, so the developer's own settings cannot decide whether this passes.
: > "$work/config.toml"

CLACK_TEST_KEYMAP=crates/clack/tests/fixtures/us.xkb \
    ./target/debug/clack \
    --config "$work/config.toml" \
    --device "$keyboard" \
    --timeout 30 \
    --margin 64 &
clack=$!
trap 'kill "$clack" 2>/dev/null || true; swaymsg exit >/dev/null 2>&1 || true' EXIT

sleep 2
if ! kill -0 "$clack" 2>/dev/null; then
    echo "FAIL: clack exited before anything was typed"
    exit 1
fi

grim "$work/before.png"
timeout 30 cargo run --quiet --example feed-keys -- "$keyboard" h e l l o
sleep 1
grim "$work/after.png"

# Mean brightness of a region. The overlay is a light pill on a dark desktop,
# so drawing it raises the mean where it is anchored and nowhere else.
brightness() {
    magick "$1" -crop "$2" +repage -colorspace gray -format "%[fx:mean]" info:
}

before_anchored=$(brightness "$work/before.png" "$ANCHORED_STRIP")
after_anchored=$(brightness "$work/after.png" "$ANCHORED_STRIP")
before_rest=$(brightness "$work/before.png" "$REST_OF_SCREEN")
after_rest=$(brightness "$work/after.png" "$REST_OF_SCREEN")

echo "anchored strip: $before_anchored before, $after_anchored after"
echo "rest of screen: $before_rest before, $after_rest after"

awk -v ba="$before_anchored" -v aa="$after_anchored" \
    -v br="$before_rest" -v ar="$after_rest" \
    -v floor="$MINIMUM_BRIGHTNESS_CHANGE" '
BEGIN {
    if (aa - ba < floor) {
        print "FAIL: typing drew nothing where the overlay is anchored"
        exit 1
    }
    if (ar - br > floor || br - ar > floor) {
        print "FAIL: something changed outside the region the overlay is anchored to"
        exit 1
    }
}'

echo "PASS"
echo 0 > "$CLACK_E2E_VERDICT"
