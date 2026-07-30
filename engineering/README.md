# Engineering principles

Principles, declarations, and shared conventions for Clack. Not feature-specific.

Clack is a Wayland keystroke visualiser: it reads input devices, translates keycodes, and paints an overlay. Doing that requires read access to every key event on the machine, including events destined for other applications. That capability, rather than the overlay, is what shapes most of what follows.

---

## Principles

### Calm app

Clack is built to feel calm. Simple, clear, predictable, respectful of user agency. Engineers carry this through to the code, the copy, the defaults, and the failure paths. Today the overlay is the product's entire surface. Whatever configuration front-end arrives later, on the command line or graphical, is held to the same standard.

- Simplicity is the default. A feature earns its place or it does not ship.
- Clarity over cleverness. Plain copy, ordinary words, predictable defaults.
- The overlay is a guest on the user's screen. It never steals focus, never takes pointer input, never covers content the user did not ask it to cover. Click-through is not an option, it is the baseline.
- Quiet defaults. Off-screen when idle, no animation for its own sake, no attention-seeking motion. A tool for screencasts must not become the thing the viewer watches.
- Every interaction is calm. No dark patterns. No shame buttons ("Accept" / "Maybe later"). No ads. No nagging. No update prompts, no telemetry, no first-run tour.
- Respect user agency. The user is in control. We do not coerce, guilt, or hide options. Every default is discoverable and overridable from the CLI and the config file.
- The user can always stop it. A pause binding and a clean exit path are load-bearing features, not extras.
- Failures are shown calmly. Error messages are accurate, plain, and never blame the user. When Clack cannot start, it says which of the two things is missing (device access or layer-shell) and what to do about it.
- A configuration front-end configures, it does not supervise. No dashboard of what you typed. No history view. See the privacy section.

### Testability via dependency injection

Reference: [Mark Seemann](https://blog.ploeh.dk/), [*Dependency Injection Principles, Practices, and Patterns*](https://www.manning.com/books/dependency-injection-principles-practices-patterns).

The book is object-oriented and .NET-flavoured. The principles carry to Rust; the mechanics change. Traits replace interfaces, `main` is the Composition Root, and there is no container.

- Inject Volatile Dependencies: anything with side effects (device IO, the Wayland socket, the file system), nondeterministic behaviour (time, randomness), a runtime configuration requirement, or that does not yet exist. For Clack that means the key source, the keymap provider, the clock, and the render surface.
- Do not inject Stable Dependencies: deterministic, side-effect-free types such as stdlib primitives, value objects, and pure functions. A keycode-to-keysym lookup over an already-loaded keymap is stable. The thing that fetched the keymap is not.
- Constructor injection is the default: dependencies arrive as struct fields set by a single constructor, or as generic parameters. Nothing reaches out to acquire its own collaborators.
- Prefer static dispatch (generic parameters with trait bounds) on the input and render path, where the call rate is high and there is exactly one production implementation. Use `dyn Trait` where the extra flexibility is actually exercised, for example when a backend is selected at runtime.
- The Composition Root is `main.rs`. It parses the CLI, loads config, opens devices, connects to the compositor, and hand-wires the object graph. Nothing else in the codebase names a concrete implementation of a Volatile Dependency.
- Pure DI only. No DI container crate. The graph is small enough to read on one screen and should stay that way.
- Service Locator is an anti-pattern: it hides dependencies behind a runtime lookup. Receive them as parameters instead. A global registry or a `OnceLock<Config>` is the same anti-pattern wearing Rust clothes.
- Ambient Context is an anti-pattern. `Instant::now()`, `SystemTime::now()`, `std::env::var`, and process-global loggers reached from deep code all count. Inject a `Clock`. This is not pedantry: the decay timer, repeat compression, and fade behaviour are all time-driven, and they are only testable if time is a parameter.
- A growing constructor argument list is a code smell that the type is doing too much.
- Tests follow the seams DI creates. Real implementations of Stable Dependencies. Hand-written in-memory fakes for Volatile ones: a scripted key source, a fixed keymap, a manually advanced clock, a recording surface that captures draw calls instead of pixels. Do not reach for a mocking crate.
- Integration tests use the real thing where reasonable: real `input_event` records over a pipe for the reader, a real headless compositor for the layer surface. Do not mock what you own. The limit is the developer's session, which a test must never touch. See the CI section.

### Layered architecture and abstraction ownership

- Three layers: input, domain, render. Input reads devices and produces raw key events. Domain holds the state machine that turns those into displayable text. Render paints it.
- Each layer declares the abstractions it consumes. The trait belongs to the consumer, not to the implementer.
- The domain declares `KeySource`, `Keymap`, and `Clock`. The input layer imports and implements them. The domain does not depend on the input layer, does not link libinput, and does not know that evdev exists.
- The domain declares what it hands to rendering: a display model of styled runs, not a Cairo context. The render layer imports that and decides how to paint it. Swapping the text stack is then a render-layer change.
- Counter-pattern to reject: input depending on domain depending on render, or the reverse. That is naive layering, not Dependency Inversion.
- This is the Dependency Inversion Principle from SOLID, applied at the module boundary.
- Practical consequence: the domain crate builds and tests on a machine with no compositor, no input devices, and no root. That is the test for whether the layering is real.

### Strict typing

Rust's type system is the tool the TypeScript rules were reaching for. Use it, and do not defeat it.

- Deny by default in the workspace lints: `unsafe_code`, `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`, `clippy::indexing_slicing`, `clippy::as_conversions`. Library code returns errors; only the Composition Root is allowed to end the process, and it does so with a message.
- `#![forbid(unsafe_code)]` in every crate that can hold it. FFI wrappers around xkbcommon, cairo, and pango are the exception; they isolate `unsafe` behind a safe API, in one module, with the invariant written down.
- Relaxing strictness is always a code smell and is fixed, not papered over. An `#[allow(...)]` carries a comment saying why, and is scoped to the smallest item that needs it. A blanket allow at crate root is a bug.
- Make illegal states unrepresentable. Newtypes over raw integers: `EvdevCode`, `XkbKeycode`, `KeysymValue`. The evdev-to-xkb offset of 8 is a documented conversion between two types, never a stray `+ 8`.
- Parse, do not validate, at every process boundary: the config file, CLI arguments, environment variables, and the keymap string handed over by the compositor. Deserialize into a typed structure with unknown fields denied, then work with the typed value. Never pass a `HashMap<String, String>` inward.
- Exhaustive matching. No catch-all arm on an internal enum, so adding a variant fails the build at every site that must handle it.

### Code comments

- Code is self-evident. Comments are a last resort.
- A comment is only justified by a non-obvious WHY: a hidden constraint, a subtle invariant, a workaround for a specific bug. Protocol quirks and kernel behaviour qualify, and should cite the protocol or the bug.
- Comments that restate WHAT the code does are noise.
- When code needs a comment to be understood, the first move is to make the code self-evident: rename, restructure, extract. A comment is what remains when that fails.
- Run the `stripping-code-comments` skill over the diff before every commit. Treat it as part of the commit, like formatting.
- `# Safety` on every `unsafe` block is not a comment in this sense. It is required.

### Dependency hygiene

- Start with the platform. The standard library, and the system libraries we already link, cover more than the crate ecosystem suggests. We link libinput, libudev, and libxkbcommon because there is no substitute; that is a reason to use them fully, not a reason to add more.
- Vet every dependency: can std or an existing dependency do it; size of the transitive tree; active maintenance; number of `unsafe` blocks; whether it pulls in a runtime we do not otherwise need.
- No async runtime unless something forces it. Clack is one process reacting to two file descriptors. A poll loop over the libinput fd and the Wayland fd is smaller, more predictable, and easier to reason about than a scheduler. Pulling in Tokio for that is a dependency budget spent on nothing.
- Prefer governance-neutral dependencies over OSS that exists to feed a vendor's hosted SaaS. In that pattern the roadmap follows the vendor's revenue, not user need: the best features land in the paid tier, the self-hosting path degrades, telemetry and login prompts creep into the CLI, and at the extreme the licence is rewritten to forbid competing hosted offerings. The integration cost falls on us either way, whether we accept the drift or migrate to a fork. Use such dependencies only when no governance-neutral alternative exists; document the choice in the PR.
- If under 50 lines, write it. Do not depend.
- Vendor utilities under 100 lines with attribution and the original license.
- Prefer crates with minimal transitive trees. Check with `cargo tree --duplicates` before adding.
- Budget: under 20 direct production dependencies for the whole workspace, and no duplicate major versions of a shared crate. Exceeding either requires written justification in the PR.
- Commit `Cargo.lock`. It is a binary, not a library. Treat `Cargo.toml` changes as code changes.
- Pin the toolchain in `rust-toolchain.toml`. Dependency version requirements stay as plain caret ranges resolved by the committed lockfile; Renovate proposes lockfile updates and humans review them.
- `cargo deny` runs in CI over licences, advisories, and duplicate versions. Its config is committed and changes to it are reviewed like code.

### Privacy

This replaces nothing and outranks convenience. Clack reads every keystroke on the machine, including keystrokes typed into other applications, including passwords. There is no way to build this feature without that capability, and no way to make the capability harmless. The obligation is to make it narrow, visible, and short-lived.

- Keystrokes never touch disk. No history file, no crash dump containing key data, no cache. If a future feature seems to need persistence, it does not ship.
- Keystrokes never leave the process boundary except to the screen. No network code in this project at all, so there is nothing to audit later.
- Keystroke data never enters a log, at any level, in any build. Debug logging emits event counts and device names, never key identities. Any tracing of actual keys sits behind a compile-time feature that is off in release and named so nobody enables it by accident.
- Least privilege on device access. Open only devices that report keyboard capability, read-only, and never take an exclusive grab. Drop privileges immediately if any were held.
- Prefer the udev `uaccess` rule over group membership or setuid: it is session-scoped and revoked at logout.
- The privileged helper, if used, stays as small as it can be, does one thing, and is auditable in a sitting.
- Visible while running. The overlay itself is the indicator. A silent mode that captures without displaying is not a feature we build.
- Document the threat model in the README, in the user's terms, before the install instructions. Users deciding whether to install a keylogger deserve the plain version.

### Operational discipline

The baselines below are the floor, not the ceiling.

- Error handling and failure modes. No silent catches, and no `let _ =` on a fallible call. Errors carry context: what was attempted, with what inputs, what failed. Typed errors in library code, a top-level report at the Composition Root. Distinguish recoverable errors from programmer errors; the latter are prevented by types where possible.
- Degrade in the open. A keyboard unplugged mid-session, a compositor that drops the layer surface, a keymap that fails to compile: each has a defined behaviour, and none of them is a panic in the paint loop.
- Observability by default, within the privacy rules. Structured logging with levels, off by default, controlled by a flag rather than an environment variable that could be set by something else. Startup emits the resolved configuration, the device list, and the keymap source. `--check` reports device access, layer-shell availability, and keymap source, and exits nonzero when the tool would not run.
- Security baseline. No network. No secrets. No credentials. The attack surface is the privileged helper and the FFI boundary, so those get the review attention.
- Performance is a correctness property here. The overlay sits in a screencast; dropped frames and lag are visible to every viewer. Keep the input-to-pixel path allocation-free in steady state and measure it rather than assuming.

### Local checks before pushing

- Run cheap checks locally: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and the tests that exercise the touched code.
- Run the `stripping-code-comments` skill over the diff before committing.
- Choose checks based on what changed. Domain-only changes need `cargo test -p clack-domain` and nothing more. Touching the input or render layer means running the integration tests too.
- `cargo deny check` before any dependency change.

### Git conventions

- [Conventional Commits](https://www.conventionalcommits.org/) for every commit message. Types: `feat`, `fix`, `chore`, `docs`, `refactor`, `test`, `perf`, `build`, `ci`, `revert`. Scope optional and lowercase, e.g. `feat(input):`, `fix(render):`.
- Imperative mood. "Add the X", not "Added the X".
- Subject line under 72 characters. Body wraps at 80.
- Body explains why when the why is not obvious from the diff.
- No value-judgement words: "improve", "better", "cleaner", "robust", "elegant". State the change in concrete terms.
- No vague qualifiers: "various", "several minor", "ensure", "make sure".
- No AI attribution and no Co-Authored-By for AI tools.
- One commit per PR is the default. Squash on merge. Split larger changes at natural seams.

### Continuous integration

- Every PR runs the same baseline: format, lint, test, dependency audit. Same commands locally and in CI.
- CI runs on Linux only. That is the entire target platform.
- Headless integration testing is expected, not aspirational. The input layer is driven over a pipe carrying `input_event` records, and the render layer runs inside a headless sway, so both are exercised with no privileges and no hardware.
- A test must never reach the developer's session. A `uinput` device is a real kernel keyboard: the compositor routes its events into whatever window has focus, so a test using one types into the developer's windows. For the same reason, screenshots are taken inside the nested compositor and never of the real screen, which would capture whatever the developer had open. Both mistakes were made here once; neither is acceptable twice.
- The Plasma-specific behaviours that motivated this project, overlay stacking above fullscreen in particular, get an explicit test or an explicit note saying they are manually verified and when.

### Reproducible, one-command developer environments

- Same toolchain and same system library versions across every developer machine and CI.
- One command to set up. No multi-step wiki page.
- Nix flake with a dev shell is the single source of truth. It pins the Rust toolchain alongside the C libraries this project links, which a language-only version manager cannot do. `nix develop`, or `direnv allow` for automatic entry. CI uses the same flake, so a green CI run and a local run mean the same thing.
- `rust-toolchain.toml` is committed as well, so a non-Nix contributor with rustup gets the right compiler and only has to solve the system libraries.

### Toolchain choices

- Rust, current stable, edition pinned in `Cargo.toml`. Chosen for the memory safety guarantee at an FFI-heavy boundary, no runtime, and a single static-ish binary. No unstable features, so the project builds on distribution toolchains.
- Cargo workspace split along the architectural layers, so the layering is enforced by the dependency graph rather than by review.
- rustfmt with default settings. Formatting is not a design discussion.
- Clippy with `-D warnings` and the lint set above.
- Testing: the built-in test harness is the default. `cargo nextest` for the run-time gain on larger suites, `insta` for snapshot assertions over the display model, which is where snapshot testing genuinely pays.
- Dependency updates via [Renovate](https://docs.renovatebot.com/). Weekly batch, 7 day minimum release age, no auto-merge. Security advisories handled same-day.

### TypeScript, in the desktop app

The desktop app is a Tauri shell around the library. Its Rust side holds the logic; the web view is a form. Keep it that way, and the same UI code can front a different transport later.

- Strict typing always. `strict: true` plus `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `noImplicitOverride`, `noUnusedLocals`, and `noUnusedParameters`.
- Relaxing strictness is a code smell and is fixed, not papered over. `any`, `@ts-ignore`, `@ts-expect-error` without explanation, and non-null assertions all count. Biome flags the common ones.
- Parse at the process boundary. Values crossing the Tauri IPC arrive as `unknown` and are turned into typed values by hand-written parsers that throw on anything unexpected. Types are derived from what was parsed, never asserted into existence. The parsers are short enough not to justify a validation library.
- Start with the platform. `fetch`, `structuredClone`, `Intl`, and native array methods before reaching for a package.
- Dependencies: React, React DOM, and the Tauri API. No component library, no state manager, no router, no CSS framework. A settings form needs none of them, and each one is a thing to keep current.
- Pin exactly: no `^` or `~`. The lockfile is committed and `package.json` changes are code changes.
- pnpm is the only package manager. Biome is the only linter and formatter.
- Node comes from the flake, like every other tool.

### Writing style

- Sentence-case in headings.
- Bold sparingly.
- No em-dashes. Use a colon, comma, semicolon, period, or rephrase.
- No rhetorical triplets ("one X, one Y, one Z" and similar three-beat patterns).
- No AI attribution in commit messages or code.

---

## Use of AI and LLMs

This project accepts the use of LLMs to write code. The tools used to build software are changing quickly, but this code is held to the same standards as code written without LLMs.

- Tests at the unit, integration, and end-to-end levels.
- Strict typing and parsed boundaries.
- Dependency hygiene and supply-chain discipline as set out above.
- Architecture and code review applied to every change, AI-assisted or not.
- Extra scrutiny at the `unsafe` and FFI boundary, and at anything touching device access or the privileged helper. Generated code that looks plausible around raw pointers and file descriptors is exactly where plausible is not enough.
