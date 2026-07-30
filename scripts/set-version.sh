#!/usr/bin/env bash
# Writes a version across every manifest that carries one.
#
# The desktop app is outside the Cargo workspace, because a browser engine has
# no business in the command line build, so it cannot inherit the version and
# has to be told separately.
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <version>" >&2
    exit 1
fi

next="$1"
root="$(cd "$(dirname "$0")/.." && pwd)"
desktop="$root/apps/desktop/src-tauri"

current="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -1)"

if [[ -z "$current" ]]; then
    echo "could not read the current version from Cargo.toml" >&2
    exit 1
fi

sed -i "s/version = \"$current\"/version = \"$next\"/g" \
    "$root/Cargo.toml" \
    "$desktop/Cargo.toml"

(cd "$root" && cargo update --workspace --offline --quiet)
(cd "$desktop" && cargo update --workspace --offline --quiet)

echo "$current -> $next"
