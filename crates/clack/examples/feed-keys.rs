//! Writes `input_event` records to a path, so an end-to-end test can type at
//! Clack without creating a kernel device. A uinput device would be a real
//! keyboard and the compositor would deliver its keys to the focused window.

#![allow(clippy::expect_used, clippy::print_stderr)]

use std::io::Write;

use std::time::Duration;
use std::{env, fs, process, thread};

const TIMEVAL_SIZE: usize = 2 * size_of::<usize>();
const EVENT_SIZE: usize = TIMEVAL_SIZE + 8;
const EV_KEY: u16 = 0x01;
const BETWEEN_KEYS: Duration = Duration::from_millis(60);

fn main() {
    let mut arguments = env::args().skip(1);
    let Some(path) = arguments.next() else {
        eprintln!("usage: feed-keys <path> <key>...");
        process::exit(2);
    };

    let codes: Vec<u16> = arguments
        .map(|key| {
            code(&key).unwrap_or_else(|| {
                eprintln!("unknown key {key}");
                process::exit(2);
            })
        })
        .collect();
    let mut sink = fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("opening the destination");

    for code in codes {
        for value in [1, 0] {
            sink.write_all(&record(code, value)).expect("writing a key");
        }
        sink.flush().expect("flushing");
        thread::sleep(BETWEEN_KEYS);
    }
}

fn record(code: u16, value: i32) -> [u8; EVENT_SIZE] {
    let mut record = [0_u8; EVENT_SIZE];
    record[TIMEVAL_SIZE..TIMEVAL_SIZE + 2].copy_from_slice(&EV_KEY.to_ne_bytes());
    record[TIMEVAL_SIZE + 2..TIMEVAL_SIZE + 4].copy_from_slice(&code.to_ne_bytes());
    record[TIMEVAL_SIZE + 4..EVENT_SIZE].copy_from_slice(&value.to_ne_bytes());
    record
}

/// Accepts a kernel key code, or a single letter for readability.
fn code(key: &str) -> Option<u16> {
    const LETTERS: [(char, u16); 26] = [
        ('a', 30),
        ('b', 48),
        ('c', 46),
        ('d', 32),
        ('e', 18),
        ('f', 33),
        ('g', 34),
        ('h', 35),
        ('i', 23),
        ('j', 36),
        ('k', 37),
        ('l', 38),
        ('m', 50),
        ('n', 49),
        ('o', 24),
        ('p', 25),
        ('q', 16),
        ('r', 19),
        ('s', 31),
        ('t', 20),
        ('u', 22),
        ('v', 47),
        ('w', 17),
        ('x', 45),
        ('y', 21),
        ('z', 44),
    ];

    if let Ok(code) = key.parse() {
        return Some(code);
    }

    let letter = key.chars().next()?;
    LETTERS
        .iter()
        .find(|(candidate, _)| *candidate == letter)
        .map(|(_, code)| *code)
}
