import Foundation
// Numeric major.minor.patch, optionally written as a "v"-prefixed release tag.
public struct Version: Comparable, CustomStringConvertible, Sendable {
 public let parts: [Int]
 public init?(_ text: String) {
  let fields = (text.hasPrefix("v") ? text.dropFirst() : Substring(text)).split(separator: ".", omittingEmptySubsequences: false)
  guard fields.count == 3, fields.allSatisfy({ !$0.isEmpty && $0.allSatisfy { ("0"..."9").contains($0) } }) else { return nil }
  let parts = fields.compactMap { Int($0) }
  guard parts.count == 3 else { return nil }
  self.parts = parts
 }
 public var description: String { parts.map(String.init).joined(separator: ".") }
 public static func < (a: Version, b: Version) -> Bool { a.parts.lexicographicallyPrecedes(b.parts) }
}
public struct Release: Sendable, Equatable {
 public let version: Version
 public let page: URL
 public let download: URL?
 public init(version: Version, page: URL, download: URL?) { self.version=version; self.page=page; self.download=download }
 // Reads GitHub's latest-release response. Drafts, prereleases, and non-HTTPS links are never offered.
 public init?(github data: Data) {
  struct Asset: Decodable { let name: String; let browserDownloadUrl: URL }
  struct Payload: Decodable { let tagName: String; let htmlUrl: URL; let draft: Bool; let prerelease: Bool; let assets: [Asset] }
  let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
  guard let payload = try? decoder.decode(Payload.self, from: data), !payload.draft, !payload.prerelease,
        let version = Version(payload.tagName), payload.htmlUrl.scheme == "https" else { return nil }
  let dmg = payload.assets.first { $0.name.hasSuffix(".dmg") && $0.browserDownloadUrl.scheme == "https" }
  self.init(version: version, page: payload.htmlUrl, download: dmg?.browserDownloadUrl)
 }
 public func isNewer(than current: String) -> Bool { Version(current).map { $0 < version } ?? false }
}
