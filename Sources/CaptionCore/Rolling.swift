import Foundation
public struct DecodeJob: Sendable {
 public let samples: [Float]
 public let start: Double
 public let audioContext: Int
 public let maxTokens: Int
 public let beam: Int32
 public let isFinal: Bool
 public var end: Double { start + Double(samples.count)/16000 }
}
public struct CaptionUpdate: Sendable {
 public var committed: [Word] = []   // Final text. Replaces the tentative words it covers.
 public var tentative: [Word]? = nil // The phrase in progress; nil means unchanged.
 public var stable = 0               // Leading tentative words that two passes agreed on.
 public var sentences: [Caption] = []
 public init() {}
}
// Fast English speech is under 5 tokens a second, so a longer result is Whisper looping.
func tokenLimit(forSeconds seconds: Double) -> Int { Int(seconds * 6) + 12 }
// Cuts a hypothesis where a phrase of up to 4 words repeats three times running. Whisper
// loops like this at the edge of the audio.
func removingLoops(_ words: [Word]) -> [Word] {
 let keys = words.map(\.key)
 for i in keys.indices {
  for n in 1...4 where i + 3*n <= keys.count {
   if keys[i..<i+n] == keys[i+n..<i+2*n], keys[i..<i+n] == keys[i+2*n..<i+3*n] { return Array(words[..<(i+n)]) }
  }
 }
 return words
}
func commonPrefix(_ a: [Word], _ b: [Word]) -> Int { zip(a, b).prefix { $0.key == $1.key }.count }
// Finds the quietest 100 ms of a recording, the safest place to cut a run with no pause.
func quietestPoint(in samples: [Float]) -> Int {
 var best = 0, lowest = Float.infinity
 for start in stride(from: 0, to: max(1, samples.count - 1600), by: 160) {
  let energy = stride(from: start, to: min(samples.count, start + 1600), by: 4).reduce(Float(0)) { $0 + samples[$1]*samples[$1] }
  if energy < lowest { lowest = energy; best = start }
 }
 return best + 800
}
// Whisper has no native streaming, and its word timestamps are too unreliable to cut audio
// on. So partial passes over the current phrase are for display only, and the transcript comes
// from one final pass over each phrase, cut at a pause. This is a pure state machine, so the
// policy is testable without audio or a model. The capture queue ingests events and the
// inference queue pulls one job at a time, under Inference's lock.
public struct RollingTranscriber {
 public var firstDecode = 0.8    // Seconds of speech before the first pass.
 public var step = 0.2           // New audio required between passes.
 public var minPhrase = 2.5      // Shorter phrases carry on through a pause.
 public var maxPhrase = 9.0      // Longer phrases are cut at their quietest point.
 public var capacity = 24.0      // Seconds kept. Past this, inference has stalled and audio is dropped.
 // Encoder frames, at 50 a second. Cost scales with this, so it is far below Whisper's 1500, but
 // it must outrun the longest phrase or words are lost. It must also be the same for every
 // pass: mixing sizes in one whisper context corrupted the passes that followed.
 public var audioContext = 640
 public var tailWords = 1        // Words shown beyond what two passes agreed on.
 public var minVoiced = 0.25     // Phrases with less speech are not decoded; Whisper invents text from silence.
 public var partialBeam: Int32 = 1
 public var finalBeam: Int32 = 5
 public private(set) var skipped = 0.0
 private var buffer: [Float] = []
 private var bufferStart = 0.0
 private var decodedEnd = 0.0
 private var closing: [(samples: [Float], start: Double)] = []
 private var busy = false
 private var previous: [Word] = []
 private var shown: [Word] = []
 private var voiced = 0.0
 private var assembler = SentenceAssembler()
 public init() {}
 public var audioEnd: Double { bufferStart + Double(buffer.count)/16000 }
 private var length: Double { Double(buffer.count)/16000 }
 public mutating func ingest(_ event: SpeechEvent) {
  switch event {
  case .audio(let samples, let start):
   if buffer.isEmpty { bufferStart = start; decodedEnd = start }
   buffer += samples
   if length >= maxPhrase {
    let tail = max(0, buffer.count - 64000)
    close(samples: tail + quietestPoint(in: Array(buffer[tail..<max(tail, buffer.count - 4800)])), force: true)
   }
  case .pause(let time, let heard):
   voiced += heard
   if length >= minPhrase, time > bufferStart { close(samples: min(buffer.count, Int((time - bufferStart) * 16000))) }
  case .endpoint(let heard):
   voiced += heard
   close(samples: buffer.count)
  case .gap:
   close(samples: buffer.count, force: true)
  case .discard:
   buffer = []; voiced = 0; closing.append(([], bufferStart))
  }
 }
 private mutating func close(samples count: Int, force: Bool = false) {
  guard count > 0 else { return }
  // A phrase that is not speech closes empty, which still clears anything shown for it.
  closing.append((voiced >= minVoiced || force ? Array(buffer[..<count]) : [], bufferStart))
  voiced = 0
  buffer.removeFirst(count); bufferStart += Double(count)/16000; decodedEnd = bufferStart
  let held = closing.reduce(0.0) { $0 + Double($1.samples.count)/16000 }
  while closing.count > 1, held > capacity { skipped += Double(closing[0].samples.count)/16000; closing.removeFirst(); break }
 }
 // Phrases are finalized in order and first, so a slow machine falls behind on the words
 // still being refined rather than on the transcript.
 public mutating func nextJob() -> DecodeJob? {
  guard !busy else { return nil }
  if let phrase = closing.first {
   busy = true
   let seconds = Double(phrase.samples.count)/16000
   return DecodeJob(samples: phrase.samples, start: phrase.start, audioContext: audioContext, maxTokens: tokenLimit(forSeconds: seconds), beam: finalBeam, isFinal: true)
  }
  guard length >= firstDecode, audioEnd - decodedEnd >= step else { return nil }
  busy = true; decodedEnd = audioEnd
  return DecodeJob(samples: buffer, start: bufferStart, audioContext: audioContext, maxTokens: tokenLimit(forSeconds: length), beam: partialBeam, isFinal: false)
 }
 public mutating func complete(_ job: DecodeJob, words: [Word]) -> CaptionUpdate {
  busy = false
  var update = CaptionUpdate()
  let pass = removingLoops(words)
  if job.isFinal {
   closing.removeFirst(); previous = []; shown = []
   update.committed = pass; update.tentative = []
   update.sentences = assembler.add(pass)
  } else if closing.isEmpty, !buffer.isEmpty {
   // Whisper's guess at a cut-off phrase can swing wildly from one pass to the next, so what
   // is on screen only changes when two passes in a row agree. A lone outlier is ignored.
   // New words at the end are the exception: one is shown ahead of agreement, to keep up.
   let agreed = commonPrefix(previous, pass), kept = commonPrefix(shown, pass)
   if kept == shown.count { shown = Array(pass.prefix(max(kept, agreed + tailWords))) }
   else if agreed > kept { shown = Array(pass.prefix(agreed + tailWords)) }
   previous = pass
   update.tentative = shown; update.stable = min(agreed, shown.count)
  }
  return update
 }
 // Called on Stop: nothing more will be decoded, so keep what was heard.
 public mutating func finish() -> CaptionUpdate {
  var update = CaptionUpdate()
  update.committed = previous.count > shown.count ? previous : shown; update.tentative = []
  update.sentences = assembler.add(update.committed)
  if let line = assembler.take() { update.sentences.append(line) }
  return update
 }
}
// Greedy wrapping. Each word's position depends only on the words before it, so appended
// words never move text that is already on screen.
public struct PlacedWord: Sendable, Equatable {
 public let line: Int
 public let x: Double
}
public struct WordPlacer: Sendable {
 public var line = 0
 public var x = 0.0
 public var space: Double
 public var maxWidth: Double
 public init(space: Double, maxWidth: Double) { self.space=space; self.maxWidth=maxWidth }
 public mutating func place(width: Double) -> PlacedWord {
  if x > 0, x + width > maxWidth { line += 1; x = 0 }
  defer { x += width + space }
  return PlacedWord(line: line, x: x)
 }
}
// Staggers a batch of words so it reads as a flow. A batch always drains within about 0.3 s,
// the time between passes, so pacing cannot build up a backlog of delay.
public func revealInterval(backlog: Int) -> Double { min(0.09, max(0.02, 0.28 / Double(max(1, backlog)))) }
