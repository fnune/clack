#![cfg_attr(test, allow(clippy::expect_used, clippy::panic, clippy::unwrap_used))]

use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use clack::access::{self, RuleStatus};
use clack::diagnostics::{CompositorReport, KeymapSource};
use clack::input::Access;
use clack::{Config, LiveSession, Report, Settings, TextSurface};
use clap::{Parser, ValueEnum};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Edge {
    Top,
    Bottom,
}

/// Every appearance option defaults to what the settings file says, and the
/// settings file defaults to what the desktop app writes. Flags override both,
/// for this run only.
#[derive(Debug, Parser)]
#[command(name = "clack", version, about = "Show the keys you press, on Wayland")]
struct Cli {
    /// Say whether Clack can run here, and what to do if it cannot
    #[arg(long)]
    check: bool,

    /// Allow Clack to read your keyboard, asking for your password once
    #[arg(long)]
    grant_access: bool,

    /// Take that permission away again, leaving nothing of Clack's behind
    #[arg(long)]
    revoke_access: bool,

    /// Font, in Pango notation
    #[arg(long, value_name = "FONT")]
    font: Option<String>,

    /// Which screen edge to sit against
    #[arg(long, value_name = "EDGE")]
    anchor: Option<Edge>,

    /// Distance from that edge, in pixels
    #[arg(long, value_name = "PIXELS")]
    margin: Option<i32>,

    /// How long a keystroke stays on screen, in seconds
    #[arg(long, value_name = "SECONDS")]
    timeout: Option<f32>,

    /// Draw keys with no letter of their own, such as space and enter, as symbols
    #[arg(long, overrides_with = "no_symbols")]
    symbols: bool,

    /// Draw those keys by name instead
    #[arg(long, overrides_with = "symbols")]
    no_symbols: bool,

    /// Write keystrokes to stdout instead of drawing them on screen
    #[arg(long)]
    print: bool,

    /// Read settings from here instead of the usual place
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Show where the settings file lives, then exit
    #[arg(long)]
    config_path: bool,

    /// Put every setting back to its default, keeping a copy of the old file
    #[arg(long)]
    reset: bool,

    /// Read only this device, skipping the usual keyboard test. Repeatable
    #[arg(long = "device", value_name = "PATH", hide = true)]
    devices: Vec<PathBuf>,

    /// Write the udev rule. Clack passes this to itself once polkit has raised it to root
    #[arg(long, hide = true)]
    write_udev_rule: bool,

    /// Delete the udev rule, likewise
    #[arg(long, hide = true)]
    remove_udev_rule: bool,
}

impl Cli {
    fn config(&self) -> anyhow::Result<Config> {
        let mut settings = match &self.config {
            Some(path) => Settings::load_from(path)?,
            None => Settings::load()?,
        };

        if let Some(font) = &self.font {
            settings.overlay.font = font.clone();
        }
        if let Some(margin) = self.margin {
            settings.overlay.margin = margin;
        }
        if let Some(timeout) = self.timeout {
            settings.overlay.timeout_seconds = timeout;
        }
        if let Some(anchor) = self.anchor {
            settings.overlay.anchor = match anchor {
                Edge::Top => clack::Anchor::Top,
                Edge::Bottom => clack::Anchor::Bottom,
            };
        }
        if self.symbols {
            settings.overlay.symbols = true;
        }
        if self.no_symbols {
            settings.overlay.symbols = false;
        }
        Ok(settings.overlay.to_config())
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let outcome = if cli.config_path {
        show_config_path()
    } else if cli.reset {
        reset()
    } else if cli.write_udev_rule {
        access::apply_as_root(access::Change::Allow).map_err(anyhow::Error::from)
    } else if cli.remove_udev_rule {
        access::apply_as_root(access::Change::Revoke).map_err(anyhow::Error::from)
    } else if cli.grant_access {
        grant_access()
    } else if cli.revoke_access {
        revoke_access()
    } else if cli.check {
        check()
    } else if cli.print {
        print_keys(&cli)
    } else {
        show_overlay(&cli)
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("clack: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn show_config_path() -> anyhow::Result<()> {
    println!("{}", clack::settings::path()?.display());
    Ok(())
}

fn reset() -> anyhow::Result<()> {
    let existed = clack::settings::path()?.exists();
    Settings::reset()?;

    if existed {
        println!(
            "Reset. Your previous settings are in {}.",
            clack::settings::backup_path()?.display()
        );
    } else {
        println!("Reset. There were no settings to keep.");
    }

    Ok(())
}

fn show_overlay(cli: &Cli) -> anyhow::Result<()> {
    let config = cli.config()?;
    let overlay = clack::render::LayerOverlay::open(config.style.clone())?;

    session(overlay, cli, &config)?.run()?;
    Ok(())
}

fn print_keys(cli: &Cli) -> anyhow::Result<()> {
    let config = cli.config()?;

    session(TextSurface::new(io::stdout()), cli, &config)?.run()?;
    Ok(())
}

fn session<S: clack::Surface>(
    surface: S,
    cli: &Cli,
    config: &Config,
) -> anyhow::Result<LiveSession<S>> {
    Ok(if cli.devices.is_empty() {
        LiveSession::with_surface(surface, config)?
    } else {
        LiveSession::on_devices(surface, config, &cli.devices)?
    })
}

fn grant_access() -> anyhow::Result<()> {
    if access::status() == RuleStatus::Installed {
        println!("Clack is already allowed to read your keyboard.");
        return Ok(());
    }

    println!(
        "Clack will ask for administrator access to write {}:",
        access::rule_path().display()
    );
    println!();
    for line in access::rule_text().lines() {
        println!("    {line}");
    }
    println!();
    println!("It grants read access to keyboards to whoever is logged in on the active seat,");
    println!("and that access is revoked at logout.");
    println!();

    if declined(access::install())? {
        return Ok(());
    }

    println!("Done. Run clack --check to confirm.");
    Ok(())
}

fn revoke_access() -> anyhow::Result<()> {
    if access::status() == RuleStatus::Missing {
        println!("Clack has no keyboard permission to take away.");
        return Ok(());
    }

    println!(
        "Clack will ask for administrator access to delete {}.",
        access::rule_path().display()
    );
    println!("Afterwards it can no longer read your keyboard, and nothing of Clack's is left");
    println!("on the system outside your home directory.");
    println!();

    if declined(access::remove())? {
        return Ok(());
    }

    println!("Done. A copy of Clack already running keeps the devices it opened; stop it too.");
    Ok(())
}

/// Closing the prompt is an answer rather than a fault, so it leaves without a
/// message on stderr and without a failing exit code.
fn declined(outcome: Result<(), access::AccessError>) -> anyhow::Result<bool> {
    match outcome {
        Ok(()) => Ok(false),
        Err(access::AccessError::Declined(_)) => {
            println!("Nothing changed.");
            Ok(true)
        }
        Err(error) => Err(error.into()),
    }
}

fn check() -> anyhow::Result<()> {
    let report = Report::gather()?;

    println!("keyboards       {}", describe_devices(&report));
    match &report.compositor {
        CompositorReport::Unreachable(error) => {
            println!("overlay         unknown: {error}");
            println!("layout          unknown: {error}");
        }
        CompositorReport::Reachable {
            layer_shell,
            seat,
            keymap,
        } => {
            println!(
                "overlay         {}",
                if *layer_shell {
                    "can be drawn above other windows"
                } else {
                    "this desktop cannot show it; it has no layer shell"
                }
            );
            println!(
                "layout          {}",
                describe_keymap(keymap, seat.as_deref())
            );
        }
    }

    if report.is_usable() {
        return Ok(());
    }

    if report.keyboards() == 0 {
        list_devices()?;
        if report.denied() > 0 {
            println!();
            println!("Run clack --grant-access to allow reading them.");
        }
    }

    Err(anyhow::anyhow!("clack would not run here"))
}

fn describe_devices(report: &Report) -> String {
    match (report.keyboards(), report.denied()) {
        (0, 0) => "none found".to_owned(),
        (0, denied) => format!("none readable, {denied} refused permission"),
        (found, 0) => format!("{found} readable"),
        (found, denied) => format!("{found} readable, {denied} refused permission"),
    }
}

fn describe_keymap(keymap: &KeymapSource, seat: Option<&str>) -> String {
    let seat = seat.unwrap_or("unnamed seat");
    match keymap {
        KeymapSource::Compositor { bytes } => {
            format!("{bytes} bytes from this desktop, seat {seat}")
        }
        KeymapSource::Unreadable(error) => {
            format!("this desktop sent one but it could not be read: {error}")
        }
        KeymapSource::Absent => "this desktop shared none, so keys cannot be named".to_owned(),
    }
}

fn list_devices() -> anyhow::Result<()> {
    println!();
    for device in clack::input::survey()? {
        let name = device.name.unwrap_or_else(|| "unknown".to_owned());
        let access = match device.access {
            Access::Keyboard => "keyboard".to_owned(),
            Access::NotAKeyboard => "not a keyboard".to_owned(),
            Access::Denied => "permission denied".to_owned(),
            Access::Failed(error) => format!("unreadable: {error}"),
        };
        println!("{:<20} {access:<20} {name}", device.path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The library decides what polkit is handed; clap has to declare the same
    /// two strings, and a drift between them looks like a refused prompt.
    #[test]
    fn the_hidden_flags_are_the_ones_polkit_is_handed() {
        let allow = Cli::try_parse_from(["clack", access::Change::Allow.flag()]).unwrap();
        let revoke = Cli::try_parse_from(["clack", access::Change::Revoke.flag()]).unwrap();

        assert!(allow.write_udev_rule);
        assert!(revoke.remove_udev_rule);
    }
}
