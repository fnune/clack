#!/usr/bin/env bash
# Runs the tray app in development.
#
# Two host quirks are handled here. On a non-NixOS system the Nix webkitgtk
# cannot reach the system GPU drivers, so the app goes through nixGL. And GTK3
# has no server-side decoration support on Wayland, so under XWayland the
# window gets the desktop's own decorations instead of GTK's.
set -euo pipefail

cd "$(dirname "$0")/../apps/tray"

export GDK_BACKEND="${GDK_BACKEND:-x11}"

if [[ -e /etc/NIXOS ]]; then
    exec pnpm tauri dev "$@"
fi

exec nix run --impure github:nix-community/nixGL -- pnpm tauri dev "$@"
