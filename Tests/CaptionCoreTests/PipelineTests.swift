import XCTest
@testable import CaptionCore
final class PipelineTests: XCTestCase {
 func testSilenceDoesNotEmit() { var c = Chunker(); XCTAssertTrue(c.append(Array(repeating: 0, count: 320000)).isEmpty); XCTAssertNil(c.flush()) }
 func testOverlapAndTimestamps() {
  var c = Chunker(); let chunks = c.append(Array(repeating: 0.1, count: 176000))
  XCTAssertEqual(chunks.count, 2); XCTAssertEqual(chunks[0].start, 0); XCTAssertEqual(chunks[1].start, 5)
  XCTAssertEqual(chunks[0].samples.count, 96000); XCTAssertEqual(chunks[1].end, 11)
 }
 func testSilenceFlushAndPreRoll() {
  var c = Chunker(); _ = c.append(Array(repeating: 0, count: 16000))
  _ = c.append(Array(repeating: 0.1, count: 16000))
  let chunks = c.append(Array(repeating: 0, count: 9600))
  XCTAssertEqual(chunks.count, 1); XCTAssertEqual(chunks[0].start, 0.7, accuracy: 0.001)
  XCTAssertNil(c.flush())
 }
 func testFragmentationInvariant() {
  let samples = Array(repeating: Float(0.1), count: 192000)
  var a = Chunker(); var b = Chunker(); let expected = a.append(samples)
  var actual: [AudioChunk] = []
  for offset in stride(from: 0, to: samples.count, by: 137) { actual += b.append(Array(samples[offset..<min(offset+137,samples.count)])) }
  XCTAssertEqual(actual.map(\.start), expected.map(\.start)); XCTAssertEqual(actual.map(\.samples), expected.map(\.samples))
 }
 func testOverlapDeduplication() {
  var d = Deduplicator()
  XCTAssertEqual(d.accept(Caption(start: 0, end: 6, text: "Hello, this is a test."))?.text, "Hello, this is a test.")
  XCTAssertEqual(d.accept(Caption(start: 5, end: 10, text: "a test with more words"))?.text, "with more words")
  XCTAssertNil(d.accept(Caption(start: 5, end: 10, text: "with more words")))
 }
 func testLegitimateRepetitionAfterGap() {
  var d = Deduplicator(); _ = d.accept(Caption(start: 0, end: 1, text: "Yes"))
  XCTAssertEqual(d.accept(Caption(start: 2, end: 3, text: "Yes"))?.text, "Yes")
 }
 func testTimestampResetAfterGap() {
  var c = Chunker(start: 12.5)
  let chunks = c.append(Array(repeating: 0.1, count: 96000))
  XCTAssertEqual(chunks.first?.start, 12.5)
 }
 func testShortNoiseRejected() {
  var c = Chunker(); _ = c.append(Array(repeating: 0.1, count: 640))
  XCTAssertTrue(c.append(Array(repeating: 0, count: 16000)).isEmpty)
 }
 func testBoundedQueue() { var q = LatestQueue<Int>(); q.offer(1); q.offer(2); XCTAssertEqual(q.dropped,1); XCTAssertEqual(q.take(),2); XCTAssertNil(q.take()) }
}
