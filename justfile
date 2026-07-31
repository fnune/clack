# Clack task runner. Everything here expects `nix develop`.

default:
    @just --list

# Everything CI runs, in the order it fails fastest
check:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo deny check
    cd apps/desktop && pnpm check && pnpm typecheck && pnpm build

# Run the desktop app in development
desktop:
    ./scripts/desktop.sh

# The built binary, a real compositor, checked from pixels
e2e:
    ./scripts/e2e.sh

# Draw the overlay in a nested compositor and screenshot it, for looking at
overlay:
    ./scripts/headless-overlay.sh

# What a release ships, built the same way CI builds it
artifacts:
    ./scripts/build-release.sh

# Point the Nix desktop package at a release, once its AppImage is published
pin-desktop version:
    #!/usr/bin/env bash
    set -euo pipefail

    url="https://github.com/fnune/clack/releases/download/v{{version}}/Clack-x86_64.AppImage"
    hash=$(nix hash convert --hash-algo sha256 --to sri "$(nix-prefetch-url "$url")")

    sed -i '/desktopRelease = {/,/};/{
        s|version = "[^"]*";|version = "{{version}}";|
        s|hash = "[^"]*";|hash = "'"$hash"'";|
    }' flake.nix

    echo "clack-desktop now takes v{{version}} from $hash"

# Show what the next release's notes would say
notes:
    git-cliff --unreleased

# Tag a release. CI builds it and drafts it on GitHub
release version:
    #!/usr/bin/env bash
    set -euo pipefail

    if [[ -n "$(git status --porcelain)" ]]; then
        echo "Working tree is not clean. Commit first."
        exit 1
    fi

    git fetch origin main
    if [[ -n "$(git diff HEAD origin/main)" ]]; then
        echo "Local main differs from origin. Push first."
        exit 1
    fi

    ./scripts/set-version.sh "{{ version }}"

    git-cliff --tag "v{{ version }}" --unreleased --prepend CHANGELOG.md
    awk 'prev != "" && /^## \[/ { print "" } { print; prev = $0 }' CHANGELOG.md > CHANGELOG.md.tmp
    mv CHANGELOG.md.tmp CHANGELOG.md

    git commit -am "chore(release): v{{ version }}"
    git push origin HEAD:main

    git tag "v{{ version }}"
    git push origin "v{{ version }}"
    echo "Tagged v{{ version }}. Watch https://github.com/fnune/clack/actions"

# Undo a release: the GitHub release and the tag, everywhere
unrelease version:
    gh release delete "v{{ version }}" --yes || true
    git tag -d "v{{ version }}" || true
    git push origin --delete "v{{ version }}" || true
