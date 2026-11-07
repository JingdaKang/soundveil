import SwiftUI
import AppKit
import CaptionCore
import OverlayCore

// Words on screen. Updates arrive several times a second, so this stays apart from AppState
// and the main window is not redrawn for each word. Confirmed words are placed once and
// never move; tentative words sit after them and may change.
@MainActor final class CaptionFeed: ObservableObject {
 struct Item: Identifiable, Equatable {
  let id: String
  let text: String
  let confirmed: Bool
  let style: WordStyle
  let line: Int
  let x: Double
 }
 @Published private(set) var items: [Item] = []
 @Published var placeholder = "Your live captions appear here"
 @Published var fontSize: Double = 28 { didSet { if fontSize != oldValue { widths = [:]; relayout() } } }
 @Published var highlight = true { didSet { if highlight != oldValue { widths = [:]; relayout() } } }
 @Published var style = CaptionStyle.standard
 @Published var hovering = false
 @Published var expanded = false { didSet { if expanded != oldValue { publish() } } }
 var lineHeight: Double { (fontSize * 1.3).rounded() }
 private var width = 0.0
 private var widths: [String: Double] = [:]
 private var settled: [Item] = []
 private var pending: [Item] = []
 private var cursor = WordPlacer(space: 7, maxWidth: 800)
 private var total = 0
 private var revealed = 0
 private var idle = false
 private var pacing: Task<Void, Never>?
 private var expiry: Task<Void, Never>?
 func configure(width: Double) { if width > 0, width != self.width { self.width = width; relayout() } }
 func apply(_ update: CaptionUpdate) {
  let shown = revealed
  let texts = update.committed.map(\.text), styles = wordStyles(texts)
  if !texts.isEmpty || !(update.tentative ?? []).isEmpty { idle = false }
  for (text, style) in zip(texts, styles) { settled.append(place(text, style, index: total, confirmed: true, with: &cursor)); total += 1 }
  if settled.count > 600 { settled.removeFirst(settled.count - 600) }
  if let tentative = update.tentative { placeTentative(tentative.map(\.text), stable: update.stable) }
  // A final pass rewrites words the viewer already saw, so it must not restart their reveal.
  revealed = min(max(shown, total - (update.committed.isEmpty ? 0 : pending.count)), total + pending.count)
  if !settled.isEmpty || !pending.isEmpty { placeholder = "" }
  publish(); pace()
  if !update.committed.isEmpty || !(update.tentative ?? []).isEmpty { scheduleExpiry() }
 }
 func reset(placeholder: String) {
  pacing?.cancel(); pacing = nil; expiry?.cancel(); expiry = nil
  settled = []; pending = []; total = 0; revealed = 0; idle = false; cursor = newCursor()
  self.placeholder = placeholder; publish()
 }
 private func newCursor() -> WordPlacer { WordPlacer(space: (fontSize * 0.26).rounded(), maxWidth: width > 0 ? width : 800) }
 // Highlighted words are 10% larger, and a cue is a small pill, so each is measured as drawn.
 private func measure(_ text: String, _ style: WordStyle) -> Double {
  let key = "\(style)|\(text)"
  if let known = widths[key] { return known }
  let size = style == .cue ? fontSize * 0.68 : style == .plain ? fontSize : fontSize * 1.1
  let font = NSFont.systemFont(ofSize: size, weight: style == .plain ? .semibold : .bold)
  var value = ceil(NSAttributedString(string: text, attributes: [.font: font]).size().width)
  if style == .cue { value += 20 }
  if widths.count > 2000 { widths = [:] }
  widths[key] = value
  return value
 }
 private func place(_ raw: String, _ detected: WordStyle, index: Int, confirmed: Bool, with placer: inout WordPlacer) -> Item {
  let style = highlight || detected == .cue ? detected : .plain
  let text = style == .cue ? (raw == "[music]" ? "♪ music" : String(raw.dropFirst().dropLast())) : raw
  let spot = placer.place(width: measure(text, style))
  return Item(id: "\(index)-\(text)", text: text, confirmed: confirmed, style: style, line: spot.line, x: spot.x)
 }
 private func placeTentative(_ texts: [String], stable: Int) {
  var placer = cursor
  let styles = wordStyles(texts)
  pending = texts.indices.map { place(texts[$0], styles[$0], index: total + $0, confirmed: $0 < stable, with: &placer) }
 }
 // Resizing or a new text size rewraps everything once. Otherwise words stay where they are.
 private func relayout() {
  let first = total - settled.count
  let tentative = pending.map(\.text), stable = pending.prefix { $0.confirmed }.count
  let texts = settled.map { $0.style == .cue ? "[\($0.text.replacingOccurrences(of: "♪ ", with: ""))]" : $0.text }
  let styles = wordStyles(texts)
  cursor = newCursor()
  settled = texts.indices.map { place(texts[$0], styles[$0], index: first + $0, confirmed: true, with: &cursor) }
  placeTentative(tentative, stable: stable)
  // The text has already changed size, so sliding the words into place would look broken.
  var transaction = Transaction(); transaction.disablesAnimations = true
  withTransaction(transaction) { publish() }
 }
 private func publish() {
  let first = total - settled.count
  let visible = (settled + pending).prefix(max(0, revealed - first))
  items = idle && !expanded ? [] : Array(visible)
 }
 private func pace() {
  guard pacing == nil, revealed < total + pending.count else { return }
  pacing = Task { @MainActor [weak self] in
   while let self, !Task.isCancelled, self.revealed < self.total + self.pending.count {
    self.revealed += 1; self.publish()
    let interval = revealInterval(backlog: self.total + self.pending.count - self.revealed + 1)
    try? await Task.sleep(for: .seconds(interval))
   }
   self?.pacing = nil
  }
 }
 // Old captions leave the screen but are kept, because expanding is for reading back. What
 // comes next starts on a fresh line.
 private func goIdle() {
  idle = true
  if pending.isEmpty, cursor.x > 0 { cursor.line += 1; cursor.x = 0 }
  publish()
 }
 private func scheduleExpiry() {
  expiry?.cancel()
  expiry = Task { @MainActor [weak self] in
   try? await Task.sleep(for: .seconds(8))
   if !Task.isCancelled { self?.goIdle() }
  }
 }
}
extension CaptionColor {
 var color: Color { Color(.sRGB, red: red, green: green, blue: blue) }
 init(_ color: Color) {
  let value = NSColor(color).usingColorSpace(.sRGB) ?? .white
  self.init(red: value.redComponent, green: value.greenComponent, blue: value.blueComponent)
 }
}
// A brief pop as a highlighted word lands. Only these words animate this way, and not at all
// under Reduce Motion: animating every word hurts reading.
private struct PopIn<Content: View>: View {
 @ViewBuilder let content: Content
 @State private var landed = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
 var body: some View {
  content.scaleEffect(landed ? 1 : 0.72, anchor: .bottomLeading)
   .onAppear { withAnimation(.spring(response: 0.24, dampingFraction: 0.55)) { landed = true } }
 }
}
// One word as drawn, shared by the overlay and the preview in Settings.
struct CaptionWord: View {
 let text: String
 let kind: WordStyle
 let fontSize: Double
 let style: CaptionStyle
 var body: some View {
  switch kind {
  case .plain:
   Text(text).font(.system(size: fontSize, weight: .semibold)).foregroundStyle(style.text.color)
  case .sound, .slang:
   PopIn { Text(text).font(.system(size: fontSize * 1.1, weight: .bold)).foregroundStyle(style.highlight.color) }
  case .cue:
   PopIn {
    Text(text).font(.system(size: fontSize * 0.68, weight: .semibold)).foregroundStyle(style.highlight.color)
     .padding(.horizontal, 10).frame(height: fontSize * 1.04)
     .background(style.highlight.color.opacity(0.14), in: Capsule())
     .overlay(Capsule().strokeBorder(style.highlight.color.opacity(0.45)))
   }
  }
 }
}
// With little background behind it, text needs an edge of its own or it vanishes against a
// video of the same color. The whole block is shadowed once, so this costs one pass per frame.
struct CaptionTextLook: ViewModifier {
 let style: CaptionStyle
 func body(content: Content) -> some View {
  Group {
   if style.isSeeThrough {
    content.compositingGroup()
     .shadow(color: style.edge.color.opacity(0.9), radius: 1, x: 0, y: 1)
     .shadow(color: style.edge.color.opacity(0.6), radius: 3)
   } else { content }
  }
  .opacity(style.textOpacity)
 }
}
// Roll-up captions: left-aligned, newest line at the bottom, older lines sliding up.
struct CaptionLines: View {
 @ObservedObject var feed: CaptionFeed
 let rows: Int
 private var font: Font { .system(size: feed.fontSize, weight: .semibold) }
 private var top: Int { (feed.items.last?.line ?? 0) - rows + 1 }
 // Larger words sit up by their extra height so every word shares a baseline.
 private func lift(_ item: CaptionFeed.Item) -> Double {
  switch item.style { case .plain: return 0; case .cue: return feed.fontSize * 0.1; default: return -feed.fontSize * 0.12 }
 }
 private func word(_ item: CaptionFeed.Item) -> some View {
  CaptionWord(text: item.text, kind: item.style, fontSize: feed.fontSize, style: feed.style)
   .opacity(item.confirmed ? 1 : 0.62).fixedSize()
   .offset(x: item.x, y: Double(item.line - top) * feed.lineHeight + lift(item)).transition(.opacity)
 }
 var body: some View {
  ZStack(alignment: .topLeading) {
   if feed.items.isEmpty, !feed.placeholder.isEmpty {
    Text(feed.placeholder).font(font).foregroundStyle(feed.style.text.color.opacity(0.6)).lineLimit(1).minimumScaleFactor(0.5).transition(.opacity)
   }
   ForEach(feed.items.filter { $0.line >= top - 1 }) { word($0) }
  }
  .modifier(CaptionTextLook(style: feed.style))
  .frame(maxWidth: .infinity, alignment: .topLeading)
  .frame(height: Double(rows) * feed.lineHeight, alignment: .topLeading)
  .clipped()
  .animation(.easeOut(duration: 0.18), value: feed.items)
  .accessibilityElement(children: .ignore).accessibilityLabel("Live captions")
  .accessibilityValue(feed.items.suffix(30).map(\.text).joined(separator: " "))
 }
}
// SwiftUI's onHover needs an active window, and the overlay never is one. A tracking area works
// in any window, and returning nil from hitTest leaves clicks for the view it sits behind.
final class HoverSensorView: NSView {
 var onHover: ((Bool) -> Void)?
 private var tracker: NSTrackingArea?
 override func hitTest(_ point: NSPoint) -> NSView? { nil }
 override func updateTrackingAreas() {
  super.updateTrackingAreas()
  guard tracker == nil else { return }
  let area = NSTrackingArea(rect: .zero, options: [.activeAlways, .mouseEnteredAndExited, .inVisibleRect], owner: self)
  addTrackingArea(area); tracker = area
 }
 override func mouseEntered(with event: NSEvent) { onHover?(true) }
 override func mouseExited(with event: NSEvent) { onHover?(false) }
}
struct HoverSensor: NSViewRepresentable {
 let onHover: (Bool) -> Void
 func makeNSView(context: Context) -> HoverSensorView { let view = HoverSensorView(); view.onHover = onHover; return view }
 func updateNSView(_ view: HoverSensorView, context: Context) { view.onHover = onHover }
}
// Shown on hover only, so the default overlay stays nothing but text. System tooltips don't
// appear on a panel that never takes focus, so each button names itself here instead.
struct OverlayControls: View {
 @ObservedObject var state: AppState
 @ObservedObject var feed: CaptionFeed
 @State private var hint: String?
 private func button(_ symbol: String, _ label: String, action: @escaping () -> Void) -> some View {
  Button(action: action) {
   Image(systemName: symbol).font(.system(size: 13, weight: .semibold)).foregroundStyle(Color.white)
    .frame(width: 28, height: 26).contentShape(Rectangle())
  }.buttonStyle(.plain).accessibilityLabel(label)
  .background(HoverSensor { inside in
   if inside { hint = label } else if hint == label { hint = nil }
  })
 }
 var body: some View {
  VStack(alignment: .trailing, spacing: 4) {
   HStack(spacing: 0) {
    button("textformat.size.smaller", "Smaller text") { state.fontSize = max(18, state.fontSize - 2) }
    button("textformat.size.larger", "Larger text") { state.fontSize = min(48, state.fontSize + 2) }
    button(feed.expanded ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right", feed.expanded ? "Collapse" : "Expand") { state.toggleExpanded() }
    button("cursorarrow.rays", "Let clicks pass through") { state.clickThrough = true }
    button("xmark", "Hide overlay") { state.hideOverlay() }
   }
   .padding(.horizontal, 3).background(Color.black.opacity(0.72), in: Capsule())
   .overlay(Capsule().strokeBorder(Color.white.opacity(0.14)))
   if let hint {
    Text(hint).font(.system(size: 11, weight: .medium)).foregroundStyle(Color.white).lineLimit(1)
     .padding(.horizontal, 9).frame(height: 20).background(Color.black.opacity(0.72), in: Capsule())
     .allowsHitTesting(false)
   }
  }
 }
}
// While clicks pass through, this is the one spot on the overlay that still takes a click.
struct ClickThroughBadge: View {
 static let size = CGSize(width: 182, height: 30)
 let action: () -> Void
 var body: some View {
  Button(action: action) {
   HStack(spacing: 6) {
    Image(systemName: "cursorarrow.rays")
    Text("Turn off click-through")
   }
   .font(.system(size: 12, weight: .semibold)).foregroundStyle(Color.white)
   .frame(width: Self.size.width, height: Self.size.height).contentShape(Capsule())
  }
  .buttonStyle(.plain).background(Color.black.opacity(0.72), in: Capsule())
  .overlay(Capsule().strokeBorder(Color.white.opacity(0.14)))
 }
}
// A borderless panel gets no resize edges from AppKit, so this view supplies them. It sits above
// the captions and claims only the border: anywhere else a click reaches the buttons or moves
// the panel. It stays in place during a drag, because the pointer can leave the panel at its
// minimum size and the view has to be there for the mouse-up.
final class OverlayResizer: NSView {
 static let zones = ResizeZones(band: 12, corner: 28, grip: 32)
 var onFinish: (() -> Void)?
 private var tracker: NSTrackingArea?
 private var dragging: ResizeEdges = []
 private var startFrame = NSRect.zero
 private var startMouse = NSPoint.zero
 override var mouseDownCanMoveWindow: Bool { false }
 override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
 private func edges(at point: NSPoint) -> ResizeEdges { Self.zones.edges(at: point, in: bounds.size) }
 private func edges(of event: NSEvent) -> ResizeEdges { edges(at: convert(event.locationInWindow, from: nil)) }
 private func screenPoint(_ event: NSEvent) -> NSPoint { window?.convertPoint(toScreen: event.locationInWindow) ?? .zero }
 override func hitTest(_ point: NSPoint) -> NSView? { edges(at: convert(point, from: superview)).isEmpty ? nil : self }
 override func updateTrackingAreas() {
  super.updateTrackingAreas()
  guard tracker == nil else { return }
  let area = NSTrackingArea(rect: .zero, options: [.activeAlways, .mouseMoved, .mouseEnteredAndExited, .inVisibleRect], owner: self)
  addTrackingArea(area); tracker = area
 }
 override func mouseMoved(with event: NSEvent) { if dragging.isEmpty { resizeCursor(for: edges(of: event)).set() } }
 override func mouseExited(with event: NSEvent) { if dragging.isEmpty { NSCursor.arrow.set() } }
 override func mouseDown(with event: NSEvent) {
  dragging = edges(of: event); startFrame = window?.frame ?? .zero; startMouse = screenPoint(event)
 }
 override func mouseDragged(with event: NSEvent) {
  guard let window, !dragging.isEmpty else { return }
  let mouse = screenPoint(event)
  let frame = resizedFrame(startFrame, edges: dragging, dx: mouse.x - startMouse.x, dy: mouse.y - startMouse.y, minimum: window.minSize)
  window.setFrame(frame, display: true)
 }
 override func mouseUp(with event: NSEvent) { dragging = []; onFinish?() }
}
private func resizeCursor(for edges: ResizeEdges) -> NSCursor {
 if edges.isEmpty { return .arrow }
 if #available(macOS 15, *) {
  let position: NSCursor.FrameResizePosition
  switch edges {
  case .left: position = .left
  case .right: position = .right
  case .top: position = .top
  case .bottom: position = .bottom
  case [.left, .top]: position = .topLeft
  case [.right, .top]: position = .topRight
  case [.left, .bottom]: position = .bottomLeft
  default: position = .bottomRight
  }
  return .frameResize(position: position, directions: .all)
 }
 if edges == .left || edges == .right { return .resizeLeftRight }
 if edges == .top || edges == .bottom { return .resizeUpDown }
 return .crosshair
}
// The visible hint for the bottom right corner. OverlayResizer takes the drag.
struct ResizeGrip: View {
 let color: Color
 var body: some View {
  Path { path in
   for (x, y) in [(10.0, 11.0), (15.0, 16.0), (20.0, 21.0)] {
    path.move(to: CGPoint(x: 21, y: y)); path.addLine(to: CGPoint(x: x, y: 22))
   }
  }
  .stroke(color.opacity(0.55), style: StrokeStyle(lineWidth: 1.6, lineCap: .round))
  .frame(width: 26, height: 26).allowsHitTesting(false)
 }
}
struct CaptionOverlay: View {
 @ObservedObject var state: AppState
 @ObservedObject var feed: CaptionFeed
 static let controlMargin = 7.0, panelMargin = 6.0
 private let inset = 22.0
 private var backing: Double {
  let resting = feed.items.isEmpty && feed.placeholder.isEmpty && !feed.hovering
  return resting ? min(0.05, feed.style.backgroundOpacity) : feed.style.backgroundOpacity
 }
 private func rows(_ height: Double) -> Int { max(1, Int((height - 28) / feed.lineHeight)) }
 var body: some View {
  GeometryReader { geometry in
   CaptionLines(feed: feed, rows: rows(geometry.size.height))
    .padding(.horizontal, inset)
    .frame(maxHeight: .infinity)
    .background(feed.style.background.color.opacity(backing), in: RoundedRectangle(cornerRadius: 16))
    .overlay {
     // With next to no background, this shows where the panel's edges are.
     if feed.hovering, feed.style.backgroundOpacity < 0.2 {
      RoundedRectangle(cornerRadius: 16).strokeBorder(Color.gray.opacity(0.8)).allowsHitTesting(false).transition(.opacity)
     }
    }
    .overlay(alignment: .topTrailing) {
     if feed.hovering {
      Group {
       if state.clickThrough { ClickThroughBadge { state.clickThrough = false } } else { OverlayControls(state: state, feed: feed) }
      }
      .padding(Self.controlMargin).transition(.opacity)
     }
    }
    .overlay(alignment: .bottomTrailing) {
     if feed.hovering, !state.clickThrough { ResizeGrip(color: feed.style.text.color).transition(.opacity) }
    }
    .animation(.easeOut(duration: 0.14), value: feed.hovering)
    .onAppear { feed.configure(width: geometry.size.width - inset * 2) }
    .onChange(of: geometry.size.width) { feed.configure(width: geometry.size.width - inset * 2) }
  }
  .padding(Self.panelMargin)
  .help("Drag to move. Resize from an edge or the corner.")
 }
}
// The panel never takes focus, so hover is tracked directly: SwiftUI's own hover tracking
// needs an active window, and clicks must work while Chrome stays frontmost.
final class OverlayHost: NSHostingView<CaptionOverlay> {
 var onHover: ((Bool) -> Void)?
 private var tracker: NSTrackingArea?
 // One area that follows the view's size. Replacing it on resize reports the mouse leaving.
 override func updateTrackingAreas() {
  super.updateTrackingAreas()
  guard tracker == nil else { return }
  let area = NSTrackingArea(rect: .zero, options: [.activeAlways, .mouseEnteredAndExited, .inVisibleRect], owner: self)
  addTrackingArea(area); tracker = area
 }
 override func mouseEntered(with event: NSEvent) { onHover?(true) }
 override func mouseExited(with event: NSEvent) { onHover?(false) }
 override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}
