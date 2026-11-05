import Foundation
public enum ShortcutAction: String, CaseIterable, Codable, Identifiable, Sendable {
 case toggleCaptions, toggleOverlay, toggleExpanded, largerText, smallerText, toggleClickThrough, resetOverlay
 public var id: String { rawValue }
 public var title: String {
  switch self {
  case .toggleCaptions: return "Start or stop captions"
  case .toggleOverlay: return "Show or hide overlay"
  case .toggleExpanded: return "Expand or collapse overlay"
  case .largerText: return "Larger text"
  case .smallerText: return "Smaller text"
  case .toggleClickThrough: return "Click through overlay"
  case .resetOverlay: return "Reset overlay"
  }
 }
}
// Carbon's modifier bits, which RegisterEventHotKey expects.
public enum Modifier {
 public static let command: UInt32 = 256, shift: UInt32 = 512, option: UInt32 = 2048, control: UInt32 = 4096
}
public struct Shortcut: Codable, Equatable, Sendable {
 public let keyCode: UInt32
 public let modifiers: UInt32
 public let key: String
 public init(keyCode: UInt32, modifiers: UInt32, key: String) { self.keyCode=keyCode; self.modifiers=modifiers; self.key=key }
 public var label: String {
  var text = ""
  if modifiers & Modifier.control != 0 { text += "⌃" }
  if modifiers & Modifier.option != 0 { text += "⌥" }
  if modifiers & Modifier.shift != 0 { text += "⇧" }
  if modifiers & Modifier.command != 0 { text += "⌘" }
  return text + key
 }
 // A global shortcut works in every app, so a bare key or Shift+key would stop that key
 // being typed anywhere. Function keys are the exception.
 public var isAllowed: Bool {
  modifiers & (Modifier.command | Modifier.control | Modifier.option) != 0 || Shortcut.functionKeys.contains(keyCode)
 }
 static let functionKeys: Set<UInt32> = [122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79, 80, 90]
}
public struct ShortcutMap: Codable, Equatable, Sendable {
 public private(set) var bindings: [ShortcutAction: Shortcut]
 public init(bindings: [ShortcutAction: Shortcut]) { self.bindings = bindings }
 // Control and Option together, which Chrome and macOS leave largely free.
 public static let defaults: ShortcutMap = {
  let held = Modifier.control | Modifier.option
  return ShortcutMap(bindings: [
   .toggleCaptions: Shortcut(keyCode: 1, modifiers: held, key: "S"),
   .toggleOverlay: Shortcut(keyCode: 31, modifiers: held, key: "O"),
   .toggleExpanded: Shortcut(keyCode: 14, modifiers: held, key: "E"),
   .largerText: Shortcut(keyCode: 24, modifiers: held, key: "="),
   .smallerText: Shortcut(keyCode: 27, modifiers: held, key: "-"),
   .toggleClickThrough: Shortcut(keyCode: 17, modifiers: held, key: "T"),
  ])
 }()
 public func action(using shortcut: Shortcut, except action: ShortcutAction? = nil) -> ShortcutAction? {
  bindings.first { $0.key != action && $0.value.keyCode == shortcut.keyCode && $0.value.modifiers == shortcut.modifiers }?.key
 }
 public mutating func set(_ shortcut: Shortcut?, for action: ShortcutAction) { bindings[action] = shortcut }
 public func encoded() -> Data { (try? JSONEncoder().encode(bindings.reduce(into: [String: Shortcut]()) { $0[$1.key.rawValue] = $1.value })) ?? Data() }
 public init?(data: Data) {
  guard let raw = try? JSONDecoder().decode([String: Shortcut].self, from: data) else { return nil }
  bindings = raw.reduce(into: [:]) { result, entry in if let action = ShortcutAction(rawValue: entry.key) { result[action] = entry.value } }
 }
}
