import SwiftUI
import AppKit
import CaptionCore
import ShortcutCore
import OverlayCore

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
// A stand-in for video, bright at one end and dark at the other, so that opacity has something
// to reveal and a style can be judged against both.
private struct CaptionBackdrop: View {
 var body: some View {
  LinearGradient(colors: [Color(red: 0.96, green: 0.85, blue: 0.55), Color(red: 0.5, green: 0.66, blue: 0.79), Color(red: 0.12, green: 0.16, blue: 0.27)], startPoint: .leading, endPoint: .trailing)
 }
}
private struct StylePreview: View {
 let style: CaptionStyle
 private let size = 18.0
 var body: some View {
  HStack(spacing: size * 0.3) {
   CaptionWord(text: "Captions look like this,", kind: .plain, fontSize: size, style: style)
   CaptionWord(text: "whoa", kind: .sound, fontSize: size, style: style)
   CaptionWord(text: "♪ music", kind: .cue, fontSize: size, style: style)
  }
  .lineLimit(1).fixedSize().modifier(CaptionTextLook(style: style))
  .padding(.horizontal, 14).padding(.vertical, 9)
  .background(style.background.color.opacity(style.backgroundOpacity), in: RoundedRectangle(cornerRadius: 10))
  .frame(maxWidth: .infinity).padding(.vertical, 16).background(CaptionBackdrop())
  .clipShape(RoundedRectangle(cornerRadius: 8))
  .accessibilityElement(children: .ignore).accessibilityLabel("Caption preview")
 }
}
private struct StyleTile: View {
 let preset: CaptionPreset
 let selected: Bool
 let action: () -> Void
 var body: some View {
  Button(action: action) {
   VStack(spacing: 5) {
    ZStack {
     CaptionBackdrop()
     RoundedRectangle(cornerRadius: 5).fill(preset.style.background.color.opacity(preset.style.backgroundOpacity)).padding(5)
     Text("Aa").font(.system(size: 17, weight: .semibold)).foregroundStyle(preset.style.text.color).modifier(CaptionTextLook(style: preset.style))
    }
    .frame(height: 46).clipShape(RoundedRectangle(cornerRadius: 8))
    .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(selected ? Color.accentColor : Color.primary.opacity(0.15), lineWidth: selected ? 2 : 1))
    Text(preset.name).font(.caption2).foregroundStyle(selected ? Color.primary : Color.secondary)
     .multilineTextAlignment(.center).lineLimit(2, reservesSpace: true)
   }
   .contentShape(Rectangle())
  }
  .buttonStyle(.plain).accessibilityLabel(preset.name).accessibilityAddTraits(selected ? .isSelected : [])
 }
}
private struct OpacityRow: View {
 let title: String
 @Binding var value: Double
 let range: ClosedRange<Double>
 var body: some View {
  LabeledContent(title) {
   HStack {
    Slider(value: $value, in: range, step: 0.01).labelsHidden().accessibilityLabel(title)
    Text("\(Int((value * 100).rounded()))%").monospacedDigit().frame(width: 40, alignment: .trailing)
   }
   .frame(width: 240)
  }
 }
}
struct SettingsView: View {
 @ObservedObject var state: AppState
 private func color(_ path: WritableKeyPath<CaptionStyle, CaptionColor>) -> Binding<Color> {
  Binding(get: { state.captionStyle[keyPath: path].color }, set: { state.captionStyle[keyPath: path] = CaptionColor($0) })
 }
 var body: some View {
  Form {
   Section("Captions") {
    Picker("Update speed", selection: $state.updateSpeed) { ForEach(UpdateSpeed.allCases) { Text($0.title).tag($0) } }
    Text(state.updateSpeed.detail).font(.caption).foregroundStyle(.secondary)
    Toggle("Highlight sound words and slang", isOn: $state.highlightWords)
    Toggle("Click through overlay", isOn: $state.clickThrough)
    Text("Clicks go to whatever is behind the overlay, so you can use video controls under it. To turn this off, point at the overlay and choose Turn off click-through, or use the shortcut.").font(.caption).foregroundStyle(.secondary)
   }
   Section("Appearance") {
    StylePreview(style: state.captionStyle)
    HStack(alignment: .top, spacing: 8) {
     ForEach(CaptionPreset.all) { preset in
      StyleTile(preset: preset, selected: state.captionStyle == preset.style) { state.captionStyle = preset.style }
     }
    }
    ColorPicker("Text", selection: color(\.text), supportsOpacity: false)
    ColorPicker("Background", selection: color(\.background), supportsOpacity: false)
    ColorPicker("Highlighted words", selection: color(\.highlight), supportsOpacity: false)
    OpacityRow(title: "Background opacity", value: $state.captionStyle.backgroundOpacity, range: 0...1)
    OpacityRow(title: "Text opacity", value: $state.captionStyle.textOpacity, range: CaptionStyle.textOpacities)
    if state.captionStyle.isHardToRead {
     Label("These colors are hard to read against each other.", systemImage: "exclamationmark.triangle.fill").font(.caption).foregroundStyle(.orange)
    } else if state.captionStyle.isSeeThrough {
     Text("Text gets a soft shadow so it stays readable over the video.").font(.caption).foregroundStyle(.secondary)
    }
   }
   Section("Keyboard shortcuts") {
    ForEach(ShortcutAction.allCases) { ShortcutRecorder(state: state, action: $0) }
    Text("These work in every app, including while Chrome is in front. Escape cancels recording.").font(.caption).foregroundStyle(.secondary)
    HStack { Spacer(); Button("Restore Defaults") { state.restoreShortcuts() } }
   }
  }
  .formStyle(.grouped).frame(width: 500, height: 760)
 }
}
