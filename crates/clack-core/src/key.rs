use crate::clock::Timestamp;

/// XKB reserves keycodes 0 to 7, so every kernel code sits eight places higher
/// in an XKB keymap.
const XKB_EVDEV_OFFSET: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvdevCode(u16);

impl EvdevCode {
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct XkbKeycode(u32);

impl XkbKeycode {
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl From<EvdevCode> for XkbKeycode {
    fn from(code: EvdevCode) -> Self {
        Self(u32::from(code.raw()) + XKB_EVDEV_OFFSET)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyState {
    Pressed,
    Released,
    Repeated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: EvdevCode,
    pub state: KeyState,
    pub at: Timestamp,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
    pub shift: bool,
}

impl Modifiers {
    /// Shift is excluded: it produces text rather than a shortcut.
    pub const fn is_chord(self) -> bool {
        self.ctrl || self.alt || self.logo
    }

    pub fn names(self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.ctrl {
            names.push("Ctrl");
        }
        if self.alt {
            names.push("Alt");
        }
        if self.logo {
            names.push("Super");
        }
        if self.shift {
            names.push("Shift");
        }
        names
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Translated {
    pub text: Option<String>,
    pub keysym: String,
    /// Held when the key was pressed, so never including the key itself.
    pub modifiers: Modifiers,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evdev_codes_shift_into_the_xkb_range() {
        // KEY_A is 30 in the kernel headers and 38 in an XKB keymap.
        assert_eq!(XkbKeycode::from(EvdevCode::new(30)).raw(), 38);
    }

    #[test]
    fn shift_alone_is_not_a_chord() {
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert!(!shift.is_chord());

        let ctrl_shift = Modifiers {
            ctrl: true,
            ..shift
        };
        assert!(ctrl_shift.is_chord());
        assert_eq!(ctrl_shift.names(), vec!["Ctrl", "Shift"]);
    }
}
