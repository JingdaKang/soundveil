import Foundation
import AVFoundation
import CWhisperBridge

// Exercises the same C bridge and Metal backend without screen-recording permission.
guard CommandLine.arguments.count == 3 else {
 fputs("Usage: swift run SmokeCheck /path/model.bin /path/audio.wav\n", stderr); exit(2)
}
do {
 let file = try AVAudioFile(forReading: URL(fileURLWithPath: CommandLine.arguments[2]))
 guard let input = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: AVAudioFrameCount(file.length)) else { fatalError("Input allocation failed") }
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
 guard let context = fc_create(CommandLine.arguments[1]) else { fatalError("Model load failed") }
 defer { fc_destroy(context) }
 let code = fc_transcribe(context, output.floatChannelData![0], Int32(output.frameLength), "")
 guard code == 0 else { fatalError("Inference failed: \(code)") }
 var text = ""
 for i in 0..<fc_count(context) { text += String(cString: fc_text(context,i)) }
 print(text)
 guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { fatalError("No transcript produced") }
} catch { fputs("\(error)\n", stderr); exit(1) }
