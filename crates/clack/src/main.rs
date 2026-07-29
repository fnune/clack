use std::io::{self, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clack_core::{Clock, Frame, Pipeline, Surface, Timestamp};
use clap::Parser;

use clack::input::{self, Access, EvdevSource};
use clack::keymap::XkbTranslator;
use clack::render::{LayerOverlay, Style};
use clack::wayland;

#[derive(Debug, Parser)]
#[command(version, about = "Show the keys you press, on Wayland")]
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

fn main() -> ExitCode {
    let cli = Cli::parse();

    let outcome = if cli.list_devices {
        list_devices()
    } else if cli.check {
        check()
    } else if cli.print {
        print_keys(Duration::from_secs_f32(cli.timeout))
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

fn list_devices() -> anyhow::Result<()> {
    for device in input::survey()? {
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
    let devices = input::survey()?;
    let keyboards = devices
        .iter()
        .filter(|device| matches!(device.access, Access::Keyboard))
        .count();
    let denied = devices
        .iter()
        .filter(|device| matches!(device.access, Access::Denied))
        .count();

    let compositor = wayland::probe();

    println!("device access   {}", describe_devices(keyboards, denied));
    match &compositor {
        Ok(compositor) => {
            println!(
                "layer shell     {}",
                if compositor.layer_shell {
                    "available"
                } else {
                    "missing, the overlay cannot be drawn"
                }
            );
            println!("keymap          {}", describe_keymap(compositor));
        }
        Err(error) => {
            println!("layer shell     unknown: {error}");
            println!("keymap          unknown: {error}");
        }
    }

    let usable = keyboards > 0
        && compositor
            .as_ref()
            .is_ok_and(|compositor| compositor.layer_shell && compositor.keymap.is_some());

    if usable {
        Ok(())
    } else {
        Err(anyhow::anyhow!("clack would not run here"))
    }
}

fn describe_devices(keyboards: usize, denied: usize) -> String {
    match (keyboards, denied) {
        (0, 0) => "no keyboard found".to_owned(),
        (0, denied) => format!(
            "no readable keyboard, {denied} device(s) denied; install packaging/65-clack.rules"
        ),
        (found, 0) => format!("{found} keyboard(s) readable"),
        (found, denied) => format!("{found} keyboard(s) readable, {denied} denied"),
    }
}

fn describe_keymap(compositor: &wayland::Compositor) -> String {
    let seat = compositor.seat_name.as_deref().unwrap_or("unnamed seat");
    match (&compositor.keymap, &compositor.keymap_error) {
        (Some(keymap), _) => format!("{} bytes from the compositor, seat {seat}", keymap.len()),
        (None, Some(error)) => format!("the compositor sent one but it could not be read: {error}"),
        (None, None) => "the compositor sent none without keyboard focus".to_owned(),
    }
}

fn print_keys(timeout: Duration) -> anyhow::Result<()> {
    let clock = MonotonicClock::started();
    let mut pipeline = Pipeline::new(
        EvdevSource::open(clock)?,
        translator()?,
        StdoutSurface::default(),
        clock,
        timeout,
    );

    loop {
        pipeline.step()?;
    }
}

fn show_overlay(cli: &Cli) -> anyhow::Result<()> {
    let mut pipeline = Pipeline::new(
        EvdevSource::open(MonotonicClock::started())?,
        translator()?,
        LayerOverlay::open(Style {
            font: cli.font.clone(),
            margin: cli.margin,
            anchor_top: cli.top,
        })?,
        MonotonicClock::started(),
        Duration::from_secs_f32(cli.timeout),
    );

    while !pipeline.surface().closed() {
        pipeline.step()?;
    }

    Ok(())
}

fn translator() -> anyhow::Result<XkbTranslator> {
    let keymap = wayland::probe()?
        .keymap
        .ok_or_else(|| anyhow::anyhow!("the compositor did not send a keymap"))?;

    Ok(XkbTranslator::from_text(&keymap)?)
}

#[derive(Clone, Copy, Debug)]
struct MonotonicClock {
    start: Instant,
}

impl MonotonicClock {
    fn started() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Clock for MonotonicClock {
    fn now(&self) -> Timestamp {
        Timestamp::since_start(self.start.elapsed())
    }
}

#[derive(Debug, Default)]
struct StdoutSurface {
    last: String,
}

impl Surface for StdoutSurface {
    type Error = io::Error;

    fn present(&mut self, frame: &Frame) -> Result<(), Self::Error> {
        let line = frame.to_plain();
        if line == self.last {
            return Ok(());
        }

        self.last = line;
        if !self.last.is_empty() {
            let mut out = io::stdout().lock();
            writeln!(out, "{}", self.last)?;
            out.flush()?;
        }

        Ok(())
    }
}
