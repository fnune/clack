# Clack

Shows the keys you press, on screen, for screencasts and demos. Wayland only.

[Screenkey](https://www.thregr.org/wavexx/software/screenkey/) cannot do this on Wayland: it asks the X server for every key event, and Wayland has no equivalent, by design. Clack reads the kernel's input devices instead, and draws on a `wlr-layer-shell` surface so the overlay stays above other windows rather than behind them.

Tested on KDE Plasma, and should work on any compositor with layer shell. Early days: the overlay works, but there is no config file yet and most of screenkey's display modes are missing.

## Try it

```sh
nix run github:fnune/clack -- --check
nix run github:fnune/clack
```

Clack reads `/dev/input/event*`, which needs permission. `--check` says whether it has that, and whether your compositor supports layer shell.

## Install

On NixOS, add the flake as an input and enable the module. It installs the binary and tells udev about the rule, which a package on its own cannot do:

```nix
{
  inputs.clack.url = "github:fnune/clack";

  # in your configuration
  imports = [ inputs.clack.nixosModules.default ];
  programs.clack.enable = true;
}
```

Anywhere else, install the binary however you like and then grant it access once:

```sh
clack --install-rule
```

That shows you the udev rule it wants to write, then asks for your password once through polkit. It grants an ACL to whoever is logged in on the active seat, revoked at logout. No group membership, no setuid, no prompt on later launches.

`nix profile install` on its own is not enough: udev only reads rules a system has been told about, so the rule sits unused in your profile and Clack cannot open any keyboard. Use the module, or run `--install-rule`.

## Use

```sh
clack
clack --font "JetBrains Mono Bold 28" --top --margin 140 --timeout 3
clack --print                       # stdout instead of the overlay
clack --check                       # device access, layer shell, keymap
clack --list-devices
clack --device /dev/input/event3    # one keyboard, skipping the usual test
clack --keymap ./layout.xkb         # override the compositor's keymap
```

Ctrl+C to stop. Layouts come from your compositor, so what you configured in your desktop settings is what Clack shows.

## What it can see

Clack reads every keystroke on the machine, including keystrokes typed into other applications, including passwords. There is no way to show your keystrokes without that ability.

What it does with it:

- Nothing is written to disk. No history, no cache.
- No network code exists in this project.
- No keystroke enters a log, at any level, in any build.
- Only devices reporting as keyboards are opened, read-only, never grabbed.

The overlay is the indicator that Clack is running. Stop it before you type a password.

## Tray app

`apps/tray` is a Tauri app driving the library: a tray icon, a settings window, and an option to start hidden in the tray. Closing the window leaves it running in the tray.

```sh
./scripts/tray.sh
```

## As a library

The CLI and the tray are both consumers of the `clack` crate. Anything implementing `Surface` can take the overlay's place.

```rust
let mut session = clack::LiveSession::overlay(&clack::Config::default())?;
let handle = session.handle();   // pause, resume, or stop from another thread

session.run()?;
```

## Development

```sh
nix develop
cargo test
./scripts/e2e.sh                 # the built binary, a real compositor, checked from pixels
./scripts/headless-overlay.sh    # draws the overlay and screenshots it, for looking at
```

Tests cannot reach your session: keys are fed through a pipe rather than a uinput device, and anything that draws runs inside a nested compositor.

- [`DESIGN.md`](DESIGN.md): why Wayland forces this shape, and what the alternatives were.
- [`engineering/README.md`](engineering/README.md): principles and conventions.

MIT licensed.
