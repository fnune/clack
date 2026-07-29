#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::time::Duration;

use clack::render::{LayerOverlay, Style};
use clack_core::{Frame, Run, RunKind, Surface};

fn style() -> Style {
    Style {
        font: "Sans Bold 24".to_owned(),
        margin: 96,
        anchor_top: false,
    }
}

fn sample() -> Frame {
    Frame {
        runs: vec![
            run("c", RunKind::Text),
            run("l", RunKind::Text),
            run("a", RunKind::Text),
            run("Return", RunKind::Special),
            run("Ctrl+S", RunKind::Chord),
        ],
    }
}

fn run(text: &str, kind: RunKind) -> Run {
    Run {
        text: text.to_owned(),
        kind,
    }
}

#[test]
fn a_frame_reaches_the_compositor() {
    let Ok(mut overlay) = LayerOverlay::open(style()) else {
        eprintln!("skipped: no Wayland compositor with layer-shell support");
        return;
    };

    overlay.present(&sample()).expect("presenting a frame");
    overlay
        .present(&Frame::default())
        .expect("hiding the overlay");
}

/// Manual check: leaves the overlay on screen long enough to look at it.
#[test]
#[ignore]
fn shows_a_frame_for_a_few_seconds() {
    let mut overlay = LayerOverlay::open(style()).expect("opening the overlay");

    for _ in 0..60 {
        overlay.present(&sample()).expect("presenting a frame");
        std::thread::sleep(Duration::from_millis(100));
    }
}
