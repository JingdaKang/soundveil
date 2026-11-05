//! Global shortcuts on Wayland, where an app can't take keys for itself and has to ask the desktop.
//!
//! The desktop shows its own dialog and decides the final keys, so Settings only lists them. GNOME
//! offers this from version 48 and KDE from 5.27. Older desktops have no way to do it at all.

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;
use soundveil_core::{Shortcut, ShortcutAction, ShortcutMap};
use std::thread;

/// The keys as the desktop describes them, or why there are none.
pub type Outcome = Result<Vec<(ShortcutAction, String)>, String>;

/// A key in the form the shortcuts specification uses, such as `CTRL+ALT+SHIFT+s`.
fn trigger(shortcut: &Shortcut, keyboard: &soundveil_core::Keyboard) -> String {
    let mut parts = Vec::new();
    for (bit, name) in [
        (keyboard.control, "CTRL"),
        (keyboard.alt, "ALT"),
        (keyboard.shift, "SHIFT"),
        (keyboard.system, "LOGO"),
    ] {
        if shortcut.modifiers & bit != 0 {
            parts.push(name.to_string());
        }
    }
    parts.push(match shortcut.key.as_str() {
        "=" => "equal".to_string(),
        "-" => "minus".to_string(),
        key => key.to_lowercase(),
    });
    parts.join("+")
}

fn action(id: &str) -> Option<ShortcutAction> {
    ShortcutAction::ALL
        .into_iter()
        .find(|action| action.raw_value() == id)
}

async fn run(
    map: ShortcutMap,
    report: &(dyn Fn(Outcome) + Send + Sync),
    fire: &(dyn Fn(ShortcutAction) + Send + Sync),
) -> ashpd::Result<()> {
    let keyboard = soundveil_core::Keyboard::LINUX;
    let proxy = GlobalShortcuts::new().await?;
    let session = proxy.create_session(Default::default()).await?;
    let triggers: Vec<(ShortcutAction, String)> = map
        .iter()
        .map(|(action, shortcut)| (action, trigger(shortcut, &keyboard)))
        .collect();
    let wanted: Vec<NewShortcut> = triggers
        .iter()
        .map(|(action, keys)| {
            NewShortcut::new(action.raw_value(), action.title()).preferred_trigger(keys.as_str())
        })
        .collect();
    let bound = proxy
        .bind_shortcuts(&session, &wanted, None, Default::default())
        .await?
        .response()?;
    report(Ok(bound
        .shortcuts()
        .iter()
        .filter_map(|shortcut| {
            let keys = shortcut.trigger_description().to_string();
            Some((action(shortcut.id())?, keys))
        })
        .collect()));
    let mut pressed = proxy.receive_activated().await?;
    while let Some(event) = pressed.next().await {
        if let Some(action) = action(event.shortcut_id()) {
            fire(action);
        }
    }
    Ok(())
}

/// Asks the desktop for shortcuts. It stays connected for the life of the app.
pub fn connect(
    map: ShortcutMap,
    report: impl Fn(Outcome) + Send + Sync + 'static,
    fire: impl Fn(ShortcutAction) + Send + Sync + 'static,
) {
    thread::spawn(move || {
        if let Err(error) = pollster::block_on(run(map, &report, &fire)) {
            report(Err(error.to_string()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use soundveil_core::Keyboard;

    #[test]
    fn triggers_use_the_specifications_names() {
        let keyboard = Keyboard::LINUX;
        let map = keyboard.default_shortcuts();
        let captions = map.get(ShortcutAction::ToggleCaptions).unwrap();
        assert_eq!(trigger(captions, &keyboard), "CTRL+ALT+SHIFT+s");
        let larger = map.get(ShortcutAction::LargerText).unwrap();
        assert_eq!(trigger(larger, &keyboard), "CTRL+ALT+SHIFT+equal");
    }

    #[test]
    fn ids_lead_back_to_actions() {
        for action_kind in ShortcutAction::ALL {
            assert_eq!(action(action_kind.raw_value()), Some(action_kind));
        }
        assert_eq!(action("gone"), None);
    }
}
