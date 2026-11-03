import Foundation
public struct AudioChunk: Sendable {
 public let samples: [Float]
 public let start: Double
 public var end: Double { start + Double(samples.count) / 16000 }
 public init(samples: [Float], start: Double) { self.samples = samples; self.start = start }
}
// Called exclusively on the capture queue. Frame-sized VAD preserves pre-roll and
// closes utterances after 600ms silence; long speech emits 6s windows with 1s overlap.
public struct Chunker {
 public var threshold: Float = 0.003
 private var pending: [Float] = []
 private var active: [Float] = []
 private var pre: [Float] = []
 private var clock = 0
 private var start = 0
 private var silence = 0
 private var voiced = 0
 public init(start: Double = 0) { clock = Int(start * 16000) }
 public mutating func append(_ samples: [Float]) -> [AudioChunk] {
  pending += samples
  var result: [AudioChunk] = []
  var offset = 0
  while pending.count - offset >= 320 {
   let frame = Array(pending[offset..<offset+320]); offset += 320
   let rms = sqrt(frame.reduce(Float(0)) { $0 + $1*$1 } / 320)
   let speech = rms > threshold
   if active.isEmpty {
    if speech { start = clock - pre.count; active = pre; pre.removeAll(); voiced = 0 }
    else { pre += frame; if pre.count > 4800 { pre.removeFirst(pre.count-4800) } }
   }
   if !active.isEmpty || speech {
    active += frame
    if speech { silence = 0; voiced += 320 } else { silence += 320 }
    if active.count >= 96000 {
     result.append(AudioChunk(samples: active, start: Double(start)/16000))
     active = Array(active.suffix(16000)); start = clock + 320 - active.count; voiced = 0
    } else if silence >= 9600 {
     if voiced >= 1600 { result.append(AudioChunk(samples: active, start: Double(start)/16000)) }
     pre = Array(active.suffix(4800)); active.removeAll(); silence = 0; voiced = 0
    }
   }
   clock += 320
  }
  pending.removeFirst(offset)
  return result
 }
 public mutating func flush() -> AudioChunk? {
  defer { self = Chunker() }
  guard voiced >= 1600 else { return nil }
  return AudioChunk(samples: active + pending, start: Double(start)/16000)
 }
}
public struct Caption: Identifiable, Sendable {
 public let id = UUID()
 public let start: Double
 public let end: Double
 public let text: String
 public init(start: Double, end: Double, text: String) { self.start=start; self.end=end; self.text=text }
}
public struct Deduplicator {
 private var words: [String] = []
 private var lastEnd: Double = -1
 public init() {}
 private func normalize(_ s: String) -> String { s.lowercased().filter { $0.isLetter || $0.isNumber } }
 public mutating func accept(_ caption: Caption) -> Caption? {
  let tokens = caption.text.split(whereSeparator: { $0.isWhitespace }).map(String.init)
  guard !tokens.isEmpty, caption.end > lastEnd + 0.05 else { return nil }
  var skip = 0
  if caption.start < lastEnd + 0.15 {
   for n in stride(from: min(words.count, tokens.count, 40), through: 1, by: -1) {
    if words.suffix(n).map(normalize) == tokens.prefix(n).map(normalize) { skip=n; break }
   }
  }
  lastEnd = max(lastEnd, caption.end)
  guard skip < tokens.count else { return nil }
  let fresh = Array(tokens.dropFirst(skip)); words = Array((words + fresh).suffix(80))
  return Caption(start: caption.start, end: caption.end, text: fresh.joined(separator: " "))
 }
}
// Single pending slot: latency cannot grow without bound on slower machines.
public struct LatestQueue<Element> {
 private var value: Element?
 public private(set) var dropped = 0
 public init() {}
 public mutating func offer(_ item: Element) { if value != nil { dropped += 1 }; value = item }
 public mutating func take() -> Element? { defer { value = nil }; return value }
}
