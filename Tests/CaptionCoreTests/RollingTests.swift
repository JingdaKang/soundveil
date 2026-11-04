import XCTest
@testable import CaptionCore
func placeWords(widths: [Double], space: Double, maxWidth: Double) -> [PlacedWord] {
 var placer = WordPlacer(space: space, maxWidth: maxWidth)
 return widths.map { placer.place(width: $0) }
}
final class RollingTests: XCTestCase {
 func fast() -> RollingTranscriber { var t = RollingTranscriber.init(); t.step = 0.2; return t }
 func words(_ text: String, from start: Double = 0, each: Double = 0.4) -> [Word] {
  text.split(separator: " ").enumerated().map { Word(text: String($1), start: start + Double($0)*each, end: start + Double($0+1)*each) }
 }
 func texts(_ words: [Word]?) -> String { (words ?? []).map(\.text).joined(separator: " ") }
 func audio(_ seconds: Double, at start: Double = 0, level: Float = 0.1) -> SpeechEvent { .audio(Array(repeating: level, count: Int(seconds*16000)), start: start) }
 func testAudioContextOutrunsTheLongestPhrase() {
  let t = fast(); XCTAssertGreaterThan(Double(t.audioContext), t.maxPhrase * 50 + 100)
 }
 func testEveryPassUsesTheSameAudioContext() {
  var t = fast(); t.ingest(audio(3)); let partial = t.nextJob()!
  _ = t.complete(partial, words: []); t.ingest(.pause(at: 2.9, voiced: 2.5)); XCTAssertEqual(t.nextJob()!.audioContext, partial.audioContext)
 }
 func testTokenLimitAllowsFastSpeech() { XCTAssertGreaterThan(tokenLimit(forSeconds: 10), 60) }
 func testLoopsAreCut() {
  XCTAssertEqual(texts(removingLoops(words("what your your your your"))), "what your")
  XCTAssertEqual(texts(removingLoops(words("so you are, you are, you are"))), "so you are,")
  XCTAssertEqual(texts(removingLoops(words("no no we agree"))), "no no we agree")
 }
 func testQuietestPointFindsTheDip() {
  var samples = [Float](repeating: 0.2, count: 16000)
  for i in 9000..<10000 { samples[i] = 0 }
  XCTAssertEqual(Double(quietestPoint(in: samples)), 9500, accuracy: 300)
 }
 func testNoPassBeforeFirstDecode() {
  var t = fast(); t.ingest(audio(0.5)); XCTAssertNil(t.nextJob())
  t.ingest(audio(0.5, at: 0.5)); XCTAssertNotNil(t.nextJob())
 }
 func testPartialsUseGreedyAndFinalsUseBeam() {
  var t = fast(); t.ingest(audio(3)); let partial = t.nextJob()!; XCTAssertEqual(partial.beam, 1)
  _ = t.complete(partial, words: []); t.ingest(.pause(at: 2.9, voiced: 2.5)); let final = t.nextJob()!
  XCTAssertTrue(final.isFinal); XCTAssertEqual(final.beam, 5)
 }
 func testOnlyOnePassRunsAtATime() {
  var t = fast(); t.ingest(audio(2)); let job = t.nextJob()!; XCTAssertNil(t.nextJob())
  t.ingest(audio(0.5, at: 2)); XCTAssertNil(t.nextJob())
  _ = t.complete(job, words: []); XCTAssertNotNil(t.nextJob())
 }
 func testNoPassWithoutEnoughNewAudio() {
  var t = fast(); t.ingest(audio(2)); _ = t.complete(t.nextJob()!, words: words("One"))
  XCTAssertNil(t.nextJob()); t.ingest(audio(0.25, at: 2)); XCTAssertNotNil(t.nextJob())
 }
 func pass(_ t: inout RollingTranscriber, _ text: String, at time: inout Double) -> CaptionUpdate {
  let update = t.complete(t.nextJob()!, words: words(text)); t.ingest(audio(0.25, at: time)); time += 0.25
  return update
 }
 func testOnlyOneWordIsShownAheadOfAgreement() {
  var t = fast(), time = 2.0; t.ingest(audio(2))
  XCTAssertEqual(texts(pass(&t, "one two three", at: &time).tentative), "one")
  let update = pass(&t, "one two three four", at: &time)
  XCTAssertEqual(texts(update.tentative), "one two three four"); XCTAssertEqual(update.stable, 3)
 }
 func testOneOutlierPassCannotChangeTheScreen() {
  var t = fast(), time = 2.0; t.ingest(audio(2))
  _ = pass(&t, "ask what you can", at: &time); _ = pass(&t, "ask what you can", at: &time)
  XCTAssertEqual(texts(pass(&t, "to the to", at: &time).tentative), "ask what you can")
 }
 func testTwoPassesCanChangeTheScreen() {
  var t = fast(), time = 2.0; t.ingest(audio(2))
  _ = pass(&t, "one two three", at: &time); _ = pass(&t, "one two three", at: &time)
  _ = pass(&t, "one to three four", at: &time)
  XCTAssertEqual(texts(pass(&t, "one to three four", at: &time).tentative), "one to three four")
 }
 func testWordsAreNotTakenBackWhenAPassIsShorter() {
  var t = fast(), time = 2.0; t.ingest(audio(2))
  _ = pass(&t, "one two three", at: &time); _ = pass(&t, "one two three", at: &time)
  XCTAssertEqual(texts(pass(&t, "one", at: &time).tentative), "one two three")
 }
 func testPauseCutsThePhraseExactlyThere() {
  var t = fast(); t.ingest(audio(3)); t.ingest(.pause(at: 2.5, voiced: 2.4)); let job = t.nextJob()!
  XCTAssertTrue(job.isFinal); XCTAssertEqual(job.samples.count, 40000); XCTAssertEqual(t.audioEnd, 3, accuracy: 0.001)
 }
 func testShortPhraseCarriesOnThroughAPause() {
  var t = fast(); t.ingest(audio(1.5)); t.ingest(.pause(at: 1.4, voiced: 1.2)); XCTAssertFalse(t.nextJob()!.isFinal)
 }
 func testFinalPassCommitsAndReplacesTentative() {
  var t = fast(); t.ingest(audio(3)); _ = t.complete(t.nextJob()!, words: words("Hello there"))
  t.ingest(.endpoint(voiced: 2.5)); let last = t.nextJob()!; XCTAssertTrue(last.isFinal)
  let update = t.complete(last, words: words("Hello there, friend."))
  XCTAssertEqual(texts(update.committed), "Hello there, friend."); XCTAssertEqual(update.tentative?.count, 0)
  XCTAssertEqual(update.sentences.map(\.text), ["Hello there, friend."]); XCTAssertNil(t.nextJob())
 }
 func testPartialFinishingAfterACutIsIgnored() {
  var t = fast(); t.ingest(audio(3)); let partial = t.nextJob()!; t.ingest(.pause(at: 2.9, voiced: 2.5))
  let update = t.complete(partial, words: words("Stale words")); XCTAssertNil(update.tentative)
  XCTAssertTrue(t.nextJob()!.isFinal)
 }
 func testTailOfAPauseIsNotDecoded() {
  var t = fast(); t.ingest(audio(3)); t.ingest(.pause(at: 2.9, voiced: 2.5)); _ = t.complete(t.nextJob()!, words: words("Done."))
  t.ingest(audio(0.4, at: 3, level: 0)); t.ingest(.endpoint(voiced: 0)); XCTAssertEqual(t.nextJob()?.samples.count, 0)
 }
 func testNextPhraseWaitsForTheFinalPass() {
  var t = fast(); t.ingest(audio(3)); t.ingest(.endpoint(voiced: 2.5))
  let last = t.nextJob()!; t.ingest(audio(2, at: 5)); XCTAssertNil(t.nextJob()); _ = t.complete(last, words: words("Done."))
  let next = t.nextJob()!; XCTAssertFalse(next.isFinal); XCTAssertEqual(next.start, 5)
 }
 func testLongRunIsCutAtItsQuietestPoint() {
  var t = fast()
  var samples = [Float](repeating: 0.2, count: 9*16000)
  for i in (7*16000)..<(7*16000+2400) { samples[i] = 0 }
  t.ingest(.audio(samples, start: 0)); let job = t.nextJob()!
  XCTAssertTrue(job.isFinal); XCTAssertEqual(job.end, 7.07, accuracy: 0.1)
 }
 func testSilentPhraseIsNotDecodedAndClearsTheScreen() {
  var t = fast(); t.ingest(audio(3)); _ = t.complete(t.nextJob()!, words: words("Invented"))
  t.ingest(.endpoint(voiced: 0.1)); let job = t.nextJob()!; XCTAssertTrue(job.samples.isEmpty)
  XCTAssertEqual(t.complete(job, words: []).tentative?.count, 0)
 }
 func testFinalPhrasesAreBoundedAndReportLostCoverage() {
  var t = fast(); t.capacity = 10
  for i in 0..<6 { t.ingest(audio(3, at: Double(i)*3)); t.ingest(.endpoint(voiced: 2.5)) }
  XCTAssertGreaterThan(t.skipped, 0)
 }
 func testStopKeepsTentativeWords() {
  var t = fast(); t.ingest(audio(2)); _ = t.complete(t.nextJob()!, words: words("Almost done"))
  XCTAssertEqual(texts(t.finish().committed), "Almost done")
 }
 func testDiscardDuringAPassClearsItsWords() {
  var t = fast(); t.ingest(audio(1)); let job = t.nextJob()!; t.ingest(.discard)
  _ = t.complete(job, words: words("Noise")); let clear = t.nextJob()!; XCTAssertTrue(clear.samples.isEmpty)
  XCTAssertEqual(t.complete(clear, words: []).tentative?.count, 0)
 }
 func testAutomaticKeepsTheGPUUnderHalfLoad() {
  XCTAssertEqual(updateInterval(.auto, passTime: 0.1, constrained: false), 0.4)
  XCTAssertEqual(updateInterval(.auto, passTime: 0.6, constrained: false), 1.2)
  XCTAssertEqual(updateInterval(.auto, passTime: 9, constrained: false), 3)
 }
 func testAutomaticSlowsDownUnderPressure() {
  XCTAssertEqual(updateInterval(.auto, passTime: 0.1, constrained: true), 0.8)
  XCTAssertEqual(updateInterval(.auto, passTime: 2, constrained: true), 3)
 }
 func testFixedSpeedsAreAlwaysHonored() {
  XCTAssertEqual(updateInterval(.fastest, passTime: 2, constrained: true), 0.2)
  XCTAssertEqual(updateInterval(.saver, passTime: 0, constrained: false), 1.5)
 }
 func testEmptyPassesBackOff() {
  var t = fast(); t.ingest(audio(2))
  _ = t.complete(t.nextJob()!, words: [])
  t.ingest(audio(0.25, at: 2)); XCTAssertNil(t.nextJob())
  t.ingest(audio(0.2, at: 2.25)); XCTAssertNotNil(t.nextJob())
 }
 func testWordsEndTheBackOff() {
  var t = fast(); t.ingest(audio(2))
  _ = t.complete(t.nextJob()!, words: []); t.ingest(audio(0.5, at: 2))
  _ = t.complete(t.nextJob()!, words: words("Back")); t.ingest(audio(0.25, at: 2.5)); XCTAssertNotNil(t.nextJob())
 }
 func testWrappingBreaksOnWidth() {
  let placed = placeWords(widths: [60, 60, 60], space: 10, maxWidth: 135)
  XCTAssertEqual(placed.map(\.line), [0, 0, 1]); XCTAssertEqual(placed[1].x, 70); XCTAssertEqual(placed[2].x, 0)
 }
 func testAppendingNeverMovesEarlierWords() {
  let widths = (0..<40).map { Double(20 + $0 * 7 % 50) }
  let all = placeWords(widths: widths, space: 6, maxWidth: 300)
  for n in 1..<widths.count { XCTAssertEqual(placeWords(widths: Array(widths.prefix(n)), space: 6, maxWidth: 300), Array(all.prefix(n))) }
 }
 func testOversizedWordKeepsItsOwnLine() { XCTAssertEqual(placeWords(widths: [500, 20], space: 5, maxWidth: 300).map(\.line), [0, 1]) }
 func testRevealDrainsABatchQuickly() {
  XCTAssertLessThanOrEqual(revealInterval(backlog: 2) * 2, 0.3); XCTAssertLessThanOrEqual(revealInterval(backlog: 30) * 30, 0.7)
  XCTAssertEqual(revealInterval(backlog: 1), 0.09); XCTAssertGreaterThan(revealInterval(backlog: 1000), 0)
 }
}
