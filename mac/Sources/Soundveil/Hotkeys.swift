import Carbon.HIToolbox
import ShortcutCore

// Global shortcuts through Carbon, which works while Chrome is frontmost and needs no
// Accessibility permission.
private nonisolated(unsafe) var fired: ((UInt32) -> Void)?
@MainActor final class Hotkeys {
 private var registered: [EventHotKeyRef] = []
 private var actions: [UInt32: ShortcutAction] = [:]
 private var handler: EventHandlerRef?
 var onAction: ((ShortcutAction) -> Void)?
 init() {
  var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
  InstallEventHandler(GetApplicationEventTarget(), { _, event, _ in
   var id = EventHotKeyID()
   GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID), nil, MemoryLayout<EventHotKeyID>.size, nil, &id)
   fired?(id.id)
   return noErr
  }, 1, &spec, nil, &handler)
  fired = { [weak self] id in DispatchQueue.main.async { MainActor.assumeIsolated { if let self, let action = self.actions[id] { self.onAction?(action) } } } }
 }
 // Returns the actions macOS refused, usually because another app holds that shortcut.
 @discardableResult func install(_ map: ShortcutMap) -> [ShortcutAction] {
  pause()
  var refused: [ShortcutAction] = []
  for (index, action) in ShortcutAction.allCases.enumerated() {
   guard let shortcut = map.bindings[action] else { continue }
   var ref: EventHotKeyRef?
   let id = EventHotKeyID(signature: OSType(0x53564C21), id: UInt32(index + 1)) // "SVL!"
   if RegisterEventHotKey(shortcut.keyCode, shortcut.modifiers, id, GetApplicationEventTarget(), 0, &ref) == noErr, let ref {
    registered.append(ref); actions[id.id] = action
   } else { refused.append(action) }
  }
  return refused
 }
 // While recording, the keys being pressed must reach the recorder.
 func pause() {
  registered.forEach { UnregisterEventHotKey($0) }
  registered = []; actions = [:]
 }
}
