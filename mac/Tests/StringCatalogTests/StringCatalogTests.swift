import XCTest

// Keeps the catalogs complete, so a string added with scripts/sync-strings.sh can't ship half
// translated. Reads the files directly: the app's translations aren't in the test bundle.
final class StringCatalogTests: XCTestCase {
 struct Catalog: Decodable {
  struct Entry: Decodable {
   struct Localization: Decodable {
    struct Unit: Decodable { let state: String; let value: String }
    let stringUnit: Unit?
   }
   let localizations: [String: Localization]?
   let shouldTranslate: Bool?
  }
  let sourceLanguage: String
  let strings: [String: Entry]
 }
 func load(_ name: String) throws -> Catalog {
  let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
  return try JSONDecoder().decode(Catalog.self, from: Data(contentsOf: root.appendingPathComponent("Resources/\(name).xcstrings")))
 }
 func placeholders(_ text: String) -> [String] {
  let pattern = try! NSRegularExpression(pattern: "%(?:\\d+\\$)?(?:@|lld|ld|d|lf|f)")
  return pattern.matches(in: text, range: NSRange(text.startIndex..., in: text)).map {
   (text as NSString).substring(with: $0.range).replacingOccurrences(of: "\\d+\\$", with: "", options: .regularExpression)
  }.sorted()
 }
 func languages(_ catalog: Catalog) -> Set<String> {
  Set(catalog.strings.values.flatMap { ($0.localizations ?? [:]).keys }).subtracting([catalog.sourceLanguage])
 }
 func testEveryLanguageHasEveryString() throws {
  for name in ["Localizable", "InfoPlist"] {
   let catalog = try load(name)
   let expected = languages(catalog)
   XCTAssertFalse(expected.isEmpty)
   for (key, entry) in catalog.strings where entry.shouldTranslate != false {
    for language in expected {
     let unit = entry.localizations?[language]?.stringUnit
     XCTAssertEqual(unit?.state, "translated", "\(language) is missing: \(key)")
     XCTAssertFalse(unit?.value.isEmpty ?? true, "\(language) is empty: \(key)")
    }
   }
  }
 }
 func testTranslationsKeepTheirPlaceholders() throws {
  let catalog = try load("Localizable")
  for (key, entry) in catalog.strings {
   for (language, localization) in entry.localizations ?? [:] {
    XCTAssertEqual(placeholders(localization.stringUnit?.value ?? ""), placeholders(key), "\(language): \(key)")
   }
  }
 }
 func testBothCatalogsCoverTheSameLanguages() throws {
  XCTAssertEqual(languages(try load("Localizable")), languages(try load("InfoPlist")))
 }
}
