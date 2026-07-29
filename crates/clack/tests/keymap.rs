#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use clack::keymap::XkbTranslator;
use clack_core::{EvdevCode, KeyEvent, KeyState, Timestamp, Translated, Translator};

const US_KEYMAP: &str = include_str!("fixtures/us.xkb");

const A: u16 = 30;
const C: u16 = 46;
const ENTER: u16 = 28;
const LEFT_CTRL: u16 = 29;
const LEFT_SHIFT: u16 = 42;
const UNMAPPED: u16 = 250;

fn translator() -> XkbTranslator {
    XkbTranslator::from_text(US_KEYMAP).expect("compiling the fixture keymap")
}

fn event(code: u16, state: KeyState) -> KeyEvent {
    KeyEvent {
        code: EvdevCode::new(code),
        state,
        at: Timestamp::ZERO,
    }
}

fn press(translator: &mut XkbTranslator, code: u16) -> Option<Translated> {
    translator.translate(event(code, KeyState::Pressed))
}

fn release(translator: &mut XkbTranslator, code: u16) -> Option<Translated> {
    translator.translate(event(code, KeyState::Released))
}

#[test]
fn a_letter_key_produces_that_letter() {
    let typed = press(&mut translator(), A).expect("a produces something");

    assert_eq!(typed.text.as_deref(), Some("a"));
    assert_eq!(typed.keysym, "a");
    assert!(!typed.modifiers.is_chord());
}

#[test]
fn holding_shift_produces_the_capital_letter() {
    let mut translator = translator();

    assert_eq!(press(&mut translator, LEFT_SHIFT), None);
    let typed = press(&mut translator, A).expect("shift and a produce something");

    assert_eq!(typed.text.as_deref(), Some("A"));
    assert!(typed.modifiers.shift);
    assert!(
        !typed.modifiers.is_chord(),
        "shift alone is typing, not a shortcut"
    );
}

#[test]
fn holding_control_makes_the_next_key_a_chord() {
    let mut translator = translator();

    assert_eq!(press(&mut translator, LEFT_CTRL), None);
    let chord = press(&mut translator, C).expect("ctrl and c produce something");

    assert_eq!(chord.keysym, "c");
    assert!(chord.modifiers.ctrl);
    assert!(chord.modifiers.is_chord());
}

#[test]
fn a_modifier_stops_applying_once_released() {
    let mut translator = translator();

    press(&mut translator, LEFT_SHIFT);
    release(&mut translator, LEFT_SHIFT);
    let typed = press(&mut translator, A).expect("a produces something");

    assert_eq!(typed.text.as_deref(), Some("a"));
    assert!(!typed.modifiers.shift);
}

#[test]
fn a_key_with_no_printable_output_keeps_its_name() {
    let typed = press(&mut translator(), ENTER).expect("enter produces something");

    assert_eq!(typed.keysym, "Return");
}

#[test]
fn a_code_the_keymap_says_nothing_about_shows_nothing() {
    assert_eq!(press(&mut translator(), UNMAPPED), None);
}

#[test]
fn releases_show_nothing() {
    let mut translator = translator();
    press(&mut translator, A);

    assert_eq!(release(&mut translator, A), None);
}
