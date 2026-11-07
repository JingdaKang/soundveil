import AppKit

// The app's own language, separate from the captions, which stay English. The list comes from the
// .lproj folders the build makes, so a language with translations in the catalog appears here by
// itself. macOS reads AppleLanguages once at launch, so a change needs a restart.
enum AppLanguage {
 static let key = "AppleLanguages"
 static var available: [String] {
  Bundle.main.localizations.filter { $0 != "Base" }.sorted { name($0).localizedStandardCompare(name($1)) == .orderedAscending }
 }
 // Each language is named in its own words, so people can find theirs from any starting point.
 static func name(_ code: String) -> String {
  let locale = Locale(identifier: code)
  return locale.localizedString(forIdentifier: code)?.capitalized(with: locale) ?? code
 }
 // Nil means following the Mac. This reads the app's own settings, because the standard lookup
 // also returns the Mac's list. System Settings can save a region too, as in zh-Hans-CN.
 static var chosen: String? {
  guard let id = Bundle.main.bundleIdentifier, let saved = UserDefaults.standard.persistentDomain(forName: id)?[key] as? [String], let first = saved.first else { return nil }
  return available.first { first == $0 || first.hasPrefix($0 + "-") }
 }
 static func choose(_ code: String?) {
  if let code { UserDefaults.standard.set([code], forKey: key) } else { UserDefaults.standard.removeObject(forKey: key) }
 }
 // The wait lets this copy release its global shortcuts before the next one claims them.
 static func restart() {
  let task = Process()
  task.executableURL = URL(fileURLWithPath: "/bin/sh")
  task.arguments = ["-c", "sleep 0.6; open \"$0\"", Bundle.main.bundlePath]
  try? task.run()
  NSApp.terminate(nil)
 }
}
