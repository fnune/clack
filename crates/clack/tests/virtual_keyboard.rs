#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clack::input::EvdevSource;
use clack_core::{Clock, EvdevCode, KeyEvent, KeySource, KeyState, Timestamp};
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};

const TYPED: [KeyCode; 2] = [KeyCode::KEY_H, KeyCode::KEY_I];
const DEADLINE: Duration = Duration::from_secs(5);

/// udev's input_id builtin only sets ID_INPUT_KEYBOARD, which our uaccess rule
/// keys off, for a device carrying every code from KEY_ESC to KEY_D.
const FULL_KEYBOARD: std::ops::RangeInclusive<u16> = 1..=32;

#[derive(Clone, Copy)]
struct StartedNow(Instant);

impl Clock for StartedNow {
    fn now(&self) -> Timestamp {
        Timestamp::since_start(self.0.elapsed())
    }
}

/// Drives the real evdev path with a kernel-level virtual keyboard, so the
/// reader, the device filter, and the batching are exercised as shipped.
#[test]
fn a_virtual_keyboard_reaches_the_key_source() {
    let mut keys = AttributeSet::<KeyCode>::new();
    for code in FULL_KEYBOARD {
        keys.insert(KeyCode(code));
    }
    for key in [KeyCode::KEY_A, KeyCode::KEY_Z, KeyCode::KEY_SPACE] {
        keys.insert(key);
    }
    for key in TYPED {
        keys.insert(key);
    }

    let built = VirtualDevice::builder()
        .and_then(|builder| builder.name("clack test keyboard").with_keys(&keys))
        .and_then(evdev::uinput::VirtualDeviceBuilder::build);

    let Ok(mut device) = built else {
        eprintln!("skipped: /dev/uinput is not writable by this user");
        return;
    };

    let paths: Vec<PathBuf> = device
        .enumerate_dev_nodes_blocking()
        .expect("listing the virtual device nodes")
        .filter_map(Result::ok)
        .collect();

    let mut source = open_once_udev_grants_access(&paths);

    for key in TYPED {
        let press = InputEvent::new(EventType::KEY.0, key.0, 1);
        let release = InputEvent::new(EventType::KEY.0, key.0, 0);
        device.emit(&[press, release]).expect("emitting key events");
    }

    let events = collect(&mut source, TYPED.len() * 2);
    let pressed: Vec<EvdevCode> = events
        .iter()
        .filter(|event| event.state == KeyState::Pressed)
        .map(|event| event.code)
        .collect();

    assert_eq!(
        pressed,
        TYPED.map(|key| EvdevCode::new(key.0)).to_vec(),
        "expected the keys emitted through uinput, got {events:?}"
    );
}

/// udev applies the uaccess ACL to a freshly created device asynchronously, so
/// the first open of a virtual keyboard usually loses the race.
fn open_once_udev_grants_access(paths: &[PathBuf]) -> EvdevSource {
    let deadline = Instant::now() + DEADLINE;
    let mut last = None;

    while Instant::now() < deadline {
        match EvdevSource::open_paths(paths, StartedNow(Instant::now())) {
            Ok(source) => return source,
            Err(error) => last = Some(error),
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    panic!("the virtual keyboard never became readable: {last:?}");
}

fn collect(source: &mut EvdevSource, wanted: usize) -> Vec<KeyEvent> {
    let deadline = Instant::now() + DEADLINE;
    let mut events = Vec::new();

    while events.len() < wanted && Instant::now() < deadline {
        events.extend(source.next_batch().expect("reading the source"));
    }

    events
}
