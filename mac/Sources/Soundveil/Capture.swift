import ScreenCaptureKit
import AVFoundation
import CaptionCore

final class Capture: NSObject, SCStreamOutput, SCStreamDelegate, @unchecked Sendable {
 private let queue = DispatchQueue(label: "Soundveil.capture", qos: .userInitiated)
 private var stream: SCStream?
 private var gate = SpeechGate()
 private var converter: AVAudioConverter?
 private var inputFormat: AVAudioFormat?
 private var accepting = true
 private var levelTick = 0
 private var origin: Double?
 private var expectedPTS: Double?
 var onEvents: (@Sendable ([SpeechEvent]) -> Void)?
 var onLevel: (@Sendable (Float) -> Void)?
 var onError: (@Sendable (String) -> Void)?
 func start(system: Bool, threshold: Float) async throws {
  gate.threshold = threshold
  // Include off-screen windows: Chrome is often on another Space or full screen.
  let content = try await permitted { try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false) }
  guard let display = content.displays.first else { throw AppError.message(String(localized: "No display is available for audio capture.")) }
  let filter: SCContentFilter
  if system {
   filter = SCContentFilter(display: display, excludingApplications: content.applications.filter { $0.processID == ProcessInfo.processInfo.processIdentifier }, exceptingWindows: [])
  } else {
   let apps = content.applications.filter { $0.bundleIdentifier == "com.google.Chrome" || $0.bundleIdentifier.hasPrefix("com.google.Chrome.") }
   guard !apps.isEmpty else { throw AppError.message(String(localized: "Open Chrome and play a video, then start captions.")) }
   filter = SCContentFilter(display: display, including: apps, exceptingWindows: [])
  }
  let config = SCStreamConfiguration()
  config.capturesAudio = true; config.excludesCurrentProcessAudio = true
  config.sampleRate = 48000; config.channelCount = 2
  if #available(macOS 15, *) { config.captureMicrophone = false }
  config.width = 2; config.height = 2
  config.minimumFrameInterval = CMTime(value: 1, timescale: 1)
  config.queueDepth = 3
  let stream = SCStream(filter: filter, configuration: config, delegate: self)
  try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: queue)
  self.stream = stream
  try await permitted { try await stream.startCapture() }
 }
 private func permitted<T>(_ work: () async throws -> T) async throws -> T {
  do { return try await work() }
  catch where (error as NSError).code == SCStreamError.Code.userDeclined.rawValue { throw AppError.permission }
 }
 func stop() async {
  try? await stream?.stopCapture(); stream = nil
  await withCheckedContinuation { continuation in
   queue.async { self.accepting = false; self.gate = SpeechGate(); self.converter = nil; continuation.resume() }
  }
 }
 func stream(_ stream: SCStream, didStopWithError error: Error) { onError?(error.localizedDescription) }
 func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
  guard accepting, type == .audio, sampleBuffer.isValid,
   let description = sampleBuffer.formatDescription else { return }
  let format = AVAudioFormat(cmAudioFormatDescription: description)
  let pts = sampleBuffer.presentationTimeStamp.seconds
  guard pts.isFinite else { return }
  if origin == nil { origin = pts }
  if let expectedPTS, abs(pts - expectedPTS) > 0.25 {
   let threshold = gate.threshold
   gate = SpeechGate(start: max(0, pts - (origin ?? pts))); gate.threshold = threshold; converter?.reset()
   onEvents?([.gap])
  }
  expectedPTS = pts + Double(sampleBuffer.numSamples)/format.sampleRate
  guard let input = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(sampleBuffer.numSamples)) else { return }
  input.frameLength = input.frameCapacity
  let status = CMSampleBufferCopyPCMDataIntoAudioBufferList(sampleBuffer, at: 0, frameCount: Int32(input.frameLength), into: input.mutableAudioBufferList)
  guard status == noErr else { onError?(String(localized: "Cannot read captured PCM audio (\(status)).")); return }
  let target = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16000, channels: 1, interleaved: false)!
  if inputFormat != format { converter = AVAudioConverter(from: format, to: target); inputFormat = format }
  guard let converter, let output = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: AVAudioFrameCount(Double(input.frameLength) * 16000 / format.sampleRate) + 256) else { return }
  var supplied = false
  var error: NSError?
  converter.convert(to: output, error: &error) { _, state in
   if supplied { state.pointee = .noDataNow; return nil }
   supplied = true; state.pointee = .haveData; return input
  }
  if let error { onError?(error.localizedDescription); return }
  guard let data = output.floatChannelData else { return }
  let samples = Array(UnsafeBufferPointer(start: data[0], count: Int(output.frameLength)))
  levelTick += samples.count
  if levelTick >= 4000, !samples.isEmpty {
   levelTick = 0
   onLevel?(sqrt(samples.reduce(Float(0)) { $0 + $1*$1 } / Float(samples.count)))
  }
  let events = gate.append(samples)
  if !events.isEmpty { onEvents?(events) }
 }
}
