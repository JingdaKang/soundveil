import Foundation
// The recognition models on offer, lightest first. Each raw value names a file in the upstream
// GGML collection and the file saved on disk, so changing one orphans existing downloads.
public enum Model: String, CaseIterable, Identifiable, Sendable {
 case base = "base.en", small = "small.en", turboCompressed = "large-v3-turbo-q5_0", turbo = "large-v3-turbo", quality = "large-v3"
 public static let standard = Model.turbo
 public static func saved(_ value: String?) -> Model { value.flatMap(Model.init(rawValue:)) ?? standard }
 public var id: String { rawValue }
 public var fileName: String { "ggml-\(rawValue).bin" }
 public var name: String {
  switch self {
  case .base: return "Base"
  case .small: return "Small"
  case .turboCompressed: return "Large v3 Turbo Compressed"
  case .turbo: return "Large v3 Turbo"
  case .quality: return "Large v3"
  }
 }
 public var title: String {
  switch self {
  case .base: return String(localized: "Base · lightest · 148 MB")
  case .small: return String(localized: "Small · light · 488 MB")
  case .turboCompressed: return String(localized: "Large v3 Turbo Compressed · less memory · 574 MB")
  case .turbo: return String(localized: "Large v3 Turbo · recommended · 1.6 GB")
  case .quality: return String(localized: "Large v3 · heaviest · 3.1 GB")
  }
 }
 public var detail: String {
  switch self {
  case .base: return String(localized: "The fastest choice for older Macs or long battery life. Expect more mistakes, especially on names and fast speech.")
  case .small: return String(localized: "More accurate than Base and still light. Try this if the Large models fall behind on your Mac.")
  case .turboCompressed: return String(localized: "Close to Turbo's accuracy in much less memory. A good first choice for Macs with 8 GB.")
  case .turbo: return String(localized: "The best balance of accuracy and speed. Needs about 2 GB of free memory, so it suits Macs with 16 GB or more.")
  case .quality: return String(localized: "May do better on difficult audio, but captions run further behind and it needs far more memory.")
  }
 }
}
