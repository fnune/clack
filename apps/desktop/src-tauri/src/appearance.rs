//! What the desktop looks like, so the window can match it rather than impose
//! a look of its own.

use std::collections::HashMap;

use serde::Serialize;
use zbus::blocking::Connection;
use zbus::zvariant::OwnedValue;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";

#[derive(Debug, Default, Serialize)]
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
