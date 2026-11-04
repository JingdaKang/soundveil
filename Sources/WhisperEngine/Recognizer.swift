import Foundation
import CaptionCore
import CWhisperBridge

public final class Recognizer: @unchecked Sendable {
 private let context: FCContext
 public var allowCues = true
 public init?(path: String) {
  guard let context = fc_create(path) else { return nil }
  self.context = context
 }
 deinit { fc_destroy(context) }
 public func decode(_ job: DecodeJob, beam: Int32? = nil) -> [Word]? {
  guard !job.samples.isEmpty else { return [] }
  let code = job.samples.withUnsafeBufferPointer { samples in
   fc_stream(context, samples.baseAddress, Int32(samples.count), Int32(job.audioContext), beam ?? job.beam, Int32(job.maxTokens), job.isFinal && allowCues ? 1 : 0)
  }
  guard code == 0 else { return nil }
  return words(in: job)
 }
 // Whisper's tokens are fragments of words, and of UTF-8 sequences, so bytes are joined
 // before decoding. A token that opens with a space starts a new word. Times are estimated
 // from each word's share of the text, which is close enough for history timestamps.
 private func words(in job: DecodeJob) -> [Word] {
  var texts: [String] = [], bytes: [UInt8] = [], note = ""
  func close() {
   defer { bytes = [] }
   guard !bytes.isEmpty else { return }
   var text = String(decoding: bytes, as: UTF8.self).trimmingCharacters(in: .whitespaces)
   // Non-speech notes can span tokens, such as "[" "MUSIC" "]". Known ones become a cue.
   if note.isEmpty, text.hasPrefix("[") || text.hasPrefix("(") || text.hasPrefix("♪") || text.hasPrefix("♫") || text.hasPrefix("*") { note = " " }
   if !note.isEmpty {
    note += text + " "
    if text.hasSuffix("]") || text.hasSuffix(")") || text.hasSuffix("♪") || text.hasSuffix("♫") || text.hasSuffix("*") {
     if let label = cueLabel(for: note), texts.last != "[\(label)]" { texts.append("[\(label)]") }
     note = ""
    }
    return
   }
   while text.hasSuffix("…") { text.removeLast() }
   while text.hasSuffix("...") { text.removeLast(3) }
   if !text.isEmpty { texts.append(text) }
  }
  for segment in 0..<fc_count(context) {
   for token in 0..<fc_tokens(context, segment) where fc_token_is_text(context, segment, token) != 0 {
    guard let raw = fc_token_text(context, segment, token) else { continue }
    let piece = Array(UnsafeBufferPointer(start: UnsafeRawPointer(raw).assumingMemoryBound(to: UInt8.self), count: strlen(raw)))
    if piece.first == 0x20 { close() }
    bytes += piece
   }
   close()
  }
  let total = Double(max(1, texts.reduce(0) { $0 + $1.count + 1 }))
  var offset = 0.0
  return texts.map { text in
   let begin = offset; offset += Double(text.count + 1)
   return Word(text: text, start: job.start + begin/total * (job.end - job.start), end: job.start + offset/total * (job.end - job.start))
  }
 }
}
