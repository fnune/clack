#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::io::{Write, pipe};
use std::time::{Duration, Instant};

use clack::input::EvdevSource;
use clack_core::{Clock, EvdevCode, KeyEvent, KeySource, KeyState, Timestamp};

const TIMEVAL_SIZE: usize = 2 * size_of::<usize>();
const EVENT_SIZE: usize = TIMEVAL_SIZE + 8;
const EV_KEY: u16 = 0x01;
const KEY_H: u16 = 35;
const KEY_I: u16 = 23;
const DEADLINE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy)]
struct StartedNow(Instant);

impl Clock for StartedNow {
    fn now(&self) -> Timestamp {
        Timestamp::since_start(self.0.elapsed())
    }
}

#[test]
fn typing_reaches_the_key_source_in_order() {
    let (reader, mut writer) = pipe().expect("creating a pipe");
    let mut source = EvdevSource::from_streams(vec![reader], StartedNow(Instant::now()));

    for code in [KEY_H, KEY_I] {
        writer.write_all(&record(EV_KEY, code, 1)).unwrap();
        writer.write_all(&record(EV_KEY, code, 0)).unwrap();
    }
    writer.flush().unwrap();

    let events = collect(&mut source, 4);
    let pressed: Vec<EvdevCode> = events
        .iter()
        .filter(|event| event.state == KeyState::Pressed)
        .map(|event| event.code)
        .collect();

    assert_eq!(
        pressed,
        vec![EvdevCode::new(KEY_H), EvdevCode::new(KEY_I)],
        "expected the keys written to the stream, got {events:?}"
    );
}

#[test]
fn several_keyboards_merge_into_one_stream() {
    let (left_reader, mut left) = pipe().expect("creating a pipe");
    let (right_reader, mut right) = pipe().expect("creating a pipe");
    let mut source =
        EvdevSource::from_streams(vec![left_reader, right_reader], StartedNow(Instant::now()));

    left.write_all(&record(EV_KEY, KEY_H, 1)).unwrap();
    left.flush().unwrap();
    right.write_all(&record(EV_KEY, KEY_I, 1)).unwrap();
    right.flush().unwrap();

    let mut codes: Vec<u16> = collect(&mut source, 2)
        .iter()
        .map(|event| event.code.raw())
        .collect();
    codes.sort_unstable();

    assert_eq!(codes, vec![KEY_I, KEY_H]);
}

fn record(type_: u16, code: u16, value: i32) -> [u8; EVENT_SIZE] {
    let mut record = [0_u8; EVENT_SIZE];
    record[TIMEVAL_SIZE..TIMEVAL_SIZE + 2].copy_from_slice(&type_.to_ne_bytes());
    record[TIMEVAL_SIZE + 2..TIMEVAL_SIZE + 4].copy_from_slice(&code.to_ne_bytes());
    record[TIMEVAL_SIZE + 4..EVENT_SIZE].copy_from_slice(&value.to_ne_bytes());
    record
}

fn collect(source: &mut EvdevSource, wanted: usize) -> Vec<KeyEvent> {
    let deadline = Instant::now() + DEADLINE;
    let mut events = Vec::new();

    while events.len() < wanted && Instant::now() < deadline {
        events.extend(source.next_batch().expect("reading the source"));
    }

    events
}
