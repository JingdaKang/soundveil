//! Global shortcuts, which work whichever app is in front.
//!
//! Saved shortcuts name their key the way Settings shows it, and that name is what gets bound here.
//! On a Wayland desktop an app can't take keys for itself, so `portal.rs` asks the desktop instead.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use slint::platform::Key;
use soundveil_core::{Keyboard, Shortcut, ShortcutAction, ShortcutMap};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex, PoisonError};

fn code(key: &str) -> Option<Code> {
    let name = match key {
        "=" => "Equal",
        "-" => "Minus",
        "[" => "BracketLeft",
        "]" => "BracketRight",
        ";" => "Semicolon",
        "'" => "Quote",
        "," => "Comma",
        "." => "Period",
        "/" => "Slash",
        "\\" => "Backslash",
        "`" => "Backquote",
        "Left" => "ArrowLeft",
        "Right" => "ArrowRight",
        "Up" => "ArrowUp",
        "Down" => "ArrowDown",
        "Space" => "Space",
        _ => key,
    };
    let name = match name.chars().next() {
        Some(letter) if name.len() == 1 && letter.is_ascii_uppercase() => format!("Key{name}"),
        Some(digit) if name.len() == 1 && digit.is_ascii_digit() => format!("Digit{name}"),
        _ => name.to_string(),
    };
    Code::from_str(&name).ok()
}

fn hotkey(shortcut: &Shortcut, keyboard: &Keyboard) -> Option<HotKey> {
    let mut held = Modifiers::empty();
    for (bit, modifier) in [
        (keyboard.control, Modifiers::CONTROL),
        (keyboard.alt, Modifiers::ALT),
        (keyboard.shift, Modifiers::SHIFT),
        (keyboard.system, Modifiers::META),
    ] {
        if shortcut.modifiers & bit != 0 {
            held |= modifier;
        }
    }
    Some(HotKey::new(Some(held), code(&shortcut.key)?))
}

/// The name a shortcut uses for a key press, or `None` for keys such as Shift on their own.
/// Holding Shift can change the character, so it's put back to the key that makes it.
pub fn key_name(text: &str) -> Option<String> {
    let mut letters = text.chars();
    let letter = letters.next()?;
    if letters.next().is_some() {
        return None;
    }
    const SHIFTED: &str = "+_{}:\"<>?|~!@#$%^&*()";
    const PLAIN: &str = "=-[];',./\\`1234567890";
    if let Some(at) = SHIFTED.chars().position(|c| c == letter) {
        return PLAIN.chars().nth(at).map(String::from);
    }
    const NAMED: [(Key, &str); 5] = [
        (Key::Space, "Space"),
        (Key::LeftArrow, "Left"),
        (Key::RightArrow, "Right"),
        (Key::UpArrow, "Up"),
        (Key::DownArrow, "Down"),
    ];
    if let Some((_, name)) = NAMED.iter().find(|(key, _)| char::from(key.clone()) == letter) {
        return Some((*name).to_string());
    }
    const FUNCTION: [Key; 24] = [
        Key::F1,
        Key::F2,
        Key::F3,
        Key::F4,
        Key::F5,
        Key::F6,
        Key::F7,
        Key::F8,
        Key::F9,
        Key::F10,
        Key::F11,
        Key::F12,
        Key::F13,
        Key::F14,
        Key::F15,
        Key::F16,
        Key::F17,
        Key::F18,
        Key::F19,
        Key::F20,
        Key::F21,
        Key::F22,
        Key::F23,
        Key::F24,
    ];
    if let Some(at) = FUNCTION.iter().position(|key| char::from(key.clone()) == letter) {
        return Some(format!("F{}", at + 1));
    }
    (letter.is_ascii_graphic()).then(|| letter.to_ascii_uppercase().to_string())
}

/// Binds shortcuts through the operating system. A failed start leaves it doing nothing, because
/// the buttons still work.
pub struct Hotkeys {
    manager: Option<GlobalHotKeyManager>,
    active: Vec<HotKey>,
    actions: Arc<Mutex<HashMap<u32, ShortcutAction>>>,
}

impl Hotkeys {
    /// `fire` runs on a thread of the library's choosing.
    pub fn new(fire: impl Fn(ShortcutAction) + Send + Sync + 'static) -> Self {
        let actions: Arc<Mutex<HashMap<u32, ShortcutAction>>> = Arc::default();
        let known = actions.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state() == HotKeyState::Pressed {
                let found = known
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(&event.id())
                    .copied();
                if let Some(action) = found {
                    fire(action);
                }
            }
        }));
        Self {
            manager: GlobalHotKeyManager::new().ok(),
            active: Vec::new(),
            actions,
        }
    }

    pub fn is_working(&self) -> bool {
        self.manager.is_some()
    }

    /// Lets go of every key, so a key can be recorded without setting off what it's bound to.
    pub fn pause(&mut self) {
        if let Some(manager) = &self.manager {
            let _ = manager.unregister_all(&self.active);
        }
        self.active.clear();
        self.actions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Binds every shortcut and returns the actions that another app's shortcut got to first.
    pub fn bind(&mut self, map: &ShortcutMap, keyboard: &Keyboard) -> Vec<ShortcutAction> {
        self.pause();
        let Some(manager) = &self.manager else {
            return Vec::new();
        };
        let mut taken = Vec::new();
        for (action, shortcut) in map.iter() {
            let Some(hotkey) = hotkey(shortcut, keyboard) else {
                taken.push(action);
                continue;
            };
            if manager.register(hotkey).is_ok() {
                self.active.push(hotkey);
                self.actions
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(hotkey.id(), action);
            } else {
                taken.push(action);
            }
        }
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_digits_become_codes() {
        assert_eq!(code("S"), Some(Code::KeyS));
        assert_eq!(code("7"), Some(Code::Digit7));
    }

    #[test]
    fn named_keys_become_codes() {
        assert_eq!(code("="), Some(Code::Equal));
        assert_eq!(code("\\"), Some(Code::Backslash));
        assert_eq!(code("Left"), Some(Code::ArrowLeft));
        assert_eq!(code("F12"), Some(Code::F12));
        assert_eq!(code("Space"), Some(Code::Space));
    }

    #[test]
    fn every_key_a_shortcut_can_name_has_a_code() {
        let mut keys: Vec<String> = ('A'..='Z').chain('0'..='9').map(String::from).collect();
        keys.extend("=-[];',./\\`".chars().map(String::from));
        keys.extend((1..=24).map(|number| format!("F{number}")));
        keys.extend(["Space", "Left", "Right", "Up", "Down"].map(String::from));
        for key in keys {
            assert!(code(&key).is_some(), "{key}");
        }
    }

    #[test]
    fn modifiers_follow_each_keyboard() {
        let keyboard = Keyboard::LINUX;
        let shortcut = Shortcut::new(0x73, keyboard.control | keyboard.system, "S");
        let expected = HotKey::new(Some(Modifiers::CONTROL | Modifiers::META), Code::KeyS);
        assert_eq!(hotkey(&shortcut, &keyboard), Some(expected));
    }

    #[test]
    fn the_defaults_all_bind_on_each_system() {
        for keyboard in [Keyboard::MAC, Keyboard::WINDOWS, Keyboard::LINUX] {
            for (_, shortcut) in keyboard.default_shortcuts().iter() {
                assert!(hotkey(shortcut, &keyboard).is_some());
            }
        }
    }

    #[test]
    fn key_presses_are_named_as_settings_shows_them() {
        assert_eq!(key_name("s").as_deref(), Some("S"));
        assert_eq!(key_name("S").as_deref(), Some("S"));
        assert_eq!(key_name("+").as_deref(), Some("="));
        assert_eq!(key_name("_").as_deref(), Some("-"));
        assert_eq!(key_name("!").as_deref(), Some("1"));
        assert_eq!(key_name(" ").as_deref(), Some("Space"));
        let f5 = char::from(Key::F5).to_string();
        assert_eq!(key_name(&f5).as_deref(), Some("F5"));
        let left = char::from(Key::LeftArrow).to_string();
        assert_eq!(key_name(&left).as_deref(), Some("Left"));
    }

    #[test]
    fn modifier_keys_alone_are_not_named() {
        for key in [Key::Shift, Key::Control, Key::Alt, Key::Meta] {
            assert_eq!(key_name(&char::from(key).to_string()), None);
        }
    }
}
