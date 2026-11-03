import XCTest
@testable import UpdateCore
final class ReleaseTests: XCTestCase {
 func payload(tag: String = "v0.2.0", draft: Bool = false, prerelease: Bool = false, assets: String = #"[{"name":"Soundveil-0.2.0-arm64.dmg.sha256","browser_download_url":"https://github.com/JingdaKang/soundveil/releases/download/v0.2.0/Soundveil-0.2.0-arm64.dmg.sha256"},{"name":"Soundveil-0.2.0-arm64.dmg","browser_download_url":"https://github.com/JingdaKang/soundveil/releases/download/v0.2.0/Soundveil-0.2.0-arm64.dmg"}]"#) -> Data {
  Data(#"{"tag_name":"\#(tag)","html_url":"https://github.com/JingdaKang/soundveil/releases/tag/\#(tag)","draft":\#(draft),"prerelease":\#(prerelease),"assets":\#(assets),"body":"notes"}"#.utf8)
 }
 func testVersionParsing() {
  XCTAssertEqual(Version("v1.2.3")?.parts, [1,2,3]); XCTAssertEqual(Version("0.10.0")?.description, "0.10.0")
  for bad in ["", "1.2", "1.2.3.4", "1..3", "v1.2.x", "+1.2.3", "1.2.3-beta", "99999999999999999999.0.0"] { XCTAssertNil(Version(bad), bad) }
 }
 func testNumericOrdering() {
  XCTAssertLessThan(Version("0.9.0")!, Version("0.10.0")!); XCTAssertLessThan(Version("0.1.9")!, Version("0.2.0")!)
  XCTAssertEqual(Version("v1.0.0"), Version("1.0.0"))
 }
 func testReadsLatestRelease() {
  let release = Release(github: payload())
  XCTAssertEqual(release?.version.description, "0.2.0")
  XCTAssertEqual(release?.download?.lastPathComponent, "Soundveil-0.2.0-arm64.dmg")
  XCTAssertEqual(release?.page.absoluteString, "https://github.com/JingdaKang/soundveil/releases/tag/v0.2.0")
 }
 func testMissingDiskImageFallsBackToPage() { XCTAssertNil(Release(github: payload(assets: "[]"))?.download) }
 func testIgnoresUnpublishedAndMalformed() {
  XCTAssertNil(Release(github: payload(draft: true))); XCTAssertNil(Release(github: payload(prerelease: true)))
  XCTAssertNil(Release(github: payload(tag: "nightly"))); XCTAssertNil(Release(github: Data(#"{"message":"Not Found"}"#.utf8)))
 }
 func testOnlyNewerVersionsAreOffered() {
  let release = Release(github: payload())!
  XCTAssertTrue(release.isNewer(than: "0.1.0")); XCTAssertFalse(release.isNewer(than: "0.2.0"))
  XCTAssertFalse(release.isNewer(than: "0.10.0")); XCTAssertFalse(release.isNewer(than: "unknown"))
 }
}
