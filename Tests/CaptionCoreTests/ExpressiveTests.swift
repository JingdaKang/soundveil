import XCTest
@testable import CaptionCore
final class ExpressiveTests: XCTestCase {
 func styled(_ text: String) -> [String] {
  let words = text.split(separator: " ").map(String.init)
  return zip(words, wordStyles(words)).filter { $0.1 != .plain }.map(\.0)
 }
 func testSoundWordsStandOut() {
  XCTAssertEqual(styled("Haha that was great"), ["Haha"])
  XCTAssertEqual(styled("and then whoa it just went kaboom, right"), ["whoa", "kaboom,"])
  XCTAssertEqual(wordStyles(["Ugh,", "fine."]), [.sound, .plain])
 }
 func testSpokenLaughs() {
  XCTAssertEqual(styled("He said ha ha ha and left"), ["ha", "ha", "ha"])
  XCTAssertEqual(styled("Hahahaha stop"), ["Hahahaha"]); XCTAssertEqual(styled("Hehehe okay"), ["Hehehe"])
  XCTAssertEqual(styled("Ha, I see"), [])
 }
 func testStretchedWords() {
  XCTAssertEqual(styled("It was sooooo good"), ["sooooo"]); XCTAssertEqual(styled("Nooo don't"), ["Nooo"])
  XCTAssertEqual(styled("A 1000 people"), [])
 }
 func testSlangAndMemes() {
  XCTAssertEqual(wordStyles(["bruh"]), [.slang]); XCTAssertEqual(styled("That is so sus honestly"), ["sus"])
  XCTAssertEqual(styled("No cap, that was good"), ["No", "cap,"])
  XCTAssertEqual(styled("OK let's go everyone"), ["let's", "go"])
 }
 func testEverydayWordsStayPlainUnlessShouted() {
  XCTAssertEqual(styled("The baby boom was based on data"), [])
  XCTAssertEqual(styled("And then boom! It fell"), ["boom!"]); XCTAssertEqual(styled("It was over. Boom. Gone"), ["Boom."])
  XCTAssertEqual(styled("Nice shot. Based."), ["Based."])
  XCTAssertEqual(styled("There is no way to know"), []); XCTAssertEqual(styled("No way!"), ["No", "way!"])
 }
 func testFillersStayPlain() { XCTAssertEqual(styled("Um, oh, yeah, uh, okay, hmm, well"), []) }
 func testOrdinaryDialogueIsUntouched() {
  XCTAssertEqual(styled("I dropped out of Reed College after the first six months, but then stayed around."), [])
 }
 func testCues() {
  XCTAssertEqual(wordStyles(["[laughter]", "Right"]), [.cue, .plain])
  XCTAssertEqual(cueLabel(for: "[MUSIC PLAYING]"), "music"); XCTAssertEqual(cueLabel(for: "(audience laughing)"), "laughter")
  XCTAssertEqual(cueLabel(for: "♪♪"), "music"); XCTAssertEqual(cueLabel(for: "[Applause]"), "applause")
  XCTAssertNil(cueLabel(for: "[BLANK_AUDIO]")); XCTAssertNil(cueLabel(for: "(speaking foreign language)"))
 }
}
