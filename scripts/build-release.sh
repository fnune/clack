#!/usr/bin/env bash
set -euo pipefail

image="docker.io/library/ubuntu:22.04@sha256:0d779ea97881505f5ef0039336ee85edba27519bdba968c284c86ee066a973c8"
archive_snapshot="20260701T000000Z"

node_version="24.18.0"
node_sha256="55aa7153f9d88f28d765fcdad5ae6945b5c0f98a36881703817e4c450fa76742"

rustup_version="1.29.0"
rustup_sha256="4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10"

# Every timestamp the bundle would otherwise take from the clock, so the same
# source produces the same bytes here and in CI.
epoch=1700000000

appimagetool_version="1.9.0"
appimagetool_sha256="46fdd785094c7f6e545b61afcfb0f3d98d8eab243f644b4b17698c01d06083d1"

apprun_sha256="f30140a43a0a59e46db21bdefdf749b9e9f2c6946e92afabbacf98b8ae73fb4f"
linuxdeploy_sha256="e762bea85c8eb0d4b3508d46e5c1f037f717d0f9303ae3b4aafc8b04991fa1ef"
linuxdeploy_appimage_plugin_sha256="1da16a46fa5e058ae740e7c35ed0d36d86cb869ac9cc8a5fd9a1847d7978d99a"
linuxdeploy_gtk_plugin_rev="b5eb8d05b4c0ed40107fe2158c5d8527f94568ef"
linuxdeploy_gstreamer_plugin_rev="2a2e67491c32995a3f279ad0ecbe77abd512b42a"

root="$(cd "$(dirname "$0")/.." && pwd)"

if [[ -z ${CLACK_RELEASE_BUILDER:-} ]]; then
    runtime="$(command -v podman || command -v docker)" || {
        echo "podman or docker is needed to build a release" >&2
        exit 1
    }

    cache="${XDG_CACHE_HOME:-$HOME/.cache}/clack/release"
    mkdir -p "$cache/cargo" "$cache/rustup" "$cache/pnpm" "$cache/target" \
        "$cache/apt/archives/partial" "$cache/apt/lists/partial"

    exec "$runtime" run --rm \
        --env CLACK_RELEASE_BUILDER=1 \
        --volume "$root:/src:z" \
        --volume "$cache/cargo:/root/.cargo:z" \
        --volume "$cache/rustup:/root/.rustup:z" \
        --volume "$cache/pnpm:/root/.local/share/pnpm/store:z" \
        --volume "$cache/apt/archives:/var/cache/apt/archives:z" \
        --volume "$cache/apt/lists:/var/lib/apt/lists:z" \
        --volume "$cache/target:/target:z" \
        --workdir /src \
        "$image" /src/scripts/build-release.sh "$@"
fi

export CARGO_TARGET_DIR=/target

fetch() {
    local url="$1" sha256="$2" destination="$3"
    curl -fsSL -o "$destination" "$url"
    echo "$sha256  $destination" | sha256sum --check --quiet
}

export DEBIAN_FRONTEND=noninteractive
for suite in jammy jammy-updates jammy-security; do
    echo "deb http://snapshot.ubuntu.com/ubuntu/$archive_snapshot $suite main universe"
done > /etc/apt/sources.list
cat > /etc/apt/apt.conf.d/99-clack-snapshot <<'EOF'
Acquire::Check-Valid-Until "false";
Acquire::https::snapshot.ubuntu.com::Verify-Peer "false"; # the keyring signs the packages, not TLS
Binary::apt::APT::Keep-Downloaded-Packages "true";
EOF

apt-get update -qq
apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    curl \
    desktop-file-utils \
    file \
    libayatana-appindicator3-dev \
    libcairo2-dev \
    libglib2.0-dev \
    libpango1.0-dev \
    librsvg2-dev \
    libssl-dev \
    libwebkit2gtk-4.1-dev \
    libxkbcommon-dev \
    patchelf \
    pkg-config \
    xz-utils \
    zsync

if [[ ! -x /usr/local/bin/node ]]; then
    fetch "https://nodejs.org/dist/v$node_version/node-v$node_version-linux-x64.tar.xz" \
        "$node_sha256" /tmp/node.tar.xz
    tar -xJf /tmp/node.tar.xz -C /usr/local --strip-components=1
fi

if [[ ! -x /root/.cargo/bin/cargo ]]; then
    fetch "https://static.rust-lang.org/rustup/archive/$rustup_version/x86_64-unknown-linux-gnu/rustup-init" \
        "$rustup_sha256" /tmp/rustup-init
    chmod +x /tmp/rustup-init
    /tmp/rustup-init -y --profile minimal --no-modify-path --default-toolchain none
fi
export PATH="/root/.cargo/bin:$PATH"

if [[ ! -x /usr/local/bin/appimagetool ]]; then
    fetch "https://github.com/AppImage/appimagetool/releases/download/$appimagetool_version/appimagetool-x86_64.AppImage" \
        "$appimagetool_sha256" /usr/local/bin/appimagetool
    chmod +x /usr/local/bin/appimagetool
fi

tools="$HOME/.cache/tauri"
mkdir -p "$tools"
if [[ ! -e "$tools/AppRun-x86_64" ]]; then
    fetch "https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-x86_64" \
        "$apprun_sha256" "$tools/AppRun-x86_64"
    fetch "https://github.com/tauri-apps/binary-releases/releases/download/linuxdeploy/linuxdeploy-x86_64.AppImage" \
        "$linuxdeploy_sha256" "$tools/linuxdeploy-x86_64.AppImage"
    fetch "https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage" \
        "$linuxdeploy_appimage_plugin_sha256" "$tools/linuxdeploy-plugin-appimage.AppImage"
    curl -fsSL -o "$tools/linuxdeploy-plugin-gtk.sh" \
        "https://raw.githubusercontent.com/tauri-apps/linuxdeploy-plugin-gtk/$linuxdeploy_gtk_plugin_rev/linuxdeploy-plugin-gtk.sh"
    curl -fsSL -o "$tools/linuxdeploy-plugin-gstreamer.sh" \
        "https://raw.githubusercontent.com/tauri-apps/linuxdeploy-plugin-gstreamer/$linuxdeploy_gstreamer_plugin_rev/linuxdeploy-plugin-gstreamer.sh"
    chmod +x "$tools"/*
fi

dist="/src/dist"
rm -rf "$dist"
mkdir -p "$dist"

cd /src
cargo build --release --locked --package clack-cli
install -m755 "$CARGO_TARGET_DIR/release/clack" "$dist/clack-linux-amd64"

cd /src/apps/desktop

export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
corepack enable
corepack prepare --activate

pnpm install --frozen-lockfile
pnpm tauri build --bundles appimage -- --locked

bundle="$CARGO_TARGET_DIR/release/bundle/appimage"
appdir="$(find "$bundle" -maxdepth 1 -name '*.AppDir' | head -1)"
if [[ -z $appdir ]]; then
    echo "Tauri left no AppDir behind in $bundle" >&2
    exit 1
fi

export APPIMAGE_EXTRACT_AND_RUN=1

# Tauri's bundler ships its own tray's library, and Clack's tray is the libappindicator crate.
"$tools/linuxdeploy-x86_64.AppImage" --appdir "$appdir" \
    --library /usr/lib/x86_64-linux-gnu/libayatana-appindicator3.so.1

rm -f "$appdir"/usr/lib/libwayland-*.so.* # mesa loads the host's and needs its symbols

# shellcheck disable=SC2016 # the hook expands it, not this script
sed -i 's|^export GDK_BACKEND=x11|export GDK_BACKEND="${GDK_BACKEND:-x11}"|' \
    "$appdir/apprun-hooks/linuxdeploy-plugin-gtk.sh"

find "$appdir" -exec touch --no-dereference --date="@$epoch" {} +

appimagetool \
    --no-appstream \
    --mksquashfs-opt -mkfs-time --mksquashfs-opt "$epoch" \
    --mksquashfs-opt -all-time --mksquashfs-opt "$epoch" \
    --mksquashfs-opt -all-root \
    "$appdir" "$dist/Clack-x86_64.AppImage"

# What went into the bundle, so two builds that disagree can be compared without
# shipping 80 megabytes around.
{
    (cd "$appdir" && find . -type f -exec sha256sum {} + | sort -k2)
    (cd "$appdir" && find . -type l -printf '%p -> %l\n' | sort)
} > "$dist/appdir-manifest.txt"

cd "$dist"
sha256sum clack-linux-amd64 Clack-x86_64.AppImage > checksums.txt
cat checksums.txt
