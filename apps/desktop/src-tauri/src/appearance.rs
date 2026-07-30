//! What the desktop looks like, so the window can match it rather than impose
//! a look of its own.

use std::collections::HashMap;

use serde::Serialize;
use zbus::blocking::{Connection, MessageIterator};
use zbus::zvariant::OwnedValue;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";
const CHANGED: &str = "SettingChanged";

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    /// The desktop's accent colour as `#rrggbb`, when it has one.
    pub accent: Option<String>,
    /// Whether the desktop prefers a dark palette, when it has said.
    pub dark: Option<bool>,
}

/// Reads the appearance portal, which Plasma, GNOME, and anything else with a
/// portal implement. Anything missing stays `None`, and the window falls back
/// to its own defaults rather than guessing.
pub fn read() -> Appearance {
    let Some(settings) = query() else {
        return Appearance::default();
    };

    Appearance {
        accent: settings.get("accent-color").and_then(as_hex),
        dark: settings.get("color-scheme").and_then(as_dark),
    }
}

/// Calls back whenever the desktop changes how it looks, on a thread of its own.
///
/// A palette is not read once and kept: someone who switches to dark at sunset is
/// left with a window dressed for the morning and a tray icon in a colour the
/// panel no longer paints with. The portal says when it changes, so Clack listens
/// rather than asking again and again.
pub fn watch(changed: impl Fn(Appearance) + Send + 'static) {
    std::thread::spawn(move || {
        let Some(signals) = subscribe() else {
            eprintln!("clack-desktop: the desktop will not say when it changes appearance");
            return;
        };

        let mut appearance = read();

        for signal in signals.flatten() {
            let Ok((namespace, key, value)) =
                signal.body().deserialize::<(String, String, OwnedValue)>()
            else {
                continue;
            };

            if namespace != NAMESPACE {
                continue;
            }

            // The signal carries the new value. Asking the portal again instead
            // races it, and answers with the setting that has just been replaced.
            match key.as_str() {
                "color-scheme" => appearance.dark = as_dark(&value),
                "accent-color" => appearance.accent = as_hex(&value),
                _ => continue,
            }

            changed(appearance.clone());
        }
    });
}

fn subscribe() -> Option<MessageIterator> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(PORTAL)
        .ok()?
        .path(PATH)
        .ok()?
        .interface(SETTINGS)
        .ok()?
        .member(CHANGED)
        .ok()?
        .build();

    MessageIterator::for_match_rule(rule, &Connection::session().ok()?, None).ok()
}

fn query() -> Option<HashMap<String, OwnedValue>> {
    let connection = Connection::session().ok()?;
    let reply = connection
        .call_method(
            Some(PORTAL),
            PATH,
            Some(SETTINGS),
            "ReadAll",
            &([NAMESPACE],),
        )
        .ok()?;

    let namespaces: HashMap<String, HashMap<String, OwnedValue>> =
        reply.body().deserialize().ok()?;

    namespaces.get(NAMESPACE).cloned()
}

/// The portal reports the accent as three floats in 0..1.
fn as_hex(value: &OwnedValue) -> Option<String> {
    let channels: (f64, f64, f64) = value.try_clone().ok()?.try_into().ok()?;
    let byte = |channel: f64| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;

    Some(format!(
        "#{:02x}{:02x}{:02x}",
        byte(channels.0),
        byte(channels.1),
        byte(channels.2)
    ))
}

/// 0 means no preference, 1 dark, 2 light.
fn as_dark(value: &OwnedValue) -> Option<bool> {
    match u32::try_from(value.try_clone().ok()?).ok()? {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}
