# Clack

Shows the keys you press, on screen, for screencasts and demos. Wayland only.

[Screenkey](https://www.thregr.org/wavexx/software/screenkey/) cannot do this on Wayland: it asks the X server for every key event, and Wayland has no equivalent, by design. Clack reads the kernel's input devices instead, and draws on a `wlr-layer-shell` surface so the overlay stays above other windows rather than behind them.

Tested on KDE Plasma. Should work on any compositor implementing layer shell, which includes GNOME, sway, Hyprland, and COSMIC.

Early days. The overlay works; theming beyond a few flags, a config file, and most of screenkey's display modes do not exist yet.

## Install

```sh
nix profile install github:fnune/clack
```

Or run it without installing: `nix run github:fnune/clack`.

## Device access

Clack reads `/dev/input/event*`, which needs permission. Install the udev rule:

```sh
sudo install -Dm444 packaging/65-clack.rules /etc/udev/rules.d/65-clack.rules
sudo udevadm control --reload
sudo udevadm trigger --subsystem-match=input
```

That grants an ACL to whoever is logged in on the active seat, revoked at logout. No group membership, no setuid, no password prompt at launch.

`clack --check` tells you whether this worked, along with whether your compositor supports layer shell.

## Use

```sh
clack
clack --font "JetBrains Mono Bold 28" --top --margin 140 --timeout 3
clack --print          # stdout instead of the overlay
clack --check          # device access, layer shell, keymap
clack --list-devices
```

Ctrl+C to stop.

Clack takes the keymap from your compositor, so the layouts you configured in your desktop settings are the ones it displays.

## What it can see

Clack reads every keystroke on the machine, including keystrokes typed into other applications, including passwords. There is no way to show your keystrokes without that ability.

What it does with it:

- Nothing is written to disk. No history, no cache.
- No network code exists in this project.
- No keystroke enters a log, at any level, in any build.
- Only devices that report as keyboards are opened, read-only, never grabbed.

The overlay itself is the indicator that Clack is running. Stop it before you type a password.

## As a library

The CLI is a thin consumer of the `clack` crate. Anything implementing `Surface` can take the overlay's place.

```rust
let mut session = clack::Session::overlay(&clack::Config::default())?;
let handle = session.handle();   // pause, resume, or stop from another thread

session.run()?;
```

## Tray app

`apps/tray` is a Tauri app that drives the library: a tray icon, a settings window, and an option to start hidden in the tray. Closing the window leaves it in the tray.

```sh
./scripts/tray.sh
```

Two things that script handles, both host quirks rather than choices:

- GTK3 has no `xdg-decoration` support, so on Wayland it always draws its own titlebar. Running under XWayland gets you the desktop's own decorations instead, so `GDK_BACKEND=x11` is the default.
- On a non-NixOS host, the Nix build of webkitgtk cannot reach the system GPU drivers and the web view stays blank. The script routes it through nixGL.

## Development

```sh
nix develop                      # toolchain and system libraries
cargo test
./scripts/headless-overlay.sh    # renders the overlay in a headless sway and screenshots it
```

`direnv allow` enters the shell automatically if you add an `.envrc` containing `use flake`.

Tests never touch your session: the input layer is driven over a pipe of `input_event` records, and the render layer runs inside a nested compositor.

- [`DESIGN.md`](DESIGN.md): why Wayland forces this shape, and what the alternatives were.
- [`engineering/README.md`](engineering/README.md): principles and conventions.

MIT licensed.
