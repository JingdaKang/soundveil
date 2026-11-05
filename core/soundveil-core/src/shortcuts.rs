//! Shortcut actions, validation, and storage.
//!
//! Key codes and modifier bits are stored in the operating system's own values, because those
//! are what its hotkey API expects. [`Keyboard`] holds what differs between systems.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShortcutAction {
    ToggleCaptions,
    ToggleOverlay,
    ToggleExpanded,
    LargerText,
    SmallerText,
    ToggleClickThrough,
    ResetOverlay,
}

impl ShortcutAction {
    pub const ALL: [Self; 7] = [
        Self::ToggleCaptions,
        Self::ToggleOverlay,
        Self::ToggleExpanded,
        Self::LargerText,
        Self::SmallerText,
        Self::ToggleClickThrough,
        Self::ResetOverlay,
    ];

    pub fn raw_value(self) -> &'static str {
        match self {
            Self::ToggleCaptions => "toggleCaptions",
            Self::ToggleOverlay => "toggleOverlay",
            Self::ToggleExpanded => "toggleExpanded",
            Self::LargerText => "largerText",
            Self::SmallerText => "smallerText",
            Self::ToggleClickThrough => "toggleClickThrough",
            Self::ResetOverlay => "resetOverlay",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::ToggleCaptions => "Start or stop captions",
            Self::ToggleOverlay => "Show or hide overlay",
            Self::ToggleExpanded => "Expand or collapse overlay",
            Self::LargerText => "Larger text",
            Self::SmallerText => "Smaller text",
            Self::ToggleClickThrough => "Click through overlay",
            Self::ResetOverlay => "Reset overlay",
        }
    }

    fn from_raw(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.raw_value() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LabelStyle {
    Symbols,
    Words,
}

/// What a shortcut means on one operating system.
#[derive(Clone, Copy, Debug)]
pub struct Keyboard {
    pub control: u32,
    pub alt: u32,
    pub shift: u32,
    /// Command on a Mac, the Windows key on Windows.
    pub system: u32,
    function_keys: &'static [u32],
    labels: LabelStyle,
    defaults: fn() -> ShortcutMap,
}

impl Keyboard {
    /// Carbon's modifier bits and virtual key codes, which RegisterEventHotKey expects.
    pub const MAC: Self = Self {
        control: 4096,
        alt: 2048,
        shift: 512,
        system: 256,
        function_keys: &[
            122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79, 80,
            90,
        ],
        labels: LabelStyle::Symbols,
        defaults: mac_defaults,
    };

    /// `MOD_*` flags and virtual key codes, which RegisterHotKey expects.
    pub const WINDOWS: Self = Self {
        control: 2,
        alt: 1,
        shift: 4,
        system: 8,
        function_keys: &[
            0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x7B, 0x7C, 0x7D,
            0x7E, 0x7F, 0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
        ],
        labels: LabelStyle::Words,
        defaults: windows_defaults,
    };

    pub fn default_shortcuts(&self) -> ShortcutMap {
        (self.defaults)()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub key_code: u32,
    pub modifiers: u32,
    pub key: String,
}

impl Shortcut {
    pub fn new(key_code: u32, modifiers: u32, key: impl Into<String>) -> Self {
        Self {
            key_code,
            modifiers,
            key: key.into(),
        }
    }

    pub fn label(&self, keyboard: &Keyboard) -> String {
        let held = |bit: u32| self.modifiers & bit != 0;
        let parts = [
            (held(keyboard.control), "⌃", "Ctrl"),
            (held(keyboard.alt), "⌥", "Alt"),
            (held(keyboard.shift), "⇧", "Shift"),
            (held(keyboard.system), "⌘", "Win"),
        ];
        let mut text = String::new();
        for (on, symbol, word) in parts {
            match (on, keyboard.labels) {
                (true, LabelStyle::Symbols) => text.push_str(symbol),
                (true, LabelStyle::Words) => {
                    text.push_str(word);
                    text.push('+');
                }
                (false, _) => {}
            }
        }
        text + &self.key
    }

    /// A global shortcut works in every app, so a bare key or Shift+key would stop that key being
    /// typed anywhere. Function keys are the exception.
    pub fn is_allowed(&self, keyboard: &Keyboard) -> bool {
        self.modifiers & (keyboard.system | keyboard.control | keyboard.alt) != 0
            || keyboard.function_keys.contains(&self.key_code)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutMap {
    bindings: BTreeMap<ShortcutAction, Shortcut>,
}

// Control and Option together, which Chrome and macOS leave largely free.
fn mac_defaults() -> ShortcutMap {
    let held = Keyboard::MAC.control | Keyboard::MAC.alt;
    let bind = |code, key| Shortcut::new(code, held, key);
    ShortcutMap::new([
        (ShortcutAction::ToggleCaptions, bind(1, "S")),
        (ShortcutAction::ToggleOverlay, bind(31, "O")),
        (ShortcutAction::ToggleExpanded, bind(14, "E")),
        (ShortcutAction::LargerText, bind(24, "=")),
        (ShortcutAction::SmallerText, bind(27, "-")),
        (ShortcutAction::ToggleClickThrough, bind(17, "T")),
    ])
}

// Windows treats Ctrl+Alt as AltGr, so Ctrl+Alt+E would stop people typing € on many keyboard
// layouts. Adding Shift keeps the defaults clear of those characters.
fn windows_defaults() -> ShortcutMap {
    let keyboard = Keyboard::WINDOWS;
    let held = keyboard.control | keyboard.alt | keyboard.shift;
    let bind = |code, key| Shortcut::new(code, held, key);
    ShortcutMap::new([
        (ShortcutAction::ToggleCaptions, bind(0x53, "S")),
        (ShortcutAction::ToggleOverlay, bind(0x4F, "O")),
        (ShortcutAction::ToggleExpanded, bind(0x45, "E")),
        (ShortcutAction::LargerText, bind(0xBB, "=")),
        (ShortcutAction::SmallerText, bind(0xBD, "-")),
        (ShortcutAction::ToggleClickThrough, bind(0x54, "T")),
    ])
}

impl ShortcutMap {
    pub fn new(bindings: impl IntoIterator<Item = (ShortcutAction, Shortcut)>) -> Self {
        Self {
            bindings: bindings.into_iter().collect(),
        }
    }

    pub fn get(&self, action: ShortcutAction) -> Option<&Shortcut> {
        self.bindings.get(&action)
    }

    pub fn iter(&self) -> impl Iterator<Item = (ShortcutAction, &Shortcut)> {
        self.bindings
            .iter()
            .map(|(action, shortcut)| (*action, shortcut))
    }

    /// The action already using this shortcut, if any, apart from `except`.
    pub fn action_using(
        &self,
        shortcut: &Shortcut,
        except: Option<ShortcutAction>,
    ) -> Option<ShortcutAction> {
        self.iter()
            .find(|(action, bound)| {
                Some(*action) != except
                    && bound.key_code == shortcut.key_code
                    && bound.modifiers == shortcut.modifiers
            })
            .map(|(action, _)| action)
    }

    pub fn set(&mut self, shortcut: Option<Shortcut>, action: ShortcutAction) {
        match shortcut {
            Some(shortcut) => self.bindings.insert(action, shortcut),
            None => self.bindings.remove(&action),
        };
    }

    pub fn encoded(&self) -> Vec<u8> {
        let raw: BTreeMap<&str, &Shortcut> = self
            .iter()
            .map(|(action, shortcut)| (action.raw_value(), shortcut))
            .collect();
        serde_json::to_vec(&raw).unwrap_or_default()
    }

    /// Reads saved shortcuts, ignoring actions this version does not have.
    pub fn from_json(data: &[u8]) -> Option<Self> {
        let raw: BTreeMap<String, Shortcut> = serde_json::from_slice(data).ok()?;
        Some(Self::new(raw.into_iter().filter_map(|(name, shortcut)| {
            Some((ShortcutAction::from_raw(&name)?, shortcut))
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const MAC: Keyboard = Keyboard::MAC;
    const WINDOWS: Keyboard = Keyboard::WINDOWS;

    fn held() -> u32 {
        MAC.control | MAC.alt
    }

    #[test]
    fn labels_use_each_systems_order() {
        let all = MAC.system | MAC.shift | held();
        assert_eq!(Shortcut::new(1, all, "S").label(&MAC), "⌃⌥⇧⌘S");
        let all = WINDOWS.control | WINDOWS.alt | WINDOWS.shift | WINDOWS.system;
        assert_eq!(
            Shortcut::new(0x53, all, "S").label(&WINDOWS),
            "Ctrl+Alt+Shift+Win+S"
        );
    }

    #[test]
    fn bare_keys_are_rejected() {
        for keyboard in [MAC, WINDOWS] {
            assert!(!Shortcut::new(1, 0, "S").is_allowed(&keyboard));
            assert!(!Shortcut::new(1, keyboard.shift, "S").is_allowed(&keyboard));
            assert!(Shortcut::new(1, keyboard.alt, "S").is_allowed(&keyboard));
        }
    }

    #[test]
    fn function_keys_need_no_modifier() {
        assert!(Shortcut::new(122, 0, "F1").is_allowed(&MAC));
        assert!(Shortcut::new(0x70, 0, "F1").is_allowed(&WINDOWS));
    }

    #[test]
    fn defaults_are_allowed_and_distinct() {
        for keyboard in [MAC, WINDOWS] {
            let defaults = keyboard.default_shortcuts();
            let shortcuts: Vec<&Shortcut> = defaults.iter().map(|(_, s)| s).collect();
            assert!(shortcuts.iter().all(|s| s.is_allowed(&keyboard)));
            let labels: HashSet<String> = shortcuts.iter().map(|s| s.label(&keyboard)).collect();
            assert_eq!(labels.len(), shortcuts.len());
        }
    }

    #[test]
    fn windows_defaults_leave_altgr_free() {
        let defaults = WINDOWS.default_shortcuts();
        assert!(
            defaults
                .iter()
                .all(|(_, s)| s.modifiers & WINDOWS.shift != 0)
        );
    }

    #[test]
    fn conflicts_are_found() {
        let map = MAC.default_shortcuts();
        let taken = map.get(ShortcutAction::ToggleOverlay).unwrap();
        assert_eq!(
            map.action_using(taken, None),
            Some(ShortcutAction::ToggleOverlay)
        );
        assert_eq!(
            map.action_using(taken, Some(ShortcutAction::ToggleOverlay)),
            None
        );
        assert_eq!(
            map.action_using(taken, Some(ShortcutAction::ToggleExpanded)),
            Some(ShortcutAction::ToggleOverlay)
        );
    }

    #[test]
    fn bindings_can_be_cleared_and_changed() {
        let mut map = MAC.default_shortcuts();
        map.set(None, ShortcutAction::ToggleCaptions);
        assert!(map.get(ShortcutAction::ToggleCaptions).is_none());
        let new = Shortcut::new(122, 0, "F1");
        map.set(Some(new.clone()), ShortcutAction::ResetOverlay);
        assert_eq!(map.get(ShortcutAction::ResetOverlay), Some(&new));
    }

    #[test]
    fn round_trip() {
        let map = MAC.default_shortcuts();
        assert_eq!(ShortcutMap::from_json(&map.encoded()), Some(map));
    }

    #[test]
    fn unknown_actions_are_ignored() {
        let data = br#"{"gone":{"keyCode":1,"modifiers":256,"key":"S"}}"#;
        assert_eq!(ShortcutMap::from_json(data).unwrap().iter().count(), 0);
    }

    #[test]
    fn reads_what_the_mac_app_saved() {
        let data = br#"{"toggleCaptions":{"keyCode":1,"modifiers":6144,"key":"S"}}"#;
        let map = ShortcutMap::from_json(data).unwrap();
        assert_eq!(
            map.get(ShortcutAction::ToggleCaptions).unwrap().label(&MAC),
            "⌃⌥S"
        );
    }
}
