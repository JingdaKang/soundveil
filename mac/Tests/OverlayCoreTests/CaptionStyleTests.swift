import XCTest
@testable import OverlayCore
final class CaptionStyleTests: XCTestCase {
 let white = CaptionColor(hex: 0xFFFFFF), black = CaptionColor(hex: 0x000000), gray = CaptionColor(hex: 0x777777)
 func testColorChannelsSurviveARoundTrip() {
  let color = CaptionColor(hex: 0xFFD14D)
  XCTAssertEqual(CaptionColor(red: color.red, green: color.green, blue: color.blue), color)
 }
 func testChannelsOutsideTheRangeAreClamped() {
  XCTAssertEqual(CaptionColor(red: 2, green: -1, blue: 0), CaptionColor(hex: 0xFF0000))
 }
 func testContrastMatchesWCAG() {
  XCTAssertEqual(white.contrast(with: black), 21, accuracy: 0.001)
  XCTAssertEqual(black.contrast(with: white), 21, accuracy: 0.001)
  XCTAssertEqual(gray.contrast(with: white), 4.48, accuracy: 0.01)
 }
 func testDefaultKeepsTheOriginalLook() {
  XCTAssertEqual(CaptionStyle.standard.text, white)
  XCTAssertEqual(CaptionStyle.standard.background, black)
  XCTAssertEqual(CaptionStyle.standard.backgroundOpacity, 0.78)
  XCTAssertEqual(CaptionStyle.standard.textOpacity, 1)
  XCTAssertFalse(CaptionStyle.standard.isSeeThrough)
 }
 func testPresetsAreReadable() {
  for preset in CaptionPreset.all {
   XCTAssertFalse(preset.style.isHardToRead, preset.name)
   XCTAssertGreaterThanOrEqual(preset.style.text.contrast(with: preset.style.background), 4.5, preset.name)
   XCTAssertGreaterThanOrEqual(preset.style.highlight.contrast(with: preset.style.background), 4.5, preset.name)
  }
 }
 func testHighlightsStandApartFromTheirText() {
  for preset in CaptionPreset.all { XCTAssertNotEqual(preset.style.highlight, preset.style.text, preset.name) }
 }
 func testSimilarColorsAreHardToRead() {
  var style = CaptionStyle.standard
  style.background = CaptionColor(hex: 0xEEEEEE)
  XCTAssertTrue(style.isHardToRead)
 }
 func testAFaintHighlightIsHardToRead() {
  var style = CaptionStyle.standard
  style.highlight = CaptionColor(hex: 0x222222)
  XCTAssertTrue(style.isHardToRead)
 }
 func testFadedTextIsHardToRead() {
  var style = CaptionStyle.standard
  style.textOpacity = 0.4
  XCTAssertTrue(style.isHardToRead)
  style.textOpacity = 0.6
  XCTAssertFalse(style.isHardToRead)
 }
 func testSeeThroughStylesAreJudgedByTheirShadow() {
  var style = CaptionStyle.standard
  style.background = CaptionColor(hex: 0xEEEEEE); style.backgroundOpacity = 0.3
  XCTAssertTrue(style.isSeeThrough)
  XCTAssertFalse(style.isHardToRead)
 }
 func testTheShadowOpposesTheText() {
  XCTAssertEqual(CaptionStyle.standard.edge, black)
  var dark = CaptionStyle.standard; dark.text = black
  XCTAssertEqual(dark.edge, white)
 }
 func testStylesSurviveSaving() {
  let style = CaptionPreset.all[3].style
  XCTAssertEqual(CaptionStyle(data: style.encoded()), style)
  XCTAssertNil(CaptionStyle(data: Data("nonsense".utf8)))
 }
}
