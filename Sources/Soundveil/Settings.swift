import SwiftUI
import AppKit
import CaptionCore
import ShortcutCore

private let specialKeys: [UInt16: String] = [
 36: "↩", 48: "⇥", 49: "Space", 51: "⌫", 117: "⌦", 115: "↖", 119: "↘", 116: "⇞", 121: "⇟", 123: "←", 124: "→", 125: "↓", 126: "↑",
 122: "F1", 120: "F2", 99: "F3", 118: "F4", 96: "F5", 97: "F6", 98: "F7", 100: "F8", 101: "F9", 109: "F10", 103: "F11", 111: "F12",
 105: "F13", 107: "F14", 113: "F15", 106: "F16", 64: "F17", 79: "F18", 80: "F19", 90: "F20",
]
private func shortcut(from event: NSEvent) -> Shortcut {
 let flags = event.modifierFlags
 var modifiers: UInt32 = 0
 if flags.contains(.command) { modifiers |= Modifier.command }
 if flags.contains(.shift) { modifiers |= Modifier.shift }
 if flags.contains(.option) { modifiers |= Modifier.option }
 if flags.contains(.control) { modifiers |= Modifier.control }
 let key = specialKeys[event.keyCode] ?? (event.charactersIgnoringModifiers ?? "?").uppercased()
 return Shortcut(keyCode: UInt32(event.keyCode), modifiers: modifiers, key: key)
}
struct ShortcutRecorder: View {
 @ObservedObject var state: AppState
 let action: ShortcutAction
 @State private var recording = false
 @State private var note = ""
 @State private var monitor: Any?
 private func finish() {
  recording = false
  if let monitor { NSEvent.removeMonitor(monitor) }
  monitor = nil; state.applyShortcuts()
 }
 private func record() {
  note = ""; recording = true; state.hotkeys.pause()
  monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
   if event.keyCode == 53 { finish(); return nil } // Escape cancels.
   let pressed = shortcut(from: event)
   if !pressed.isAllowed { note = "Include ⌘, ⌃, or ⌥, or use a function key."; return nil }
   if let other = state.shortcuts.action(using: pressed, except: action) { note = "Already used for \"\(other.title)\"."; return nil }
   state.setShortcut(pressed, for: action); finish()
   return nil
  }
 }
 var body: some View {
  LabeledContent(action.title) {
   VStack(alignment: .trailing, spacing: 3) {
    HStack(spacing: 6) {
     Button(recording ? "Type shortcut…" : state.shortcuts.bindings[action]?.label ?? "Record") { recording ? finish() : record() }
      .frame(minWidth: 110).monospacedDigit()
     Button { state.setShortcut(nil, for: action) } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary) }
      .buttonStyle(.plain).help("Remove shortcut").accessibilityLabel("Remove shortcut for \(action.title)")
      .disabled(state.shortcuts.bindings[action] == nil)
    }
    if !note.isEmpty { Text(note).font(.caption).foregroundStyle(.red) }
    else if state.refusedShortcuts.contains(action) { Text("macOS refused this. Another app may use it.").font(.caption).foregroundStyle(.red) }
   }
  }
  .onDisappear { if recording { finish() } }
 }
}
struct SettingsView: View {
 @ObservedObject var state: AppState
 var body: some View {
  Form {
   Section("Captions") {
    Picker("Update speed", selection: $state.updateSpeed) { ForEach(UpdateSpeed.allCases) { Text($0.title).tag($0) } }
    Text(state.updateSpeed.detail).font(.caption).foregroundStyle(.secondary)
    Toggle("Highlight sound words and slang", isOn: $state.highlightWords)
    Toggle("Click through overlay", isOn: $state.clickThrough)
    Text("Clicks pass to whatever is behind the overlay. It can't be dragged or show its controls until this is off.").font(.caption).foregroundStyle(.secondary)
   }
   Section("Keyboard shortcuts") {
    ForEach(ShortcutAction.allCases) { ShortcutRecorder(state: state, action: $0) }
    Text("These work in every app, including while Chrome is in front. Escape cancels recording.").font(.caption).foregroundStyle(.secondary)
    HStack { Spacer(); Button("Restore Defaults") { state.restoreShortcuts() } }
   }
  }
  .formStyle(.grouped).frame(width: 500, height: 620)
 }
}
