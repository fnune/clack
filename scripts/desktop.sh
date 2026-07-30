#!/usr/bin/env bash
# Runs the desktop app in development.
#
# Two host quirks are handled here. On a non-NixOS system the Nix webkitgtk
# cannot reach the system GPU drivers, so the app goes through nixGL. And GTK3
# has no server-side decoration support on Wayland, so under XWayland the
# window gets the desktop's own decorations instead of GTK's.
set -euo pipefail

cd "$(dirname "$0")/../apps/desktop"

export GDK_BACKEND="${GDK_BACKEND:-x11}"
export LD_LIBRARY_PATH="${CLACK_DESKTOP_LIBRARY_PATH:-}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

if [[ -e /etc/NIXOS ]]; then
    exec pnpm tauri dev "$@"
fi

exec nix run --impure github:nix-community/nixGL -- pnpm tauri dev "$@"
