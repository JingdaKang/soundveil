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
 var onUpdate: (@Sendable (CaptionUpdate) -> Void)?
 var onStatus: (@Sendable (String) -> Void)?
 func load(path: String) async throws {
  try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
   queue.async {
    self.recognizer = Recognizer(path: path)
    if self.recognizer == nil { continuation.resume(throwing: AppError.message("Cannot load model. Check the download and available memory.")) }
    else { continuation.resume() }
   }
  }
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
   lock.lock()
   let publish = enabled
   let update = transcriber.complete(job, words: words ?? [])
   let skipped = transcriber.skipped
   lock.unlock()
   guard publish else { continue }
   if words == nil { onStatus?("Recognition failed. Stop and start again, or choose another model."); continue }
   onUpdate?(update)
   let now = Date()
   if now.timeIntervalSince(lastStatus) >= 1 {
    lastStatus = now
    let coverage = skipped >= 1 ? String(format: " · %.0f s skipped", skipped) : ""
    onStatus?(String(format: "Listening · about %.1f s behind", now.timeIntervalSince(began)) + coverage)
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
  case .permission: return "Soundveil can't record audio yet. Turn it on in Privacy Settings, then reopen Soundveil."
  }
 }
}
