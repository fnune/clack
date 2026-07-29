use core::fmt;
use core::ops::RangeInclusive;

use clack_core::{KeyEvent, KeyState, Modifiers, Translated, Translator, XkbKeycode};
use xkbcommon::xkb;

const NO_SYMBOL: u32 = 0;

#[derive(Debug, thiserror::Error)]
pub enum KeymapError {
    #[error("the compositor sent a keymap that did not compile")]
    Invalid,
}

pub struct XkbTranslator {
    state: xkb::State,
}

impl XkbTranslator {
    pub fn from_text(text: &str) -> Result<Self, KeymapError> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_string(
            &context,
            text.to_owned(),
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::COMPILE_NO_FLAGS,
        )
        .ok_or(KeymapError::Invalid)?;

        Ok(Self {
            state: xkb::State::new(&keymap),
        })
    }

    fn held_modifiers(&self) -> Modifiers {
        Modifiers {
            ctrl: self.is_active(xkb::MOD_NAME_CTRL),
            alt: self.is_active(xkb::MOD_NAME_ALT),
            logo: self.is_active(xkb::MOD_NAME_LOGO),
            shift: self.is_active(xkb::MOD_NAME_SHIFT),
        }
    }

    fn is_active(&self, name: &str) -> bool {
        self.state
            .mod_name_is_active(name, xkb::STATE_MODS_EFFECTIVE)
    }
}

impl Translator for XkbTranslator {
    fn translate(&mut self, event: KeyEvent) -> Option<Translated> {
        let keycode = xkb::Keycode::new(XkbKeycode::from(event.code).raw());

        if event.state == KeyState::Released {
            self.state.update_key(keycode, xkb::KeyDirection::Up);
            return None;
        }

        let modifiers = self.held_modifiers();
        let symbol = self.state.key_get_one_sym(keycode);
        let text = self.state.key_get_utf8(keycode);

        if event.state == KeyState::Pressed {
            self.state.update_key(keycode, xkb::KeyDirection::Down);
        }

        if is_unmapped(symbol) || is_modifier(symbol) {
            return None;
        }

        Some(Translated {
            text: (!text.is_empty()).then_some(text),
            keysym: xkb::keysym_get_name(symbol),
            modifiers,
        })
    }
}

/// Codes the active keymap says nothing about, which some keyboards emit
/// alongside the keys they are paired with.
fn is_unmapped(symbol: xkb::Keysym) -> bool {
    symbol.raw() == NO_SYMBOL
}

/// Held on its own a modifier shows nothing: it appears as the prefix of the
/// chord it produces, once a real key completes that chord.
fn is_modifier(symbol: xkb::Keysym) -> bool {
    const SHIFT_THROUGH_HYPER: RangeInclusive<u32> = 0xffe1..=0xffee;
    const ISO_LEVEL3_SHIFT: u32 = 0xfe03;
    const ISO_LEVEL5_SHIFT: u32 = 0xfe11;
    const NUM_LOCK: u32 = 0xff7f;

    let raw = symbol.raw();
    SHIFT_THROUGH_HYPER.contains(&raw)
        || matches!(raw, ISO_LEVEL3_SHIFT | ISO_LEVEL5_SHIFT | NUM_LOCK)
}

impl fmt::Debug for XkbTranslator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XkbTranslator").finish_non_exhaustive()
    }
}
