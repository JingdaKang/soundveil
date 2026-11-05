import Foundation
public enum SpeechEvent: Sendable {
 case audio([Float], start: Double)
 case pause(at: Double, voiced: Double) // A gap between phrases, and a safe place to cut. Voiced is seconds of speech since the last pause.
 case endpoint(voiced: Double)          // Speech ended; finalize what has been heard.
 case discard           // The burst was too short to be speech.
 case gap               // Capture was interrupted; do not join audio across it.
}
// Called exclusively on the capture queue. Frame-sized VAD keeps 300 ms of pre-roll and closes
// an utterance after 600 ms of silence. Bursts under 100 ms are discarded. Pauses are measured
// against the speaker's own recent level, so they are still found over a noise floor or music.
public struct SpeechGate {
 public var threshold: Float = 0.003
 private var pending: [Float] = []
 private var pre: [Float] = []
 private var clock = 0
 private var silence = 0
 private var voiced = 0
 private var quiet = 0
 private var phrase = 0
 private var level: Float = 0
 private var active = false
 public init(start: Double = 0) { clock = Int(start * 16000) }
 public mutating func append(_ samples: [Float]) -> [SpeechEvent] {
  pending += samples
  var events: [SpeechEvent] = []
  var heard: [Float] = []
  var heardStart = 0
  func flush() { if !heard.isEmpty { events.append(.audio(heard, start: Double(heardStart)/16000)); heard = [] } }
  var offset = 0
  while pending.count - offset >= 320 {
   let frame = Array(pending[offset..<offset+320]); offset += 320
   let rms = sqrt(frame.reduce(Float(0)) { $0 + $1*$1 } / 320)
   let speech = rms > threshold
   if !active, speech {
    active = true; heard = pre; heardStart = clock - pre.count; pre.removeAll(); silence = 0; voiced = 0; quiet = 0
   }
   if active {
    if heard.isEmpty { heardStart = clock }
    heard += frame
    if speech { silence = 0; voiced += 320 } else { silence += 320 }
    if !speech || rms < level * 0.2 { quiet += 320 } else { quiet = 0; phrase += 320; level = level == 0 ? rms : level*0.98 + rms*0.02 }
    if quiet == 3200 { flush(); events.append(.pause(at: Double(clock + 320 - 1600)/16000, voiced: Double(phrase)/16000)); phrase = 0 }
    if silence >= 9600 {
     flush(); events.append(voiced >= 1600 ? .endpoint(voiced: Double(phrase)/16000) : .discard)
     active = false; silence = 0; voiced = 0; quiet = 0; phrase = 0
    }
   } else { pre += frame; if pre.count > 4800 { pre.removeFirst(pre.count-4800) } }
   clock += 320
  }
  flush()
  pending.removeFirst(offset)
  return events
 }
}
public struct Word: Sendable, Equatable {
 public let text: String
 public let start: Double
 public let end: Double
 public init(text: String, start: Double, end: Double) { self.text=text; self.start=start; self.end=end }
 var key: String { text.lowercased().filter { $0.isLetter || $0.isNumber } }
 var endsSentence: Bool { text.hasSuffix(".") || text.hasSuffix("?") || text.hasSuffix("!") }
}
public struct Caption: Identifiable, Sendable {
 public let id = UUID()
 public let start: Double
 public let end: Double
 public let text: String
 public init(start: Double, end: Double, text: String) { self.start=start; self.end=end; self.text=text }
}
// Groups confirmed words into history lines at sentence ends, or after 24 words without one.
struct SentenceAssembler {
 private var words: [Word] = []
 mutating func add(_ new: [Word]) -> [Caption] {
  var result: [Caption] = []
  for word in new {
   words.append(word)
   if word.endsSentence || words.count >= 24 { if let line = take() { result.append(line) } }
  }
  return result
 }
 mutating func take() -> Caption? {
  guard let first = words.first, let last = words.last else { return nil }
  defer { words.removeAll() }
  return Caption(start: first.start, end: last.end, text: words.map(\.text).joined(separator: " "))
 }
}
