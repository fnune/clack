#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::io::{self, Write, pipe};
use std::sync::{Arc, Mutex};

use clack::clock::MonotonicClock;
use clack::input::EvdevSource;
use clack::keymap::XkbTranslator;
use clack::{Config, Session, TextSurface};

const US_KEYMAP: &str = include_str!("fixtures/us.xkb");
const TIMEVAL_SIZE: usize = 2 * size_of::<usize>();
const EVENT_SIZE: usize = TIMEVAL_SIZE + 8;
const EV_KEY: u16 = 0x01;
const H: u16 = 35;
const I: u16 = 23;
const STEPS: usize = 8;

/// Collects what the surface was told to show, so a test can read it back.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<u8>>>);

impl Recorder {
    fn shown(&self) -> String {
        String::from_utf8(self.0.lock().expect("the recorder lock").clone())
            .expect("the surface writes text")
    }
}

impl Write for Recorder {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .expect("the recorder lock")
            .extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn record(code: u16, value: i32) -> [u8; EVENT_SIZE] {
    let mut record = [0_u8; EVENT_SIZE];
    record[TIMEVAL_SIZE..TIMEVAL_SIZE + 2].copy_from_slice(&EV_KEY.to_ne_bytes());
    record[TIMEVAL_SIZE + 2..TIMEVAL_SIZE + 4].copy_from_slice(&code.to_ne_bytes());
    record[TIMEVAL_SIZE + 4..EVENT_SIZE].copy_from_slice(&value.to_ne_bytes());
    record
}

fn type_keys(writer: &mut impl Write, codes: &[u16]) {
    for code in codes {
        writer.write_all(&record(*code, 1)).unwrap();
        writer.write_all(&record(*code, 0)).unwrap();
    }
    writer.flush().unwrap();
}

fn session(
    reader: impl io::Read + Send + 'static,
    surface: Recorder,
) -> Session<EvdevSource, XkbTranslator, TextSurface<Recorder>> {
    Session::from_parts(
        EvdevSource::from_streams(vec![reader], MonotonicClock::started()),
        XkbTranslator::from_text(US_KEYMAP).expect("compiling the fixture keymap"),
        TextSurface::new(surface),
        &Config::default(),
    )
}

#[test]
fn keys_read_from_a_device_reach_the_surface_as_text() {
    let (reader, mut writer) = pipe().expect("creating a pipe");
    let recorder = Recorder::default();
    let mut session = session(reader, recorder.clone());

    type_keys(&mut writer, &[H, I]);
    for _ in 0..STEPS {
        session.step().expect("stepping the session");
    }

    assert!(
        recorder.shown().contains("hi"),
        "expected the typed keys, got {:?}",
        recorder.shown()
    );
}

#[test]
fn a_paused_session_shows_nothing_and_keeps_nothing() {
    let (reader, mut writer) = pipe().expect("creating a pipe");
    let recorder = Recorder::default();
    let mut session = session(reader, recorder.clone());
    let handle = session.handle();

    handle.pause();
    type_keys(&mut writer, &[H, I]);
    for _ in 0..STEPS {
        session.step().expect("stepping the session");
    }
    assert_eq!(recorder.shown(), "", "a paused session shows nothing");

    handle.resume();
    for _ in 0..STEPS {
        session.step().expect("stepping the session");
    }
    assert_eq!(
        recorder.shown(),
        "",
        "keys typed while paused are not shown once resumed"
    );
}

#[test]
fn a_stopped_session_stops_running() {
    let (reader, _writer) = pipe().expect("creating a pipe");
    let mut session = session(reader, Recorder::default());

    session.handle().stop();
    session.run().expect("running a stopped session returns");
}
