import Foundation
import CaptionCore
import WhisperEngine

// Capture adds audio and this queue decodes one rolling pass at a time, always over the
// newest audio, so a slow pass delays captions by no more than its own length. If inference
// stalls, the transcriber's bounded buffer drops the oldest audio and that coverage is lost.
final class Inference: @unchecked Sendable {
 private let queue = DispatchQueue(label: "Soundveil.inference", qos: .userInitiated)
 private let lock = NSLock()
 private var transcriber = RollingTranscriber()
 private var running = false
 private var enabled = true
 private var recognizer: Recognizer?
 private var lastStatus = Date.distantPast
 private var speed = UpdateSpeed.auto
 var onUpdate: (@Sendable (CaptionUpdate) -> Void)?
 var onStatus: (@Sendable (String) -> Void)?
 func load(path: String) async throws {
  try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
   queue.async {
    self.recognizer = Recognizer(path: path)
    if self.recognizer == nil { continuation.resume(throwing: AppError.message(String(localized: "Cannot load model. Check the download and available memory."))) }
    else { continuation.resume() }
   }
  }
 }
 func setSpeed(_ speed: UpdateSpeed) {
  lock.withLock { self.speed = speed; transcriber.step = updateInterval(speed, passTime: 0, constrained: false) }
 }
 func submit(_ events: [SpeechEvent]) {
  lock.lock()
  guard enabled else { lock.unlock(); return }
  for event in events { transcriber.ingest(event) }
  let launch = !running; running = true
  lock.unlock()
  if launch { queue.async { self.drain() } }
 }
 private func drain() {
  while true {
   lock.lock()
   let job = enabled ? transcriber.nextJob() : nil
   if job == nil { running = false }
   lock.unlock()
   guard let job else { return }
   guard let recognizer else { continue }
   let began = Date()
   let words = recognizer.decode(job)
   let took = Date().timeIntervalSince(began)
   let info = ProcessInfo.processInfo
   let constrained = info.isLowPowerModeEnabled || info.thermalState == .serious || info.thermalState == .critical
   lock.lock()
   let publish = enabled
   let update = transcriber.complete(job, words: words ?? [])
   let skipped = transcriber.skipped
   if !job.isFinal { transcriber.step = updateInterval(speed, passTime: took, constrained: constrained) }
   let interval = transcriber.step
   lock.unlock()
   guard publish else { continue }
   if words == nil { onStatus?(String(localized: "Recognition failed. Stop and start again, or choose another model.")); continue }
   onUpdate?(update)
   let now = Date()
   if now.timeIntervalSince(lastStatus) >= 1 {
    lastStatus = now
    let delay = (took + interval / 2).formatted(.number.precision(.fractionLength(1)))
    let lost = skipped.formatted(.number.precision(.fractionLength(0)))
    onStatus?(skipped >= 1 ? String(localized: "Listening · about \(delay) s behind · \(lost) s skipped") : String(localized: "Listening · about \(delay) s behind"))
   }
  }
 }
 func stop() async -> CaptionUpdate {
  let update = lock.withLock { enabled = false; return transcriber.finish() }
  await withCheckedContinuation { continuation in
   queue.async { self.recognizer = nil; continuation.resume() }
  }
  return update
 }
}
enum AppError: LocalizedError {
 case message(String)
 case permission
 var errorDescription: String? {
  switch self {
  case .message(let text): return text
  case .permission: return String(localized: "Soundveil can't record audio yet. Turn it on in Privacy Settings, then reopen Soundveil.")
  }
 }
}
