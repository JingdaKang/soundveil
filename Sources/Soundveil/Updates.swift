import Foundation
import UpdateCore

// Reads the newest published GitHub release. The request carries no captions, audio, or settings.
enum Updates {
 static let endpoint = URL(string: "https://api.github.com/repos/JingdaKang/soundveil/releases/latest")!
 static var current: String { Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "" }
 static func check() async throws -> Release? {
  var request = URLRequest(url: endpoint, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 15)
  request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
  let (data, response) = try await URLSession.shared.data(for: request)
  let code = (response as? HTTPURLResponse)?.statusCode ?? 0
  if code == 404 { return nil } // Nothing has been published yet.
  guard code == 200 else { throw AppError.message("GitHub returned HTTP \(code). Try again later.") }
  guard let release = Release(github: data) else { throw AppError.message("The latest GitHub release has no readable version.") }
  return release.isNewer(than: current) ? release : nil
 }
}
