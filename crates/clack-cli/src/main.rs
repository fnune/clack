use std::io;
use std::process::ExitCode;
use std::time::Duration;

use clack::diagnostics::{CompositorReport, KeymapSource};
use clack::input::Access;
use clack::{Anchor, Config, Report, Session, Style, TextSurface};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "clack", version, about = "Show the keys you press, on Wayland")]
struct Cli {
    /// Report device access, compositor support, and keymap source, then exit
    #[arg(long)]
    check: bool,

    /// List input devices and whether Clack can read them, then exit
    #[arg(long)]
    list_devices: bool,

    /// Print keystrokes to stdout instead of drawing an overlay
    #[arg(long)]
    print: bool,

    /// How long a keystroke stays on screen, in seconds
    #[arg(long, value_name = "SECONDS", default_value_t = 1.5)]
    timeout: f32,

    /// Font, in Pango notation
    #[arg(long, value_name = "FONT", default_value = "Sans Bold 24")]
    font: String,

    /// Distance from the anchored screen edge, in pixels
    #[arg(long, value_name = "PIXELS", default_value_t = 96)]
    margin: i32,

    /// Anchor to the top edge instead of the bottom
    #[arg(long)]
    top: bool,
}

impl Cli {
    fn config(&self) -> Config {
        Config {
            timeout: Duration::from_secs_f32(self.timeout),
            style: Style {
                font: self.font.clone(),
                margin: self.margin,
                anchor: if self.top {
                    Anchor::Top
                } else {
                    Anchor::Bottom
                },
            },
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let outcome = if cli.list_devices {
        list_devices()
    } else if cli.check {
        check()
    } else if cli.print {
        print_keys(&cli.config())
    } else {
        show_overlay(&cli.config())
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("clack: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn show_overlay(config: &Config) -> anyhow::Result<()> {
    Session::overlay(config)?.run()?;
    Ok(())
}

fn print_keys(config: &Config) -> anyhow::Result<()> {
    Session::with_surface(TextSurface::new(io::stdout()), config)?.run()?;
    Ok(())
}

fn list_devices() -> anyhow::Result<()> {
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

fn check() -> anyhow::Result<()> {
    let report = Report::gather()?;

    println!("device access   {}", describe_devices(&report));
    match &report.compositor {
        CompositorReport::Unreachable(error) => {
            println!("layer shell     unknown: {error}");
            println!("keymap          unknown: {error}");
        }
        CompositorReport::Reachable {
            layer_shell,
            seat,
            keymap,
        } => {
            println!(
                "layer shell     {}",
                if *layer_shell {
                    "available"
                } else {
                    "missing, the overlay cannot be drawn"
                }
            );
            println!(
                "keymap          {}",
                describe_keymap(keymap, seat.as_deref())
            );
        }
    }

    if report.is_usable() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("clack would not run here"))
    }
}

fn describe_devices(report: &Report) -> String {
    match (report.keyboards(), report.denied()) {
        (0, 0) => "no keyboard found".to_owned(),
        (0, denied) => format!(
            "no readable keyboard, {denied} device(s) denied; install packaging/65-clack.rules"
        ),
        (found, 0) => format!("{found} keyboard(s) readable"),
        (found, denied) => format!("{found} keyboard(s) readable, {denied} denied"),
    }
}

fn describe_keymap(keymap: &KeymapSource, seat: Option<&str>) -> String {
    let seat = seat.unwrap_or("unnamed seat");
    match keymap {
        KeymapSource::Compositor { bytes } => {
            format!("{bytes} bytes from the compositor, seat {seat}")
        }
        KeymapSource::Unreadable(error) => {
            format!("the compositor sent one but it could not be read: {error}")
        }
        KeymapSource::Absent => "the compositor sent none without keyboard focus".to_owned(),
    }
}
