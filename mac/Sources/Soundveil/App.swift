import SwiftUI
import AppKit
import CaptionCore
import UpdateCore
import ShortcutCore

@MainActor final class AppState: ObservableObject {
 @Published var model: Model { didSet { UserDefaults.standard.set(model.rawValue, forKey: "model") } }
 @Published var system = false
 @Published var vadDB: Double = -50
 @Published var level: Float = 0
 @Published var fontSize: Double { didSet { UserDefaults.standard.set(fontSize, forKey: "captionSize"); feed.fontSize = fontSize } }
 @Published var status = "Download a model to get started."
 @Published var highlightWords: Bool { didSet { UserDefaults.standard.set(highlightWords, forKey: "highlightWords"); feed.highlight = highlightWords } }
 @Published var updateSpeed: UpdateSpeed { didSet { UserDefaults.standard.set(updateSpeed.rawValue, forKey: "updateSpeed"); inference?.setSpeed(updateSpeed) } }
 @Published var clickThrough: Bool { didSet { UserDefaults.standard.set(clickThrough, forKey: "clickThrough"); watchPointer() } }
 @Published private(set) var shortcuts: ShortcutMap
 @Published private(set) var refusedShortcuts: [ShortcutAction] = []
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
 private var pointerTimer: Timer?
 private var session = UUID()
 private var checked = false
 private var observers: [NSObjectProtocol] = []
 private var compactHeight = 156.0
 let feed = CaptionFeed()
 let hotkeys = Hotkeys()
 init() {
  model = Model.saved(UserDefaults.standard.string(forKey: "model"))
  let saved = UserDefaults.standard.double(forKey: "captionSize")
  fontSize = saved > 0 ? saved : 28
  highlightWords = UserDefaults.standard.object(forKey: "highlightWords") as? Bool ?? true
  updateSpeed = UpdateSpeed(rawValue: UserDefaults.standard.string(forKey: "updateSpeed") ?? "") ?? .auto
  clickThrough = UserDefaults.standard.bool(forKey: "clickThrough")
  shortcuts = UserDefaults.standard.data(forKey: "shortcuts").flatMap { ShortcutMap(data: $0) } ?? .defaults
  feed.fontSize = fontSize; feed.highlight = highlightWords
  hotkeys.onAction = { [weak self] in self?.perform($0) }
  applyShortcuts()
 }
 func applyShortcuts() { refusedShortcuts = hotkeys.install(shortcuts) }
 func setShortcut(_ shortcut: Shortcut?, for action: ShortcutAction) {
  shortcuts.set(shortcut, for: action)
  UserDefaults.standard.set(shortcuts.encoded(), forKey: "shortcuts"); applyShortcuts()
 }
 func restoreShortcuts() {
  shortcuts = .defaults
  UserDefaults.standard.set(shortcuts.encoded(), forKey: "shortcuts"); applyShortcuts()
 }
 func perform(_ action: ShortcutAction) {
  switch action {
  case .toggleCaptions: Task { if listening { await stop() } else if installed { await start() } }
  case .toggleOverlay: if overlay?.isVisible == true { hideOverlay() } else { showOverlay() }
  case .toggleExpanded: showOverlay(); toggleExpanded()
  case .largerText: fontSize = min(48, fontSize + 2)
  case .smallerText: fontSize = max(18, fontSize - 2)
  case .toggleClickThrough: showOverlay(); clickThrough.toggle()
  case .resetOverlay: showOverlay(); resetOverlay()
  }
 }
 func refresh() {
  installed = model.exists
  if !listening, !busy, !downloading { status = installed ? "Ready. Play a video in Chrome, then start captions." : "Download a model to get started." }
 }
 func downloadModel() {
  downloading = true; progress = 0; status = "Downloading \(model.name)…"
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
  engine.setSpeed(updateSpeed)
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
   let host = OverlayHost(rootView: CaptionOverlay(state: self, feed: feed))
   host.onHover = { [weak self] inside in if self?.clickThrough == false { self?.feed.hovering = inside } }
   let resizer = OverlayResizer()
   resizer.onFinish = { [weak self] in self?.saveOverlayFrame() }
   let content = NSView()
   for view in [host, resizer] { view.autoresizingMask = [.width, .height]; content.addSubview(view) }
   panel.contentView = content
   panel.minSize = NSSize(width: 400, height: 90)
   if !panel.setFrameUsingName("CaptionOverlay") { place(panel) }
   for name in [NSWindow.didMoveNotification, NSWindow.didEndLiveResizeNotification] {
    observers.append(NotificationCenter.default.addObserver(forName: name, object: panel, queue: .main) { [weak self] _ in
     MainActor.assumeIsolated { self?.saveOverlayFrame() }
    })
   }
   overlay = panel
   watchPointer()
  }
  overlay?.orderFrontRegardless()
 }
 // A panel that ignores the mouse can't see it either, and an event monitor misses movement
 // while this app is frontmost, so the pointer is polled, and only while clicks pass through.
 private func watchPointer() {
  pointerTimer?.invalidate(); pointerTimer = nil
  guard let overlay else { return }
  overlay.ignoresMouseEvents = clickThrough
  guard clickThrough else { return }
  let timer = Timer(timeInterval: 0.1, repeats: true) { [weak self] _ in MainActor.assumeIsolated { self?.followPointer() } }
  timer.tolerance = 0.03
  RunLoop.main.add(timer, forMode: .common)
  pointerTimer = timer
  followPointer()
 }
 // Clicks land only while the pointer is on the badge, and pass through everywhere else.
 private func followPointer() {
  guard let overlay else { return }
  let pointer = NSEvent.mouseLocation, frame = overlay.frame
  let inside = overlay.isVisible && frame.contains(pointer)
  if feed.hovering != inside { feed.hovering = inside }
  let margin = CaptionOverlay.panelMargin + CaptionOverlay.controlMargin, size = ClickThroughBadge.size
  let badge = NSRect(x: frame.maxX - margin - size.width, y: frame.maxY - margin - size.height, width: size.width, height: size.height)
  let takesClicks = inside && badge.contains(pointer)
  if overlay.ignoresMouseEvents == takesClicks { overlay.ignoresMouseEvents = !takesClicks }
 }
 func hideOverlay() { overlay?.orderOut(nil) }
 private func place(_ panel: NSPanel) {
  guard let screen = NSScreen.main else { return }
  panel.setFrame(NSRect(x: screen.visibleFrame.midX - 425, y: screen.visibleFrame.minY + 65, width: 850, height: 156), display: true)
 }
 // Expanding is a temporary reading mode, so only the compact size and position are remembered.
 func saveOverlayFrame() { if !feed.expanded { overlay?.saveFrame(usingName: "CaptionOverlay") } }
 // Grows upward so the newest line stays where the eye already is.
 func toggleExpanded() {
  guard let overlay, let screen = (overlay.screen ?? NSScreen.main)?.visibleFrame else { return }
  var frame = overlay.frame
  if feed.expanded {
   frame.size.height = compactHeight
  } else {
   compactHeight = frame.height
   frame.size.height = min(screen.height * 0.6, max(frame.height * 2, 12 * feed.lineHeight + 40))
   if frame.maxY > screen.maxY { frame.origin.y = screen.maxY - frame.height }
  }
  feed.expanded.toggle()
  overlay.setFrame(frame, display: true, animate: true)
  saveOverlayFrame()
 }
 func resetOverlay() {
  guard let overlay else { return }
  feed.expanded = false; place(overlay); saveOverlayFrame()
 }
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
 private func historyLine(_ text: String) -> Text {
  guard state.highlightWords else { return Text(text) }
  let words = text.split(separator: " ").map(String.init)
  var line = AttributedString()
  for (word, style) in zip(words, wordStyles(words)) {
   var piece = AttributedString(word + " ")
   if style != .plain { piece.foregroundColor = .orange; piece.font = .body.bold() }
   line += piece
  }
  return Text(line)
 }
 var body: some View {
  VStack(alignment: .leading, spacing: 16) {
   HStack {
    Text("Soundveil").font(.largeTitle.bold()); Spacer(); Text("ON DEVICE · ENGLISH").font(.caption).foregroundStyle(.secondary)
    SettingsLink { Image(systemName: "gearshape") }.help("Settings").accessibilityLabel("Settings")
   }
   Text("Live captions for Chrome videos and streams. Audio stays on this Mac.").foregroundStyle(.secondary)
   if let update = state.update { UpdateBanner(update: update, state: state) }
   Picker("Audio source", selection: $state.system) { Text("Chrome (all audible tabs)").tag(false); Text("System audio").tag(true) }.disabled(state.listening || state.busy)
   Picker("Recognition model", selection: $state.model) { ForEach(Model.allCases) { Text($0.title).tag($0) } }
    .disabled(state.listening || state.busy || state.downloading).onChange(of: state.model) { state.refresh() }
   Text(state.model.detail).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
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
     HStack(alignment: .top) { Text(String(format: "%02d:%02d", Int(item.start)/60, Int(item.start)%60)).monospacedDigit().foregroundStyle(.secondary); historyLine(item.text).textSelection(.enabled) }.id(item.id)
    }.onChange(of: state.history.count) { if let id = state.history.last?.id { proxy.scrollTo(id, anchor: .bottom) } }
   }
  }.padding(24).frame(minWidth: 650, minHeight: 620).onAppear { state.refresh() }
  .animation(.default, value: state.update)
 }
}
// SwiftUI skips the main window when macOS has any saved window to restore, even just Settings, and
// a Dock click does nothing while Settings is open. Either would leave only Settings on screen, so
// the main window is opened here. Opening it when it's already open only raises it.
final class AppDelegate: NSObject, NSApplicationDelegate {
 var showMainWindow: () -> Void = {}
 func applicationDidFinishLaunching(_ notification: Notification) {
  DispatchQueue.main.async { self.showMainWindow() }
 }
 func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool) -> Bool {
  showMainWindow()
  return true
 }
 // Captions keep running in the overlay after the main window closes.
 func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }
}
struct AppCommands: Commands {
 @ObservedObject var state: AppState
 @Binding var autoCheck: Bool
 let delegate: AppDelegate
 @Environment(\.openWindow) private var openWindow
 var body: some Commands {
  // Commands are the one place that has openWindow before any window exists.
  let _ = delegate.showMainWindow = { [openWindow] in openWindow(id: "main") }
  CommandGroup(after: .appInfo) {
   Button("Check for Updates…") { Task { await state.checkForUpdates(manual: true) } }
   Toggle("Check for Updates Automatically", isOn: $autoCheck)
  }
  CommandGroup(after: .windowSize) { Button("Reset Caption Overlay") { state.resetOverlay() } }
 }
}
@main struct SoundveilApp: App {
 @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
 @StateObject private var state = AppState()
 @AppStorage("checkForUpdates") private var autoCheck = true
 var body: some Scene {
  // SwiftUI treats the first scene as the app's main one, so this stays ahead of Settings.
  Window("Soundveil", id: "main") { ContentView(state: state).task { if autoCheck { await state.checkForUpdates(manual: false) } } }
   .defaultSize(width: 900, height: 720)
   .commands { AppCommands(state: state, autoCheck: $autoCheck, delegate: delegate) }
  Settings { SettingsView(state: state) }
 }
}
