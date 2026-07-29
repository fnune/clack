use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::thread;
use std::time::Duration;
use std::{fs, io};

use clack_core::{Clock, EvdevCode, KeyEvent, KeySource, KeyState};
use evdev::{Device, KeyCode};

const DEVICE_DIR: &str = "/dev/input";

/// Layout of the kernel's `struct input_event`: a `timeval` of two C longs,
/// then `type`, `code`, and `value`.
const TIMEVAL_SIZE: usize = 2 * size_of::<usize>();
const TYPE_OFFSET: usize = TIMEVAL_SIZE;
const CODE_OFFSET: usize = TYPE_OFFSET + 2;
const VALUE_OFFSET: usize = CODE_OFFSET + 2;
const EVENT_SIZE: usize = VALUE_OFFSET + 4;
const EV_KEY: u16 = 0x01;

const EXPIRY_TICK: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub struct DeviceReport {
    pub path: PathBuf,
    pub name: Option<String>,
    pub access: Access,
}

#[derive(Debug)]
pub enum Access {
    Keyboard,
    NotAKeyboard,
    Denied,
    Failed(io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("no readable keyboard found under {DEVICE_DIR}")]
    NoKeyboards,
    #[error("every keyboard reader stopped")]
    Disconnected,
    #[error("listing {DEVICE_DIR} failed: {0}")]
    Listing(#[from] io::Error),
    #[error("opening {0} failed: {1}")]
    Opening(String, #[source] io::Error),
}

pub fn survey() -> Result<Vec<DeviceReport>, SourceError> {
    let mut reports = Vec::new();

    for path in event_device_paths()? {
        let report = match Device::open(&path) {
            Ok(device) => DeviceReport {
                name: device.name().map(str::to_owned),
                access: if is_keyboard(&device) {
                    Access::Keyboard
                } else {
                    Access::NotAKeyboard
                },
                path,
            },
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => DeviceReport {
                path,
                name: None,
                access: Access::Denied,
            },
            Err(error) => DeviceReport {
                path,
                name: None,
                access: Access::Failed(error),
            },
        };
        reports.push(report);
    }

    reports.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(reports)
}

fn event_device_paths() -> Result<Vec<PathBuf>, SourceError> {
    let mut paths = Vec::new();

    for entry in fs::read_dir(Path::new(DEVICE_DIR))? {
        let path = entry?.path();
        let is_event_device = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("event"));
        if is_event_device {
            paths.push(path);
        }
    }

    Ok(paths)
}

fn is_keyboard(device: &Device) -> bool {
    // Power buttons and lid switches report key codes too, so EV_KEY alone would match them.
    const TYPING_KEYS: [KeyCode; 3] = [KeyCode::KEY_A, KeyCode::KEY_Z, KeyCode::KEY_SPACE];

    device
        .supported_keys()
        .is_some_and(|keys| TYPING_KEYS.into_iter().all(|key| keys.contains(key)))
}

#[derive(Debug)]
pub struct EvdevSource {
    events: Receiver<KeyEvent>,
}

impl EvdevSource {
    pub fn open<C: Clock + Clone + Send + 'static>(clock: C) -> Result<Self, SourceError> {
        let paths: Vec<PathBuf> = survey()?
            .into_iter()
            .filter(|report| matches!(report.access, Access::Keyboard))
            .map(|report| report.path)
            .collect();

        Self::open_paths(&paths, clock)
    }

    pub fn open_paths<C: Clock + Clone + Send + 'static>(
        paths: &[PathBuf],
        clock: C,
    ) -> Result<Self, SourceError> {
        let mut streams = Vec::new();
        for path in paths {
            let failed = |error| SourceError::Opening(path.display().to_string(), error);
            if is_keyboard(&Device::open(path).map_err(failed)?) {
                streams.push(File::open(path).map_err(failed)?);
            }
        }

        if streams.is_empty() {
            return Err(SourceError::NoKeyboards);
        }

        Ok(Self::from_streams(streams, clock))
    }

    /// Reads `struct input_event` records from anything, so the reader can be
    /// exercised over a pipe instead of a kernel device.
    pub fn from_streams<R, C>(streams: Vec<R>, clock: C) -> Self
    where
        R: Read + Send + 'static,
        C: Clock + Clone + Send + 'static,
    {
        let (sender, events) = channel();
        for stream in streams {
            let sender = sender.clone();
            let clock = clock.clone();
            thread::spawn(move || pump(stream, &sender, &clock));
        }

        Self { events }
    }
}

fn pump<R: Read, C: Clock>(mut stream: R, sender: &Sender<KeyEvent>, clock: &C) {
    let mut record = [0_u8; EVENT_SIZE];

    while stream.read_exact(&mut record).is_ok() {
        let Some((code, state)) = decode(&record) else {
            continue;
        };
        let key = KeyEvent {
            code,
            state,
            at: clock.now(),
        };
        if sender.send(key).is_err() {
            return;
        }
    }
}

fn decode(record: &[u8; EVENT_SIZE]) -> Option<(EvdevCode, KeyState)> {
    let field = |offset: usize| record.get(offset..offset + 2)?.try_into().ok();
    if u16::from_ne_bytes(field(TYPE_OFFSET)?) != EV_KEY {
        return None;
    }

    let code = EvdevCode::new(u16::from_ne_bytes(field(CODE_OFFSET)?));
    let raw = record.get(VALUE_OFFSET..EVENT_SIZE)?.try_into().ok()?;
    Some((code, key_state(i32::from_ne_bytes(raw))?))
}

const fn key_state(value: i32) -> Option<KeyState> {
    match value {
        0 => Some(KeyState::Released),
        1 => Some(KeyState::Pressed),
        2 => Some(KeyState::Repeated),
        _ => None,
    }
}

impl KeySource for EvdevSource {
    type Error = SourceError;

    fn next_batch(&mut self) -> Result<Vec<KeyEvent>, Self::Error> {
        let mut batch = match self.events.recv_timeout(EXPIRY_TICK) {
            Ok(event) => vec![event],
            Err(RecvTimeoutError::Timeout) => return Ok(Vec::new()),
            Err(RecvTimeoutError::Disconnected) => return Err(SourceError::Disconnected),
        };

        loop {
            match self.events.try_recv() {
                Ok(event) => batch.push(event),
                Err(TryRecvError::Empty) => return Ok(batch),
                Err(TryRecvError::Disconnected) => return Ok(batch),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(type_: u16, code: u16, value: i32) -> [u8; EVENT_SIZE] {
        let mut record = [0_u8; EVENT_SIZE];
        record[TYPE_OFFSET..CODE_OFFSET].copy_from_slice(&type_.to_ne_bytes());
        record[CODE_OFFSET..VALUE_OFFSET].copy_from_slice(&code.to_ne_bytes());
        record[VALUE_OFFSET..EVENT_SIZE].copy_from_slice(&value.to_ne_bytes());
        record
    }

    #[test]
    fn a_key_press_decodes_to_its_code_and_state() {
        let decoded = decode(&record(EV_KEY, 30, 1));

        assert_eq!(decoded, Some((EvdevCode::new(30), KeyState::Pressed)));
    }

    #[test]
    fn events_that_are_not_key_events_decode_to_nothing() {
        const EV_REL: u16 = 0x02;

        assert_eq!(decode(&record(EV_REL, 0, 1)), None);
    }

    #[test]
    fn key_values_outside_the_known_states_decode_to_nothing() {
        assert_eq!(decode(&record(EV_KEY, 30, 7)), None);
    }
}
