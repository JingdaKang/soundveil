import Foundation
import CaptionCore

extension Model {
 var file: URL { Self.directory.appendingPathComponent(fileName) }
 var exists: Bool { FileManager.default.fileExists(atPath: file.path) }
 // Preserve the original storage path so existing multi-GB downloads remain usable.
 static var directory: URL {
  FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("FreeCaptions/Models", isDirectory: true)
 }
}
// Downloads stream to disk, report progress, and only install a complete file.
final class ModelDownload: NSObject, URLSessionDownloadDelegate, @unchecked Sendable {
 private var session: URLSession?
 private var task: URLSessionDownloadTask?
 private let model: Model
 let progress: @Sendable (Double) -> Void
 let completion: @Sendable (String?) -> Void
 init(model: Model, progress: @escaping @Sendable (Double)->Void, completion: @escaping @Sendable (String?)->Void) {
  self.model=model; self.progress=progress; self.completion=completion
 }
 func start() {
  let config = URLSessionConfiguration.default
  config.timeoutIntervalForResource = 86400
  session = URLSession(configuration: config, delegate: self, delegateQueue: nil)
  task = session!.downloadTask(with: URL(string: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/\(model.fileName)")!)
  task!.resume()
 }
 func cancel() { task?.cancel() }
 func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
  if totalBytesExpectedToWrite > 0 { progress(Double(totalBytesWritten)/Double(totalBytesExpectedToWrite)) }
 }
 func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
  do {
   guard (downloadTask.response as? HTTPURLResponse)?.statusCode == 200 else { throw AppError.message(String(localized: "Model server returned an error. Please retry.")) }
   let file = try FileHandle(forReadingFrom: location); defer { try? file.close() }
   guard try file.read(upToCount: 4) == Data([0x6c,0x6d,0x67,0x67]) else { throw AppError.message(String(localized: "Downloaded file is not a GGML Whisper model.")) }
   try FileManager.default.createDirectory(at: Model.directory, withIntermediateDirectories: true)
   try FileManager.default.moveItem(at: location, to: model.file)
   completion(nil)
  } catch { completion(error.localizedDescription) }
 }
 func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
  if let error { completion(error.localizedDescription) }
  session.finishTasksAndInvalidate(); self.session = nil
 }
}
