use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::os::fd::OwnedFd;

use wayland_client::protocol::{wl_keyboard, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};

const LAYER_SHELL_INTERFACE: &str = "zwlr_layer_shell_v1";
const MAX_SEAT_VERSION: u32 = 8;
const ROUNDTRIPS_AWAITING_KEYMAP: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum WaylandError {
    #[error("connecting to the Wayland compositor failed: {0}")]
    Connect(#[from] wayland_client::ConnectError),
    #[error("talking to the Wayland compositor failed: {0}")]
    Dispatch(#[from] wayland_client::DispatchError),
}

#[derive(Debug, Default)]
pub struct Compositor {
    pub layer_shell: bool,
    pub seat_name: Option<String>,
    pub keymap: Option<String>,
    pub keymap_error: Option<io::Error>,
}

pub fn probe() -> Result<Compositor, WaylandError> {
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue();
    let handle = queue.handle();
    connection.display().get_registry(&handle, ());

    let mut state = State::default();
    queue.roundtrip(&mut state)?;

    for _ in 0..ROUNDTRIPS_AWAITING_KEYMAP {
        if state.keymap.is_some() || state.keymap_error.is_some() {
            break;
        }
        queue.roundtrip(&mut state)?;
    }

    Ok(Compositor {
        layer_shell: state.layer_shell,
        seat_name: state.seat_name,
        keymap: state.keymap,
        keymap_error: state.keymap_error,
    })
}

#[derive(Debug, Default)]
struct State {
    seat: Option<wl_seat::WlSeat>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    layer_shell: bool,
    seat_name: Option<String>,
    keymap: Option<String>,
    keymap_error: Option<io::Error>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };

        match interface.as_str() {
            LAYER_SHELL_INTERFACE => state.layer_shell = true,
            "wl_seat" if state.seat.is_none() => {
                let seat = registry.bind::<wl_seat::WlSeat, _, _>(
                    name,
                    version.min(MAX_SEAT_VERSION),
                    handle,
                    (),
                );
                state.seat = Some(seat);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_seat::Event::Name { name } => state.seat_name = Some(name),
            wl_seat::Event::Capabilities {
                capabilities: WEnum::Value(capabilities),
            } if capabilities.contains(wl_seat::Capability::Keyboard)
                && state.keyboard.is_none() =>
            {
                state.keyboard = Some(seat.get_keyboard(handle, ()));
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let wl_keyboard::Event::Keymap {
            format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1),
            fd,
            ..
        } = event
        else {
            return;
        };

        match read_keymap(fd) {
            Ok(text) => state.keymap = Some(text),
            Err(error) => state.keymap_error = Some(error),
        }
    }
}

fn read_keymap(fd: OwnedFd) -> io::Result<String> {
    let mut file = File::from(fd);
    file.seek(SeekFrom::Start(0))?;

    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text.trim_end_matches('\0').to_owned())
}
