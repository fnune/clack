# Clack

Shows the keys you press, on screen, for screencasts, demos and teaching. For Wayland desktops,
where [screenkey](https://www.thregr.org/wavexx/software/screenkey/) cannot go.

There is a desktop app with a tray icon and a single switch, and a command line program. Both do
the same thing and share the same settings.

## Install

### The desktop app

Download `Clack-x86_64.AppImage` from [the latest
release](https://github.com/fnune/clack/releases/latest), make it executable, and run it:

```sh
chmod +x Clack-x86_64.AppImage
./Clack-x86_64.AppImage
```

It puts an icon in your tray. Click the switch to start showing your keys.

### The command line

Download `clack-linux-amd64` from the same release and put it on your `PATH`:

```sh
install -Dm755 clack-linux-amd64 ~/.local/bin/clack
clack --check
```

### With Nix

On NixOS, add the flake as an input and enable the module:

```nix
{
  inputs.clack.url = "github:fnune/clack";

  # in your configuration
  imports = [ inputs.clack.nixosModules.default ];
  programs.clack.enable = true;
}
```

Elsewhere, `nix run github:fnune/clack`. Installing the package into a profile is not enough on
its own, because udev only reads rules a system has been told about; the module handles that, and
so does `--grant-access` below.

## Letting Clack read your keyboard

Clack reads the kernel's input devices, which needs permission your user does not have by
default. Grant it once:

```sh
clack --grant-access
```

The desktop app has a button for the same thing. Either way you are asked for your password once,
nothing stays elevated, and later launches ask for nothing. `clack --check` says whether it
worked.

`clack --revoke-access` takes it back, and the app offers that too. Afterwards nothing of Clack's
is left outside your home directory. Devices already open stay readable until you log out, which
is how the ACL works rather than anything Clack decides.

## Use

```sh
clack                                                  # show the overlay, Ctrl+C to stop
clack --check                                          # will Clack work here?
clack --anchor top --margin 140 --timeout 3             # where it sits, and for how long
clack --font "JetBrains Mono Bold 28"                   # any font Pango can name
clack --symbols                                        # space and enter get glyphs, not names
clack --print                                          # keystrokes on stdout, to pipe anywhere
clack --reset                                          # settings back to defaults, keeping a copy
```

`--help` lists the rest. Your layout comes from your desktop, so what you configured there is
what Clack shows.

## Settings

One file, at `$XDG_CONFIG_HOME/clack/config.toml`, drives both programs. `clack --config-path`
prints where that is. Flags override it for a single run; the desktop app writes to it as you
change things.

```toml
[overlay]
font = "Sans Bold 24"
margin = 96
anchor = "bottom"
timeout_seconds = 1.5
symbols = false

[overlay.symbol_overrides]
Tab = "→"

[desktop]
start_hidden = false
start_overlay = false
```

Anything you leave out keeps its default. A key Clack does not recognise is an error rather than a
silent no-op, so a typo tells you instead of quietly doing nothing.

## What Clack can see

Clack reads every keystroke on the machine, including keystrokes typed into other applications,
including passwords. There is no way to show your keystrokes without that ability.

What it does with it:

- Nothing is written to disk. No history, no cache.
- No network code exists in this project.
- No keystroke enters a log, at any level, in any build.
- Only devices reporting as keyboards are opened, read-only, never grabbed.

The overlay is the indicator that Clack is running. Stop it before you type a password.

## As a library

Both programs are consumers of the `clack` crate. The overlay is one implementation of `Surface`,
and anything else that can show a `Frame` will do:

```rust
use clack::{Config, Frame, LiveSession, Surface};

struct Caption(String);

impl Surface for Caption {
    type Error = std::convert::Infallible;

    fn present(&mut self, frame: &Frame) -> Result<(), Self::Error> {
        self.0 = frame.runs.iter().map(|run| run.text.as_str()).collect();
        Ok(())
    }
}

let config = Config::default();
let mut session = LiveSession::with_surface(Caption(String::new()), &config)?;
let handle = session.handle();

std::thread::spawn(move || {
    handle.pause();
    handle.resume();
    handle.stop();
});

session.run()?;
```

`Handle` is the only thing that crosses threads. Reading, naming and drawing all happen on the
thread that called `run`.

## How it works

Wayland gives no client the ability to watch another client's input, by design, so screenkey's
approach has no equivalent here. Clack instead reads `/dev/input/event*` directly, asks the
compositor for the keyboard layout it is already using, and draws on a `wlr-layer-shell` surface
so the overlay sits above other windows rather than behind them.

Developed on KDE Plasma. It should work on any compositor implementing layer shell, which is most
of them apart from GNOME's Mutter.

[`DESIGN.md`](DESIGN.md) has the long version: what the alternatives were, why each one fails,
and the compositor behaviour that shaped the drawing code.

## Development

Everything comes from a [Nix](https://nixos.org/download/) flake, so
[install Nix](https://nixos.org/download/) and the rest follows:

```sh
nix develop
just            # lists every task
just check      # what CI runs
just desktop    # the desktop app, in development
just e2e        # the built binary, a real compositor, checked from pixels
```

Tests cannot reach your session: keys are fed through a pipe rather than a uinput device, and
anything that draws runs inside a nested compositor.

[`engineering/README.md`](engineering/README.md) has the principles and conventions.

MIT licensed.
