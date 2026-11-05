import Foundation
import CaptionCore

// Prints everything the caption logic does on seeded random input, with a scripted decoder.
// core/soundveil-core/examples/trace.rs is the same program in Rust, so identical output shows
// the two implementations agree. core/scripts/compare-with-swift.sh runs both.
struct Rng {
 var state: UInt64
 mutating func next() -> UInt64 {
  state &+= 0x9E3779B97F4A7C15
  var z = state
  z = (z ^ (z >> 30)) &* 0xBF58476D1CE4E5B9
  z = (z ^ (z >> 27)) &* 0x94D049BB133111EB
  return z ^ (z >> 31)
 }
 mutating func unit() -> Double { Double(next() >> 11) / 9007199254740992 }
 mutating func below(_ n: Int) -> Int { Int(next() % UInt64(n)) }
 mutating func pick(_ list: [String]) -> String { list[below(list.count)] }
}
let vocabulary = ["Hello", "hello,", "there.", "What", "what", "your", "your,", "no", "No", "way!", "way", "cap", "cap,", "go", "let's", "Let's", "oh", "my", "god!", "ha", "Ha", "haha", "Hahaha", "sooooo", "1000", "5th", "boom", "Boom.", "boom!", "based", "Based.", "sus", "bruh", "[laughter]", "[music]", "um,", "?", "sure?", "café", "ÉCOLE", "école", "Straße", "١٢٣", "½", "it's", "giving", "tick", "tock", "ding", "dong"]
let notes = ["[MUSIC PLAYING]", "(audience laughing)", "♪♪", "[Applause]", "[BLANK_AUDIO]", "(speaking foreign language)", "*cheering*", "(Chuckles)", "♫ singing ♫", "[clapping]", "", "plain"]
func number(_ value: Double) -> String { String(format: "%.9f", value) }
func checksum(_ samples: [Float]) -> UInt64 {
 var hash: UInt64 = 14695981039346656037
 for sample in samples { hash = (hash ^ UInt64(sample.bitPattern)) &* 1099511628211 }
 return hash
}
func show(_ words: [Word]) -> String { words.map { "\($0.text)@\(number($0.start))-\(number($0.end))" }.joined(separator: " ") }
func describe(_ event: SpeechEvent) -> String {
 switch event {
 case .audio(let samples, let start): return "audio \(samples.count) \(number(start)) \(checksum(samples))"
 case .pause(let at, let voiced): return "pause \(number(at)) \(number(voiced))"
 case .endpoint(let voiced): return "endpoint \(number(voiced))"
 case .discard: return "discard"
 case .gap: return "gap"
 }
}
func describe(_ job: DecodeJob) -> String {
 "job \(job.samples.count) \(number(job.start)) \(number(job.end)) \(job.audioContext) \(job.maxTokens) \(job.beam) \(job.isFinal) \(checksum(job.samples))"
}
func describe(_ update: CaptionUpdate) -> String {
 "update [\(show(update.committed))] [\(update.tentative.map(show) ?? "nil")] \(update.stable) [\(update.sentences.map { "\($0.text)@\(number($0.start))-\(number($0.end))" }.joined(separator: " | "))]"
}
func decode(_ job: DecodeJob, _ rng: inout Rng, last: inout [String]) -> [Word] {
 var texts: [String] = []
 switch rng.below(6) {
 case 0: break
 case 1, 2: texts = last + [rng.pick(vocabulary)]
 case 3: texts = Array(last.prefix(rng.below(last.count + 1))) + [rng.pick(vocabulary), rng.pick(vocabulary)]
 case 4:
  let phrase = (0..<(1 + rng.below(4))).map { _ in rng.pick(vocabulary) }
  texts = [rng.pick(vocabulary)]
  for _ in 0..<(2 + rng.below(3)) { texts += phrase }
 default: texts = (0..<rng.below(13)).map { _ in rng.pick(vocabulary) }
 }
 last = texts
 if job.samples.isEmpty { return [] }
 return texts.enumerated().map { Word(text: $1, start: job.start + Double($0)*0.3, end: job.start + Double($0+1)*0.3) }
}
let seeds = CommandLine.arguments.dropFirst().first.flatMap { Int($0) } ?? 300
for seed in 0..<seeds {
 var rng = Rng(state: UInt64(seed))
 print("seed \(seed)")
 var gate = SpeechGate(start: [0, 12.5][rng.below(2)])
 var transcriber = RollingTranscriber()
 transcriber.step = [0.2, 0.4, 0.75, 1.5][rng.below(4)]
 transcriber.capacity = [24, 6][rng.below(2)]
 transcriber.maxPhrase = [9, 5][rng.below(2)]
 var running: DecodeJob?, hold = 0, last: [String] = []
 func pump() {
  if let job = running {
   if hold > 0 { hold -= 1; return }
   running = nil
   print(describe(transcriber.complete(job, words: decode(job, &rng, last: &last))))
  }
  if let job = transcriber.nextJob() { print(describe(job)); running = job; hold = rng.below(4) }
 }
 for _ in 0..<80 {
  let kind = rng.below(10)
  let seconds = 0.02 + rng.unit() * (kind < 6 ? 2.5 : 1)
  let level: Float = kind < 6 ? 0.02 + Float(rng.unit()) * 0.28 : kind < 8 ? 0 : 0.001 + Float(rng.unit()) * 0.009
  var samples = [Float](repeating: 0, count: Int(seconds * 16000))
  for i in samples.indices { samples[i] = Float(rng.unit() * 2 - 1) * level }
  var offset = 0
  while offset < samples.count {
   let end = min(samples.count, offset + 1 + rng.below(3000))
   for event in gate.append(Array(samples[offset..<end])) { print(describe(event)); transcriber.ingest(event) }
   offset = end
   pump()
  }
  if rng.below(20) == 0 { print(describe(SpeechEvent.gap)); transcriber.ingest(.gap); pump() }
  if rng.below(8) == 0 {
   let speed = UpdateSpeed.allCases[rng.below(UpdateSpeed.allCases.count)]
   transcriber.step = updateInterval(speed, passTime: rng.unit(), constrained: rng.below(2) == 0)
   print("step \(number(transcriber.step))")
  }
 }
 if let job = running { print(describe(transcriber.complete(job, words: decode(job, &rng, last: &last)))) }
 print(describe(transcriber.finish()))
 print("skipped \(number(transcriber.skipped)) end \(number(transcriber.audioEnd))")
 for _ in 0..<40 {
  let words = (0..<rng.below(10)).map { _ in rng.pick(vocabulary) }
  print("styles \(words.joined(separator: " ")) = \(wordStyles(words).map { "\($0)" }.joined(separator: " "))")
 }
 for note in notes { print("cue \(note) = \(cueLabel(for: note) ?? "nil")") }
 var placer = WordPlacer(space: 6, maxWidth: 200 + rng.unit() * 400)
 for _ in 0..<30 { let placed = placer.place(width: 10 + rng.unit() * 120); print("place \(placed.line) \(number(placed.x))") }
 for _ in 0..<5 { print("reveal \(number(revealInterval(backlog: rng.below(60))))") }
}
