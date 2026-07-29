use crate::input::{self, Access, DeviceReport, SourceError};
use crate::wayland;

/// Everything `clack --check` needs, and everything a settings dialog would
/// need to explain why Clack cannot run.
#[derive(Debug)]
pub struct Report {
    pub devices: Vec<DeviceReport>,
    pub compositor: CompositorReport,
}

#[derive(Debug)]
pub enum CompositorReport {
    Reachable {
        layer_shell: bool,
        seat: Option<String>,
        keymap: KeymapSource,
    },
    Unreachable(String),
}

#[derive(Debug)]
pub enum KeymapSource {
    Compositor { bytes: usize },
    Unreadable(String),
    Absent,
}

impl Report {
    pub fn gather() -> Result<Self, SourceError> {
        let compositor = match wayland::probe() {
            Err(error) => CompositorReport::Unreachable(error.to_string()),
            Ok(compositor) => CompositorReport::Reachable {
                layer_shell: compositor.layer_shell,
                seat: compositor.seat_name,
                keymap: match (compositor.keymap, compositor.keymap_error) {
                    (Some(keymap), _) => KeymapSource::Compositor {
                        bytes: keymap.len(),
                    },
                    (None, Some(error)) => KeymapSource::Unreadable(error.to_string()),
                    (None, None) => KeymapSource::Absent,
                },
            },
        };

        Ok(Self {
            devices: input::survey()?,
            compositor,
        })
    }

    pub fn keyboards(&self) -> usize {
        self.count(|access| matches!(access, Access::Keyboard))
    }

    pub fn denied(&self) -> usize {
        self.count(|access| matches!(access, Access::Denied))
    }

    pub fn can_draw(&self) -> bool {
        matches!(
            self.compositor,
            CompositorReport::Reachable {
                layer_shell: true,
                ..
            }
        )
    }

    pub fn can_translate(&self) -> bool {
        matches!(
            &self.compositor,
            CompositorReport::Reachable {
                keymap: KeymapSource::Compositor { .. },
                ..
            }
        )
    }

    pub fn is_usable(&self) -> bool {
        self.keyboards() > 0 && self.can_draw() && self.can_translate()
    }

    fn count(&self, wanted: impl Fn(&Access) -> bool) -> usize {
        self.devices
            .iter()
            .filter(|device| wanted(&device.access))
            .count()
    }
}
