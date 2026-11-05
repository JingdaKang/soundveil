import Foundation
import AVFoundation
import CaptionCore
import WhisperEngine

// Replays a recording through the rolling pipeline on a simulated live clock: audio "arrives"
// while each pass really runs. Measures caption delay without recording permission. It does
// not exercise ScreenCaptureKit, the overlay, or real playback.
let usage = "Usage: StreamCheck model.bin audio.wav [reference.txt] [partial=1] [final=5] [step=0.2] [phrase=2.5] [maxphrase=9] [first=0.8] [ctx=640] [tail=1] [show=1]\n"
var paths: [String] = [], options: [String: Double] = [:]
for argument in CommandLine.arguments.dropFirst() {
 let parts = argument.split(separator: "=")
 if parts.count == 2, let value = Double(parts[1]) { options[String(parts[0])] = value } else { paths.append(argument) }
}
guard paths.count >= 2 else { fputs(usage, stderr); exit(2) }
func load(_ path: String) throws -> [Float] {
 let file = try AVAudioFile(forReading: URL(fileURLWithPath: path))
 let input = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: AVAudioFrameCount(file.length))!
 try file.read(into: input)
 let target = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16000, channels: 1, interleaved: false)!
 let converter = AVAudioConverter(from: input.format, to: target)!
 let output = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: AVAudioFrameCount(Double(input.frameLength) * 16000 / input.format.sampleRate) + 256)!
 var supplied = false; var error: NSError?
 converter.convert(to: output, error: &error) { _, status in
  if supplied { status.pointee = .endOfStream; return nil }
  supplied = true; status.pointee = .haveData; return input
 }
 if let error { throw error }
 return Array(UnsafeBufferPointer(start: output.floatChannelData![0], count: Int(output.frameLength)))
}
func tokens(_ text: String) -> [String] { text.lowercased().split(whereSeparator: \.isWhitespace).map { $0.filter { $0.isLetter || $0.isNumber } }.filter { !$0.isEmpty } }
func errorRate(_ reference: [String], _ hypothesis: [String]) -> Double {
 var row = Array(0...hypothesis.count)
 for (i, r) in reference.enumerated() {
  var previous = row[0]; row[0] = i + 1
  for (j, h) in hypothesis.enumerated() {
   let cost = min(previous + (r == h ? 0 : 1), row[j] + 1, row[j+1] + 1)
   previous = row[j+1]; row[j+1] = cost
  }
 }
 return Double(row[hypothesis.count]) / Double(max(1, reference.count))
}
func percentile(_ values: [Double], _ p: Double) -> Double { let s = values.sorted(); return s.isEmpty ? 0 : s[min(s.count-1, Int(Double(s.count) * p))] }
func mean(_ values: [Double]) -> Double { values.isEmpty ? 0 : values.reduce(0, +) / Double(values.count) }
let samples = try load(paths[1]) + Array(repeating: Float(0), count: 24000)
guard let recognizer = Recognizer(path: paths[0], useGPU: options["cpu"] == nil) else { fatalError("Model load failed") }
if options["cues"] == 0 { recognizer.allowCues = false }
var gate = SpeechGate()
var transcriber = RollingTranscriber()
if let v = options["step"] { transcriber.step = v }
if let v = options["phrase"] { transcriber.minPhrase = v }
if let v = options["maxphrase"] { transcriber.maxPhrase = v }
if let v = options["partial"] { transcriber.partialBeam = Int32(v) }
if let v = options["final"] { transcriber.finalBeam = Int32(v) }
if let v = options["first"] { transcriber.firstDecode = v }
if let v = options["tail"] { transcriber.tailWords = Int(v) }
if let v = options["ctx"] { transcriber.audioContext = Int(v) }
let show_ = options["show"] != nil
var fed = 0, now = 0.0, passes: [Double] = [], lengths: [Double] = []
var committed = 0, changes = 0, showing: [String] = [], firstSeen: [(key: String, position: Int, time: Double)] = []
var firstShown: [Double] = [], settled: [Double] = [], transcript: [String] = [], finals: [Double] = []
func feed(until time: Double) {
 let target = min(samples.count, Int(time * 16000))
 while fed < target {
  let slice = Array(samples[fed..<min(target, fed + 1600)]); fed += slice.count
  for event in gate.append(slice) { transcriber.ingest(event) }
 }
}
func key(_ word: String) -> String { word.lowercased().filter { $0.isLetter || $0.isNumber } }
// Words the viewer saw replaced: anything on screen after the first word that changed.
func show(_ words: [String]) {
 let same = zip(showing, words).prefix { key($0) == key($1) }.count
 changes += showing.count - same
 showing = words
}
func record(_ update: CaptionUpdate) {
 for (i, word) in update.committed.enumerated() {
  settled.append(max(0, now - word.end)); transcript.append(word.text)
  // A word counts as shown once it was on screen near its final position in the phrase.
  let seen = firstSeen.first { $0.key == key(word.text) && abs($0.position - i) <= 2 }?.time ?? now
  firstShown.append(max(0, seen - word.end))
 }
 committed += update.committed.count
 if !update.committed.isEmpty { show(update.committed.map(\.text)); showing = []; firstSeen = [] }
 if let tentative = update.tentative {
  if update.committed.isEmpty { show(tentative.map(\.text)) }
  for (i, word) in tentative.enumerated() { firstSeen.append((key(word.text), i, now)) }
 }
 if show_, update.tentative != nil || !update.committed.isEmpty {
  print(String(format: "%6.2f  %@ | %@", now, update.committed.map(\.text).joined(separator: " "), (update.tentative ?? []).map(\.text).joined(separator: " ")))
 }
}
while true {
 feed(until: now)
 guard let job = transcriber.nextJob() else {
  if fed >= samples.count { break }
  now += 0.01; continue
 }
 let began = Date()
 let words = recognizer.decode(job) ?? []
 let took = Date().timeIntervalSince(began) * (options["slow"] ?? 1) // slow=3 simulates a Mac three times slower.
 if options["auto"] != nil, !job.isFinal { transcriber.step = updateInterval(.auto, passTime: took, constrained: options["auto"] == 2) }
 passes.append(took); lengths.append(Double(job.samples.count)/16000); if job.isFinal { finals.append(took) }
 now += took; feed(until: now)
 record(transcriber.complete(job, words: words))
}
record(transcriber.finish())
print(transcript.joined(separator: " "))
print(String(format: "audio %.1fs · passes %d · pass mean %.0f ms, p95 %.0f ms · buffer mean %.1fs, max %.1fs · skipped %.1fs", Double(samples.count)/16000, passes.count, mean(passes)*1000, percentile(passes, 0.95)*1000, mean(lengths), lengths.max() ?? 0, transcriber.skipped))
print(String(format: "final passes %d · mean %.0f ms, p95 %.0f ms", finals.count, mean(finals)*1000, percentile(finals, 0.95)*1000))
print(String(format: "first shown after word ends: mean %.2fs, p95 %.2fs", mean(firstShown), percentile(firstShown, 0.95)))
print(String(format: "final text after word ends:  mean %.2fs, p95 %.2fs", mean(settled), percentile(settled, 0.95)))
print(String(format: "on-screen words replaced: %d across %d words (%.1f%%)", changes, committed, Double(changes)/Double(max(1, committed))*100))
if paths.count > 2 { print(String(format: "word error rate: %.1f%%", errorRate(tokens(try String(contentsOfFile: paths[2], encoding: .utf8)), tokens(transcript.joined(separator: " "))) * 100)) }
