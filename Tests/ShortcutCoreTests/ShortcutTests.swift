import XCTest
@testable import ShortcutCore
final class ShortcutTests: XCTestCase {
 let held = Modifier.control | Modifier.option
 func testLabelsUseMacOrder() {
  XCTAssertEqual(Shortcut(keyCode: 1, modifiers: Modifier.command | Modifier.shift | held, key: "S").label, "⌃⌥⇧⌘S")
 }
 func testBareKeysAreRejected() {
  XCTAssertFalse(Shortcut(keyCode: 1, modifiers: 0, key: "S").isAllowed)
  XCTAssertFalse(Shortcut(keyCode: 1, modifiers: Modifier.shift, key: "S").isAllowed)
  XCTAssertTrue(Shortcut(keyCode: 1, modifiers: Modifier.option, key: "S").isAllowed)
 }
 func testFunctionKeysNeedNoModifier() { XCTAssertTrue(Shortcut(keyCode: 122, modifiers: 0, key: "F1").isAllowed) }
 func testDefaultsAreAllowedAndDistinct() {
  let shortcuts = Array(ShortcutMap.defaults.bindings.values)
  XCTAssertTrue(shortcuts.allSatisfy(\.isAllowed))
  XCTAssertEqual(Set(shortcuts.map(\.label)).count, shortcuts.count)
 }
 func testConflictsAreFound() {
  let map = ShortcutMap.defaults, taken = map.bindings[.toggleOverlay]!
  XCTAssertEqual(map.action(using: taken), .toggleOverlay)
  XCTAssertNil(map.action(using: taken, except: .toggleOverlay))
  XCTAssertEqual(map.action(using: taken, except: .toggleExpanded), .toggleOverlay)
 }
 func testBindingsCanBeClearedAndChanged() {
  var map = ShortcutMap.defaults
  map.set(nil, for: .toggleCaptions); XCTAssertNil(map.bindings[.toggleCaptions])
  let new = Shortcut(keyCode: 122, modifiers: 0, key: "F1"); map.set(new, for: .resetOverlay)
  XCTAssertEqual(map.bindings[.resetOverlay], new)
 }
 func testRoundTrip() { XCTAssertEqual(ShortcutMap(data: ShortcutMap.defaults.encoded()), ShortcutMap.defaults) }
 func testUnknownActionsAreIgnored() {
  let data = Data(#"{"gone":{"keyCode":1,"modifiers":256,"key":"S"}}"#.utf8)
  XCTAssertEqual(ShortcutMap(data: data)?.bindings.count, 0)
 }
}
