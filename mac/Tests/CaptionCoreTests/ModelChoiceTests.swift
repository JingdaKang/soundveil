import XCTest
@testable import CaptionCore

final class ModelChoiceTests: XCTestCase {
 func testNewInstallsUseTurbo() {
  XCTAssertEqual(Model.saved(nil), .turbo)
 }
 func testUnknownSavedChoiceFallsBackToTurbo() {
  XCTAssertEqual(Model.saved("large-v9"), .turbo)
 }
 func testEveryChoiceSurvivesSaving() {
  for model in Model.allCases { XCTAssertEqual(Model.saved(model.rawValue), model) }
 }
 func testFileNamesMatchTheModelCollection() {
  XCTAssertEqual(Model.allCases.map(\.fileName), ["ggml-base.en.bin", "ggml-small.en.bin", "ggml-large-v3-turbo-q5_0.bin", "ggml-large-v3-turbo.bin", "ggml-large-v3.bin"])
 }
}
