import Foundation

// An sRGB color packed as 0xRRGGBB, so saved styles compare exactly.
public struct CaptionColor: Codable, Equatable, Hashable, Sendable {
 public let hex: UInt32
 public init(hex: UInt32) { self.hex = hex & 0xFFFFFF }
 public init(red: Double, green: Double, blue: Double) {
  func byte(_ value: Double) -> UInt32 { UInt32((min(1, max(0, value)) * 255).rounded()) }
  self.init(hex: byte(red) << 16 | byte(green) << 8 | byte(blue))
 }
 public var red: Double { Double(hex >> 16 & 255) / 255 }
 public var green: Double { Double(hex >> 8 & 255) / 255 }
 public var blue: Double { Double(hex & 255) / 255 }
 // Relative luminance, as WCAG defines it.
 public var luminance: Double {
  func linear(_ value: Double) -> Double { value <= 0.04045 ? value / 12.92 : pow((value + 0.055) / 1.055, 2.4) }
  return 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
 }
 public func contrast(with other: CaptionColor) -> Double {
  let (light, dark) = (max(luminance, other.luminance), min(luminance, other.luminance))
  return (light + 0.05) / (dark + 0.05)
 }
 public func blended(over other: CaptionColor, opacity: Double) -> CaptionColor {
  func mix(_ top: Double, _ bottom: Double) -> Double { top * opacity + bottom * (1 - opacity) }
  return CaptionColor(red: mix(red, other.red), green: mix(green, other.green), blue: mix(blue, other.blue))
 }
}
public struct CaptionStyle: Codable, Equatable, Sendable {
 public var text: CaptionColor
 public var background: CaptionColor
 public var highlight: CaptionColor // Sound words, slang, and cues such as music.
 public var backgroundOpacity: Double
 public var textOpacity: Double
 public init(text: CaptionColor, background: CaptionColor, highlight: CaptionColor, backgroundOpacity: Double, textOpacity: Double = 1) {
  self.text = text; self.background = background; self.highlight = highlight
  self.backgroundOpacity = backgroundOpacity; self.textOpacity = textOpacity
 }
 public static let textOpacities = 0.4...1.0
 // Below this the video behind matters more than the background color. White text on black at
 // this opacity still clears 4.5:1 with a white video behind it.
 public static let seeThrough = 0.6
 static let readableContrast = 4.5 // WCAG AA, also the usual advice for captions.
 public var isSeeThrough: Bool { backgroundOpacity < Self.seeThrough }
 public var edge: CaptionColor { CaptionColor(hex: text.luminance > 0.4 ? 0x000000 : 0xFFFFFF) }
 // A see-through overlay is judged by its shadow and the video, which can't be known here.
 public var isHardToRead: Bool {
  guard !isSeeThrough else { return false }
  let shown = text.blended(over: background, opacity: textOpacity)
  return min(shown.contrast(with: background), highlight.contrast(with: background)) < Self.readableContrast
 }
 public init?(data: Data) {
  guard let style = try? JSONDecoder().decode(CaptionStyle.self, from: data) else { return nil }
  self = style
 }
 public func encoded() -> Data { (try? JSONEncoder().encode(self)) ?? Data() }
}
public struct CaptionPreset: Identifiable, Sendable {
 public let name: String
 public let style: CaptionStyle
 public var id: String { name }
 // Each mirrors a look people already ask for: the standard dark box, text alone over the
 // video, and the high-contrast pairs that system caption settings offer. A highlight has to
 // stand apart from its own text color, so it changes with the pair.
 public static let all = [
  CaptionPreset(name: String(localized: "Default"), style: .standard),
  CaptionPreset(name: String(localized: "Text only"), style: CaptionStyle(text: CaptionColor(hex: 0xFFFFFF), background: CaptionColor(hex: 0x000000), highlight: CaptionColor(hex: 0xFFD14D), backgroundOpacity: 0)),
  CaptionPreset(name: String(localized: "Yellow on black"), style: CaptionStyle(text: CaptionColor(hex: 0xFFE600), background: CaptionColor(hex: 0x000000), highlight: CaptionColor(hex: 0x5CE1E6), backgroundOpacity: 1)),
  CaptionPreset(name: String(localized: "Black on white"), style: CaptionStyle(text: CaptionColor(hex: 0x000000), background: CaptionColor(hex: 0xFFFFFF), highlight: CaptionColor(hex: 0x0B57D0), backgroundOpacity: 1)),
  CaptionPreset(name: String(localized: "Yellow on blue"), style: CaptionStyle(text: CaptionColor(hex: 0xFFE600), background: CaptionColor(hex: 0x0A2A8C), highlight: CaptionColor(hex: 0xFFFFFF), backgroundOpacity: 1)),
 ]
}
public extension CaptionStyle {
 static let standard = CaptionStyle(text: CaptionColor(hex: 0xFFFFFF), background: CaptionColor(hex: 0x000000), highlight: CaptionColor(hex: 0xFFD14D), backgroundOpacity: 0.78)
}
