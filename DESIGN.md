# Clack

Wayland-native keystroke visualizer. Screenkey replacement. Must work on KDE Plasma (KWin), but DE-agnostic. Wayland-only. CLI-configured, renders on screen.

Status: the overlay draws. Input, translation, model, and the layer-shell surface are wired end to end. Theming, config file, and the remaining screenkey semantics are not done.

Engineering principles and conventions: [`engineering/README.md`](engineering/README.md).

---

## 1. Prior art

### screenkey (Python, GTK3, X11-only)

- Reference implementation for the *behaviour* we want: composed/translated/raw/keysyms modes, backspace handling (normal/baked/full), modifier rendering styles (PC/Mac/Emacs), repeat compression, whitespace glyphs, timed fade-out.
- Capture: talks to `libX11.so.6` directly. Uses the X record/input machinery to see every key event delivered by the X server, plus XKB for translation. No root needed, because the X server is already a global input broker and any client can ask it for the whole event stream.
- Why it cannot work on Wayland: **that global input broker does not exist**. In Wayland the compositor delivers key events only to the focused surface. A client has no protocol to say "send me every key". This is deliberate, not an oversight. Under XWayland screenkey still sees X11 clients' input but is blind to native Wayland clients, which is why it appears "half broken" rather than dead.
- Positioning also breaks: screenkey places its window at absolute coordinates. Wayland has no client-side window positioning.

### wshowkeys (C, unmaintained)

- Reads evdev directly via libinput, **setuid root**, drops privileges after opening devices.
- Renders with `wlr-layer-shell-unstable-v1` + pango/cairo. Correct rendering approach.
- Deps: libinput, udev, wayland, xkbcommon, pango.
- Unmaintained. Forks exist (DreamMaoMao, ammgws, "Keystro").

### Show Me The Key (C, GTK4/libadwaita)

- Split process: privileged CLI backend (`pkexec`) reads libinput/libevdev/libudev, emits JSON lines; unprivileged GTK4 frontend renders.
- Translation via libxkbcommon, but from its **own** xkb config, so DE-level remaps are invisible; upstream tells users to remap in udev instead.
- **Renders into an ordinary GTK toplevel** with a magic window title so users can write compositor window rules. No layer-shell.
- Confirmed in practice on this machine: on Plasma the overlay sits *under* other windows. That is the direct consequence of using a plain toplevel: on Wayland a client cannot raise or pin itself, and Plasma's "Keep Above" window rule does not beat a focused window reliably. This is the single biggest thing Clack must do differently.
- Also: `pkexec` prompt on every launch, and it fails on compositors with no polkit agent.

### Others

- **Keyviz**: Flutter, primarily X11 on Linux, not a Wayland citizen.
- **koverlay**: not a keystroke tool, but a useful reference: click-through always-on-top overlay explicitly targeting Plasmashell/KWin via LayerShellQt.
- **wev**: debug tool; shows only keys delivered to its own focused surface. Demonstrates the limitation, not a solution.

---

## 2. The input problem on Wayland

Four candidate sources of global key events. Only one actually works today.

| Source | Verdict |
|---|---|
| `wl_keyboard` key events | **No.** Compositor must not send `key` without the client's surface holding keyboard focus. Only usable for keys typed *into Clack itself*. |
| `xdg-desktop-portal` InputCapture | **No.** Built for InputLeap-style edge-barrier handoff: you declare Zones, place PointerBarriers, and only receive events *after* a barrier is crossed. The compositor controls everything and can filter or stop at will. It is explicitly designed to not be a keylogger. |
| `xdg-desktop-portal` RemoteDesktop + libei | **No.** libei is emulation-first (sender mode); receiver mode is bound to the same capture semantics. The EIS implementation decides which devices exist and when they may send. |
| **evdev / libinput on `/dev/input/event*`** | **Yes.** Below the compositor entirely. Works on KWin, Mutter, wlroots, and on a bare TTY. Requires privileges. |

So: same conclusion every working tool reached. Read the kernel devices, pay the privilege cost, and be honest about it in the README.

### Privilege options

1. **udev `uaccess` rule** (preferred default). Ship `65-clack.rules` tagging keyboard-capable input devices `TAG+="uaccess"`; logind then grants an ACL to the user owning the active seat, revoked on logout. The file must sort after `60-input-id.rules`, which is what sets `ID_INPUT_KEYBOARD`, and before `73-seat-late.rules`, which applies the ACL. No group membership, no setuid, no password prompt, session-scoped.
2. **`input` group membership**. One-liner, but permanent and seat-independent, so weaker.
3. **Privileged helper over a socket**. `clack-input` started by pkexec/systemd, opens devices, passes read-only fds via `SCM_RIGHTS` or streams decoded events to the unprivileged UI. Fallback for users who won't install a udev rule.
4. **setuid**: rejected. wshowkeys-style, hard to audit, worse than (1).

Security note to document plainly: any of these means a userspace process can read every keystroke, including passwords typed into other applications. That is inherent to the feature, not a Clack bug. Mitigation: refuse to run unless explicitly invoked, never log to disk by default, and offer a "pause/panic" hotkey.

### Keycode → symbol

evdev gives raw kernel keycodes (xkb keycode = evdev code + 8). Turning those into "Shift+A" or "é" needs the **active** xkb keymap.

Trick worth exploiting: `wl_keyboard.keymap` is sent to any client that calls `wl_seat.get_keyboard`: it does **not** require focus (only `key` does). So Clack can pull the compositor's real, live keymap fd, mmap it, and feed it to `xkb_keymap_new_from_string`. That makes Clack aware of layouts configured in Plasma's own settings, strictly better than Show Me The Key's approach.

Known gap: the active layout *group* index arrives in `wl_keyboard.modifiers`, which the compositor may send without focus but is not obliged to. If group tracking proves unreliable, fall back to tracking group changes from the evdev stream itself, or expose `--layout` override. Open question, resolve during spike.

---

## 3. The rendering problem on Wayland

`wlr-layer-shell-unstable-v1`, `overlay` layer.

- KWin implements it (listed at protocol version 6.6, alongside Mutter, Sway, Hyprland, COSMIC, Weston). This is what waybar-class tools use on Plasma.
- Gives what a plain toplevel cannot: guaranteed z-order above normal windows, edge anchoring + margins (so `--position bottom-center` is meaningful), and `exclusive_zone = -1` so panels don't reflow around us.
- `keyboard_interactivity = none` and an empty input region → fully click-through, never steals focus.
- Fullscreen caveat: KWin lowers a fullscreen window below an overlay only under conditions involving per-output geometry checks. Needs testing against a fullscreen video/game on this machine before claiming it works.
- No layer-shell fallback planned for v1. If a compositor lacks it, error out with a clear message rather than degrade to the Show Me The Key failure mode.

---

## 4. Design

### Shape

Single Rust binary `clack`, plus optional `clack-input` privileged helper. No daemon, no GUI, no tray.

```
evdev ──> reader thread ──> channel ──> translator ──> model ──> surface
 (one per keyboard)                      (xkb)      (decay)   (layer shell)
```

Two crates. `clack-core` holds the model and the traits it consumes (`KeySource`, `Translator`, `Surface`, `Clock`); it links nothing and needs no devices. `clack` implements those traits and wires them in `main`.

### Modules

- `input`: enumerates `/dev/input/event*`, filters to keyboards, one reader thread each. Emits `(keycode, state, timestamp)`. Reports per-device access so `--check` can explain a permission failure.
- `keymap`: Wayland `wl_seat.get_keyboard` → keymap fd → `xkbcommon`. Compose sequences and dead keys still to do. Fallback to `XKB_DEFAULT_*` env still to do.
- `model`: the screenkey semantics: modes (`composed` / `translated` / `raw` / `keysyms`), backspace policy, repeat compression, whitespace glyphs, buffer decay timer.
- `render`: layer surface, damage tracking, text layout, theme application.
- `config`: TOML at `$XDG_CONFIG_HOME/clack/config.toml`, every key overridable by CLI flag.

### Crates

In use: `evdev`, `wayland-client`, `xkbcommon`, `clap`, `anyhow`, `thiserror`.

Still to choose, for rendering: `smithay-client-toolkit` for the layer surface (see `examples/simple_layer.rs`), and a text stack. `pangocairo` pulls C deps but gives shaping, bidi, CJK, and font fallback for free; `cosmic-text` + `tiny-skia` is the pure-Rust alternative.

Reversed from the first pass: libinput and udev are gone. Clack wants raw key codes, and libinput's value is pointer acceleration, gesture recognition, and device grouping, all of which we would discard. The `evdev` crate reads the same devices with no C dependency and no `open_restricted` privilege callback. Hotplug is the one thing given up, and it is recoverable later with an inotify watch on `/dev/input`.

### Look, customizable

TOML + flags: font family/size/weight, foreground/background/accent colors, opacity, corner radius, padding, anchor (`top|bottom|left|right|center` combos), margin, output selection, max width, fade/decay timeout, modifier glyph style (PC / Mac / Emacs), whether to show plain characters vs only shortcuts.

### CLI sketch

```
clack --position bottom-center --font "Inter 24" --timeout 2s --mode composed
clack --config ~/.config/clack/config.toml
clack --list-devices
clack --check          # report layer-shell support, device access, keymap source
```

`--check` matters: the two failure modes (no device access, no layer-shell) should be diagnosable in one command.

### Milestones

1. Done. Read evdev, translate, print to stdout via `--print`. Untested against real devices: this machine has no ACL on `/dev/input/event*` yet.
2. Done. Keymap pulled from `wl_keyboard.keymap` with no surface and no focus. KWin sends it.
3. Done. `--check` reports device access, layer-shell availability, and keymap source.
4. Done. Layer-shell overlay on the overlay layer, click-through, anchored, rendering at the output's integer scale.
5. Done. `LayerOverlay` implements `Surface`; `--print` keeps the stdout one.
6. Next. screenkey semantics: modes, backspace, compression, glyph styles.
7. Theming and config file.
8. Packaging and the README security section.
9. Overlay above a fullscreen window on KWin, still unverified.

### Verified on KWin (Plasma 6, this machine)

- `zwlr_layer_shell_v1` is advertised.
- The compositor sends a full keymap to a client with no surface and no keyboard focus. 36826 bytes, seat `seat0`. The approach in section 2 holds.

### Testing

- Input layer: pipe of `input_event` records. No kernel device, no privileges, runs in CI.
- Render layer: `scripts/headless-overlay.sh` runs the overlay inside a headless sway and captures it with grim.
- `uinput` was tried and abandoned. A virtual keyboard is a real kernel device, so the compositor delivers its events to whatever window has focus, meaning the tests typed into the running session.

### Open questions

- Overlay above fullscreen on KWin: verify, don't assume.
- Fractional scale. Integer buffer scale is handled; on a 1.75 output the compositor still resamples. `wp_fractional_scale_v1` with `wp_viewporter` is the fix.
- Layout-group tracking without keyboard focus.
- Multi-seat and multi-keyboard merge behaviour.
- Whether to capture mouse buttons at all (screenkey does; defer).

---

## Sources

- [screenkey](https://www.thregr.org/wavexx/software/screenkey/)
- [Show Me The Key](https://github.com/AlynxZhou/showmethekey) · [Hack Week writeup](https://hackweek.opensuse.org/projects/a-screenkey-alternative-that-works-under-wayland-via-reading-evdev-directly)
- [wshowkeys](https://sr.ht/~sircmpwn/wshowkeys/)
- [koverlay](https://github.com/erik96/koverlay)
- [wlr-layer-shell-unstable-v1](https://wayland.app/protocols/wlr-layer-shell-unstable-v1) · [LayerShellQt](https://invent.kde.org/plasma/layer-shell-qt)
- [Wayland Book: keyboard input](https://wayland-book.com/seat/keyboard.html)
- [XDG InputCapture portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.InputCapture.html) · [who-t: libei and the portals](http://who-t.blogspot.com/2022/12/libei-opening-portal-doors.html)
- [smithay client-toolkit](https://github.com/Smithay/client-toolkit)
