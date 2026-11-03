import SwiftUI
import AppKit
import CaptionCore

@MainActor final class AppState: ObservableObject {
 @Published var model: Model = .turbo
 @Published var system = false
 @Published var vadDB: Double = -50
 @Published var level: Float = 0
 @Published var fontSize: Double = 28
 @Published var status = "Download a model, then start Chrome audio."
 @Published var caption = "Your live English captions appear here"
 @Published var history: [Caption] = []
 @Published var busy = false
 @Published var listening = false
 @Published var downloading = false
 @Published var progress: Double = 0
 @Published var installed = false
 private var capture: Capture?
 private var inference: Inference?
 private var download: ModelDownload?
 private var overlay: NSPanel?
 private var session = UUID()
 private var expire: Task<Void, Never>?
 func refresh() { installed = model.exists }
 func downloadModel() {
  downloading = true; progress = 0; status = "Downloading \(model.rawValue)…"
  download = ModelDownload(model: model, progress: { value in Task { @MainActor in self.progress = value } }, completion: { error in
   Task { @MainActor in self.downloading = false; self.status = error ?? "Model ready. Start Chrome audio."; self.refresh(); self.download = nil }
  }); download?.start()
 }
 func cancelDownload() { download?.cancel() }
 func start() async {
  guard !busy, !listening else { return }
  busy = true; status = "Loading model into memory…"; history = []; caption = "Waiting for speech…"
  session = UUID(); let id = session
  let engine = Inference(); inference = engine
  engine.onCaption = { item in Task { @MainActor in
   guard self.session == id, self.listening else { return }
   self.history.append(item); if self.history.count > 2000 { self.history.removeFirst(self.history.count - 2000) }
   self.caption = item.text
   self.expire?.cancel(); self.expire = Task { @MainActor in
    try? await Task.sleep(for: .seconds(10)); if !Task.isCancelled { self.caption = "" }
   }
  }}
  engine.onStatus = { text in Task { @MainActor in if self.session == id { self.status = text } } }
  do {
   try await engine.load(path: model.file.path)
   status = "Requesting screen and system audio permission…"
   let capture = Capture(); self.capture = capture
   capture.onChunk = { [weak engine] in engine?.submit($0) }
   capture.onLevel = { level in Task { @MainActor in if self.session == id { self.level = level } } }
   capture.onError = { message in Task { @MainActor in
    guard self.session == id else { return }; await self.stop(); self.status = message
   }}
   try await capture.start(system: system, threshold: Float(pow(10, vadDB/20)))
   listening = true; busy = false; status = "Listening · play a video in Chrome"; showOverlay()
  } catch {
   await capture?.stop(); capture = nil; await engine.stop(); inference = nil
   status = "\(error.localizedDescription) If access was denied, enable FreeCaptions in System Settings → Privacy & Security → Screen & System Audio Recording, then relaunch."
   busy = false
  }
 }
 func stop() async {
  guard !busy else { return }
  busy = true; listening = false; level = 0; session = UUID(); expire?.cancel(); caption = "Stopped"
  status = "Stopping (finishing current model call)…"
  await capture?.stop(); capture = nil
  await inference?.stop(); inference = nil
  busy = false; status = "Stopped"
 }
 func showOverlay() {
  if overlay == nil {
   let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 850, height: 180), styleMask: [.borderless, .nonactivatingPanel, .resizable], backing: .buffered, defer: false)
   panel.isFloatingPanel = true; panel.level = .floating; panel.isOpaque = false; panel.backgroundColor = .clear
   panel.hasShadow = false; panel.isMovableByWindowBackground = true
   panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
   panel.contentView = NSHostingView(rootView: CaptionOverlay(state: self))
   if let screen = NSScreen.main { panel.setFrameOrigin(NSPoint(x: screen.visibleFrame.midX - 425, y: screen.visibleFrame.minY + 65)) }
   panel.minSize = NSSize(width: 400, height: 100); overlay = panel
  }
  overlay?.orderFrontRegardless()
 }
 func hideOverlay() { overlay?.orderOut(nil) }
 func export() {
  let panel = NSSavePanel(); panel.allowedContentTypes = [.plainText]; panel.nameFieldStringValue = "Captions.txt"
  if panel.runModal() == .OK, let url = panel.url {
   do { try history.map { String(format: "[%02d:%02d] %@", Int($0.start)/60, Int($0.start)%60, $0.text) }.joined(separator: "\n").write(to: url, atomically: true, encoding: .utf8) }
   catch { status = error.localizedDescription }
  }
 }
}
struct CaptionOverlay: View {
 @ObservedObject var state: AppState
 var body: some View {
  Text(state.caption).font(.system(size: state.fontSize, weight: .semibold)).foregroundStyle(.white)
   .multilineTextAlignment(.center).lineLimit(4).minimumScaleFactor(0.75)
   .padding(18).frame(maxWidth: .infinity, maxHeight: .infinity)
   .background(.black.opacity(state.caption.isEmpty ? 0.05 : 0.78), in: RoundedRectangle(cornerRadius: 16))
   .padding(6).accessibilityLabel("Live captions").accessibilityValue(state.caption)
   .help("Drag to move; resize from an edge. Hide using the main window.")
 }
}
struct ContentView: View {
 @ObservedObject var state: AppState
 var body: some View {
  VStack(alignment: .leading, spacing: 16) {
   HStack { Text("FreeCaptions").font(.largeTitle.bold()); Spacer(); Text("ON DEVICE · ENGLISH").font(.caption).foregroundStyle(.secondary) }
   Text("Live captions for Chrome videos and streams. Audio stays on this Mac.").foregroundStyle(.secondary)
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
   Text(state.status).font(.callout).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
   Text("First use: allow Screen & System Audio Recording when macOS asks. No microphone access is used. Mute other Chrome tabs to isolate your video.").font(.caption).foregroundStyle(.secondary)
   Divider()
   HStack { Text("Caption history").font(.headline); Spacer(); Button("Export…") { state.export() }.disabled(state.history.isEmpty) }
   ScrollViewReader { proxy in
    List(state.history) { item in
     HStack(alignment: .top) { Text(String(format: "%02d:%02d", Int(item.start)/60, Int(item.start)%60)).monospacedDigit().foregroundStyle(.secondary); Text(item.text).textSelection(.enabled) }.id(item.id)
    }.onChange(of: state.history.count) { if let id = state.history.last?.id { proxy.scrollTo(id, anchor: .bottom) } }
   }
  }.padding(24).frame(minWidth: 650, minHeight: 620).onAppear { state.refresh() }
 }
}
@main struct FreeCaptionsApp: App {
 @StateObject private var state = AppState()
 var body: some Scene { WindowGroup { ContentView(state: state) } }
}
