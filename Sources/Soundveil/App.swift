import SwiftUI
import AppKit
import CaptionCore
import UpdateCore

@MainActor final class AppState: ObservableObject {
 @Published var model: Model = .turbo
 @Published var system = false
 @Published var vadDB: Double = -50
 @Published var level: Float = 0
 @Published var fontSize: Double { didSet { UserDefaults.standard.set(fontSize, forKey: "captionSize"); feed.fontSize = fontSize } }
 @Published var status = "Download a model to get started."
 @Published var needsPermission = false
 @Published var history: [Caption] = []
 @Published var busy = false
 @Published var listening = false
 @Published var downloading = false
 @Published var progress: Double = 0
 @Published var installed = false
 @Published var update: Release?
 private var capture: Capture?
 private var inference: Inference?
 private var download: ModelDownload?
 private var overlay: NSPanel?
 private var session = UUID()
 private var checked = false
 let feed = CaptionFeed()
 init() {
  let saved = UserDefaults.standard.double(forKey: "captionSize")
  fontSize = saved > 0 ? saved : 28
  feed.fontSize = fontSize
 }
 func refresh() {
  installed = model.exists
  if !listening, !busy, !downloading { status = installed ? "Ready. Play a video in Chrome, then start captions." : "Download a model to get started." }
 }
 func downloadModel() {
  downloading = true; progress = 0; status = "Downloading \(model.rawValue)…"
  download = ModelDownload(model: model, progress: { value in Task { @MainActor in self.progress = value } }, completion: { error in
   Task { @MainActor in self.downloading = false; self.refresh(); if let error { self.status = error }; self.download = nil }
  }); download?.start()
 }
 func cancelDownload() { download?.cancel() }
 func start() async {
  guard !busy, !listening else { return }
  busy = true; needsPermission = false; status = "Loading the model. The first run can take up to a minute…"; history = []
  feed.reset(placeholder: "Waiting for speech…")
  session = UUID(); let id = session
  let engine = Inference(); inference = engine
  // The main queue keeps updates in order; a later pass replaces the tentative words of an earlier one.
  engine.onUpdate = { update in DispatchQueue.main.async { MainActor.assumeIsolated {
   guard self.session == id, self.listening else { return }
   self.receive(update)
  }}}
  engine.onStatus = { text in Task { @MainActor in if self.session == id { self.status = text } } }
  do {
   try await engine.load(path: model.file.path)
   status = "Requesting screen and system audio permission…"
   let capture = Capture(); self.capture = capture
   capture.onEvents = { [weak engine] in engine?.submit($0) }
   capture.onLevel = { level in Task { @MainActor in if self.session == id { self.level = level } } }
   capture.onError = { message in Task { @MainActor in
    guard self.session == id else { return }; await self.stop(); self.status = message
   }}
   try await capture.start(system: system, threshold: Float(pow(10, vadDB/20)))
   listening = true; busy = false; status = "Listening · play a video in Chrome"; showOverlay()
  } catch {
   await capture?.stop(); capture = nil; _ = await engine.stop(); inference = nil
   if case AppError.permission = error { needsPermission = true }
   status = error.localizedDescription; feed.reset(placeholder: ""); busy = false
  }
 }
 private func receive(_ update: CaptionUpdate) {
  feed.apply(update)
  history += update.sentences
  if history.count > 2000 { history.removeFirst(history.count - 2000) }
 }
 func stop() async {
  guard !busy else { return }
  busy = true; listening = false; level = 0; session = UUID()
  status = "Stopping…"
  await capture?.stop(); capture = nil
  if let update = await inference?.stop() { receive(update) }
  inference = nil; busy = false; status = "Stopped"
 }
 func showOverlay() {
  if overlay == nil {
   let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 850, height: 156), styleMask: [.borderless, .nonactivatingPanel, .resizable], backing: .buffered, defer: false)
   panel.isFloatingPanel = true; panel.level = .floating; panel.isOpaque = false; panel.backgroundColor = .clear
   panel.hasShadow = false; panel.isMovableByWindowBackground = true
   panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
   panel.contentView = NSHostingView(rootView: CaptionOverlay(feed: feed))
   panel.minSize = NSSize(width: 400, height: 90)
   if !panel.setFrameUsingName("CaptionOverlay"), let screen = NSScreen.main { panel.setFrameOrigin(NSPoint(x: screen.visibleFrame.midX - 425, y: screen.visibleFrame.minY + 65)) }
   panel.setFrameAutosaveName("CaptionOverlay"); overlay = panel
  }
  overlay?.orderFrontRegardless()
 }
 func hideOverlay() { overlay?.orderOut(nil) }
 // Automatic checks run once per launch and stay silent on failure; manual checks always report.
 func checkForUpdates(manual: Bool) async {
  guard manual || !checked else { return }
  checked = true
  let alert = NSAlert()
  do {
   update = try await Updates.check()
   guard manual else { return }
   if let update {
    alert.messageText = "Soundveil \(update.version) is available"
    alert.informativeText = "You have \(Updates.current). Download the disk image, then replace Soundveil in Applications."
    alert.addButton(withTitle: "Download"); alert.addButton(withTitle: "Later")
    if alert.runModal() == .alertFirstButtonReturn { openUpdate() }
   } else { alert.messageText = "Soundveil \(Updates.current) is up to date"; _ = alert.runModal() }
  } catch {
   guard manual else { return }
   alert.messageText = "Couldn't check for updates"; alert.informativeText = error.localizedDescription; _ = alert.runModal()
  }
 }
 func openUpdate() { if let update { NSWorkspace.shared.open(update.download ?? update.page) } }
 func export() {
  let panel = NSSavePanel(); panel.allowedContentTypes = [.plainText]; panel.nameFieldStringValue = "Captions.txt"
  if panel.runModal() == .OK, let url = panel.url {
   do { try history.map { String(format: "[%02d:%02d] %@", Int($0.start)/60, Int($0.start)%60, $0.text) }.joined(separator: "\n").write(to: url, atomically: true, encoding: .utf8) }
   catch { status = error.localizedDescription }
  }
 }
}
struct UpdateBanner: View {
 let update: Release
 @ObservedObject var state: AppState
 var body: some View {
  HStack(spacing: 12) {
   Image(systemName: "arrow.down.circle.fill").font(.title2).foregroundStyle(Color.accentColor)
   VStack(alignment: .leading, spacing: 2) {
    Text("Soundveil \(update.version) is available").font(.headline)
    Text("You have \(Updates.current). Download the disk image, then replace Soundveil in Applications.").font(.caption).foregroundStyle(.secondary)
   }
   Spacer()
   Button("Later") { state.update = nil }
   Button("Download") { state.openUpdate() }.buttonStyle(.borderedProminent)
  }.padding(12).background(Color.accentColor.opacity(0.1), in: RoundedRectangle(cornerRadius: 10))
  .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Color.accentColor.opacity(0.25)))
  .accessibilityElement(children: .contain).accessibilityLabel("Update available")
 }
}
struct ContentView: View {
 @ObservedObject var state: AppState
 var body: some View {
  VStack(alignment: .leading, spacing: 16) {
   HStack { Text("Soundveil").font(.largeTitle.bold()); Spacer(); Text("ON DEVICE · ENGLISH").font(.caption).foregroundStyle(.secondary) }
   Text("Live captions for Chrome videos and streams. Audio stays on this Mac.").foregroundStyle(.secondary)
   if let update = state.update { UpdateBanner(update: update, state: state) }
   Picker("Audio source", selection: $state.system) { Text("Chrome (all audible tabs)").tag(false); Text("System audio").tag(true) }.disabled(state.listening || state.busy)
   Picker("Recognition model", selection: $state.model) { ForEach(Model.allCases) { Text($0.title).tag($0) } }
    .disabled(state.listening || state.busy || state.downloading).onChange(of: state.model) { state.refresh() }
   HStack {
    if state.downloading { ProgressView(value: state.progress); Button("Cancel download") { state.cancelDownload() } }
    else { Button(state.installed ? "Model installed" : "Download model (free)") { state.downloadModel() }.disabled(state.installed || state.busy) }
    Button("Open model folder") { try? FileManager.default.createDirectory(at: Model.directory, withIntermediateDirectories: true); NSWorkspace.shared.open(Model.directory) }
   }
   HStack {
    Button(state.listening ? "Stop" : "Start captions") { Task { if state.listening { await state.stop() } else { await state.start() } } }
     .buttonStyle(.borderedProminent).disabled(state.busy || (!state.listening && !state.installed)).keyboardShortcut(.return)
    Button("Show overlay") { state.showOverlay() }; Button("Hide overlay") { state.hideOverlay() }
    Spacer(); if state.busy { ProgressView().controlSize(.small) }
   }
   HStack {
    Text("Quiet speech threshold")
    Slider(value: $state.vadDB, in: -65 ... -35, step: 1).disabled(state.listening || state.busy)
    Text("\(Int(state.vadDB)) dB").monospacedDigit()
    ProgressView(value: min(1, Double(state.level) * 10)).frame(width: 60).help("Captured audio level")
   }.help("Lower the threshold for quiet speech. Higher sensitivity also admits more noise and music.")
   HStack { Text("Caption size"); Slider(value: $state.fontSize, in: 18...48, step: 1); Text("\(Int(state.fontSize)) pt").monospacedDigit() }
   HStack {
    Text(state.status).font(.callout).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
    if state.needsPermission {
     Button("Open Privacy Settings") { NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")!) }
    }
   }
   Text("First use: allow Screen & System Audio Recording when macOS asks. No microphone access is used. Mute other Chrome tabs to isolate your video.").font(.caption).foregroundStyle(.secondary)
   Divider()
   HStack { Text("Caption history").font(.headline); Spacer(); Button("Export…") { state.export() }.disabled(state.history.isEmpty) }
   ScrollViewReader { proxy in
    List(state.history) { item in
     HStack(alignment: .top) { Text(String(format: "%02d:%02d", Int(item.start)/60, Int(item.start)%60)).monospacedDigit().foregroundStyle(.secondary); Text(item.text).textSelection(.enabled) }.id(item.id)
    }.onChange(of: state.history.count) { if let id = state.history.last?.id { proxy.scrollTo(id, anchor: .bottom) } }
   }
  }.padding(24).frame(minWidth: 650, minHeight: 620).onAppear { state.refresh() }
  .animation(.default, value: state.update)
 }
}
@main struct SoundveilApp: App {
 @StateObject private var state = AppState()
 @AppStorage("checkForUpdates") private var autoCheck = true
 var body: some Scene {
  WindowGroup { ContentView(state: state).task { if autoCheck { await state.checkForUpdates(manual: false) } } }
   .commands { CommandGroup(after: .appInfo) {
    Button("Check for Updates…") { Task { await state.checkForUpdates(manual: true) } }
    Toggle("Check for Updates Automatically", isOn: $autoCheck)
   }}
 }
}
