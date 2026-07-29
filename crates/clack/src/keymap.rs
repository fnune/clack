use core::fmt;

use clack_core::{KeyEvent, KeyState, Modifiers, Translated, Translator, XkbKeycode};
use xkbcommon::xkb;

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
        let text = self.state.key_get_utf8(keycode);
        let keysym = xkb::keysym_get_name(self.state.key_get_one_sym(keycode));

        if event.state == KeyState::Pressed {
            self.state.update_key(keycode, xkb::KeyDirection::Down);
        }

        Some(Translated {
            text: (!text.is_empty()).then_some(text),
            keysym,
            modifiers,
        })
    }
}

impl fmt::Debug for XkbTranslator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XkbTranslator").finish_non_exhaustive()
    }
}
