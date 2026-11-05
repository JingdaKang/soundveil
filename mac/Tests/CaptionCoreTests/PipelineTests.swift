import XCTest
@testable import CaptionCore
final class PipelineTests: XCTestCase {
 func speech(_ count: Int) -> [Float] { Array(repeating: 0.1, count: count) }
 func silence(_ count: Int) -> [Float] { Array(repeating: 0, count: count) }
 func heard(_ events: [SpeechEvent]) -> (samples: Int, starts: [Double], endpoints: Int) {
  var samples = 0, starts: [Double] = [], endpoints = 0
  for event in events {
   if case .audio(let s, let start) = event { samples += s.count; starts.append(start) }
   if case .endpoint = event { endpoints += 1 }
  }
  return (samples, starts, endpoints)
 }
 func testSilenceEmitsNothing() { var g = SpeechGate(); XCTAssertTrue(g.append(silence(320000)).isEmpty) }
 func testSpeechIncludesPreRoll() {
  var g = SpeechGate(); _ = g.append(silence(16000))
  let events = g.append(speech(16000))
  XCTAssertEqual(heard(events).starts.first!, 0.7, accuracy: 0.001); XCTAssertEqual(heard(events).samples, 20800)
 }
 func testSilenceClosesUtterance() {
  var g = SpeechGate(); _ = g.append(speech(16000))
  XCTAssertEqual(heard(g.append(silence(9600))).endpoints, 1)
 }
 func pauses(_ events: [SpeechEvent]) -> [Double] { events.compactMap { if case .pause(let t, _) = $0 { return t }; return nil } }
 func testShortPauseIsReported() {
  var g = SpeechGate(); _ = g.append(speech(16000))
  let events = g.append(silence(4000))
  XCTAssertEqual(pauses(events).count, 1); XCTAssertEqual(pauses(events)[0], 1.1, accuracy: 0.03)
 }
 func testBriefDipIsNotAPause() {
  var g = SpeechGate(); _ = g.append(speech(16000))
  XCTAssertTrue(pauses(g.append(silence(1600) + speech(1600))).isEmpty)
 }
 func testPauseFoundOverANoiseFloor() {
  var g = SpeechGate(); _ = g.append(speech(32000))
  XCTAssertEqual(pauses(g.append(Array(repeating: 0.01, count: 4000))).count, 1)
 }
 func testShortNoiseIsDiscarded() {
  var g = SpeechGate(); _ = g.append(speech(640))
  let events = g.append(silence(16000))
  XCTAssertEqual(heard(events).endpoints, 0)
  XCTAssertTrue(events.contains { if case .discard = $0 { return true }; return false })
 }
 func testFragmentationInvariant() {
  let samples = speech(32000) + silence(12800)
  var a = SpeechGate(); var b = SpeechGate(); let expected = heard(a.append(samples))
  var events: [SpeechEvent] = []
  for offset in stride(from: 0, to: samples.count, by: 137) { events += b.append(Array(samples[offset..<min(offset+137,samples.count)])) }
  XCTAssertEqual(heard(events).samples, expected.samples); XCTAssertEqual(heard(events).endpoints, 1)
 }
 func testTimestampsContinueAfterGap() {
  var g = SpeechGate(start: 12.5)
  XCTAssertEqual(heard(g.append(speech(16000))).starts.first!, 12.5, accuracy: 0.001)
 }
 func testSentencesGroupIntoHistoryLines() {
  var a = SentenceAssembler()
  let lines = a.add([Word(text: "Hello", start: 0, end: 1), Word(text: "there.", start: 1, end: 2), Word(text: "Next", start: 2, end: 3)])
  XCTAssertEqual(lines.map(\.text), ["Hello there."]); XCTAssertEqual(a.take()?.text, "Next")
 }
 func testLongUnpunctuatedRunStillReachesHistory() {
  var a = SentenceAssembler()
  XCTAssertEqual(a.add((0..<24).map { Word(text: "w\($0)", start: Double($0), end: Double($0)+1) }).count, 1)
 }
}
