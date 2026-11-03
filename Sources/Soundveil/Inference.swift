import Foundation
import CaptionCore
import CWhisperBridge

final class Inference: @unchecked Sendable {
 private let queue = DispatchQueue(label: "Soundveil.inference", qos: .userInitiated)
 private let lock = NSLock()
 private var pending = LatestQueue<AudioChunk>()
 private var running = false
 private var enabled = true
 private var context: FCContext?
 private var prompt = ""
 private var previousEnd: Double = -1
 private var dedup = Deduplicator()
 var onCaption: (@Sendable (Caption) -> Void)?
 var onStatus: (@Sendable (String) -> Void)?
 func load(path: String) async throws {
  try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
   queue.async {
    self.context = fc_create(path)
    if self.context == nil { continuation.resume(throwing: AppError.message("Cannot load model. Check the download and available memory.")) }
    else { continuation.resume() }
   }
  }
 }
 func submit(_ chunk: AudioChunk) {
  lock.lock()
  guard enabled else { lock.unlock(); return }
  pending.offer(chunk)
  let launch = !running; running = true
  lock.unlock()
  if launch { queue.async { self.drain() } }
 }
 private func drain() {
  while true {
   lock.lock()
   let chunk = enabled ? pending.take() : nil
   let dropped = pending.dropped
   if chunk == nil { running = false }
   lock.unlock()
   guard let chunk else { return }
   guard let context else { continue }
   if chunk.start > previousEnd + 2 { prompt = ""; dedup = Deduplicator() }
   let began = Date()
   let code = chunk.samples.withUnsafeBufferPointer { samples in
    prompt.withCString { fc_transcribe(context, samples.baseAddress, Int32(samples.count), $0) }
   }
   lock.lock(); let publish = enabled; lock.unlock()
   guard publish else { continue }
   if code != 0 { onStatus?("Inference failed (\(code)). Stop and restart, or select a smaller model."); continue }
   for i in 0..<fc_count(context) {
    let start = chunk.start + Double(fc_start(context,i))/100
    let end = min(chunk.end, chunk.start + Double(fc_end(context,i))/100)
    guard end > start else { continue }
    let text = String(cString: fc_text(context,i)).trimmingCharacters(in: .whitespacesAndNewlines)
    if let caption = dedup.accept(Caption(start: start, end: end, text: text)) {
     prompt = String((prompt + " " + caption.text).suffix(600)); onCaption?(caption)
    }
   }
   previousEnd = chunk.end
   onStatus?(String(format: "Listening · decode %.1fs · skipped windows %d", Date().timeIntervalSince(began), dropped))
  }
 }
 func stop() async {
  lock.withLock { enabled = false; pending = LatestQueue() }
  await withCheckedContinuation { continuation in
   queue.async { if let c = self.context { fc_destroy(c); self.context = nil }; continuation.resume() }
  }
 }
 deinit { if let context { fc_destroy(context) } }
}
enum AppError: LocalizedError {
 case message(String)
 var errorDescription: String? { if case .message(let s) = self { return s }; return nil }
}
