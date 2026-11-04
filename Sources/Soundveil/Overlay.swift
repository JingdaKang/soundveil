import SwiftUI
import AppKit
import CaptionCore

// Words on screen. Updates arrive several times a second, so this stays apart from AppState
// and the main window is not redrawn for each word. Confirmed words are placed once and
// never move; tentative words sit after them and may change.
@MainActor final class CaptionFeed: ObservableObject {
 struct Item: Identifiable, Equatable {
  let id: String
  let text: String
  let confirmed: Bool
  let line: Int
  let x: Double
 }
 @Published private(set) var items: [Item] = []
 @Published var placeholder = "Your live captions appear here"
 @Published var fontSize: Double = 28 { didSet { if fontSize != oldValue { widths = [:]; relayout() } } }
 var lineHeight: Double { (fontSize * 1.3).rounded() }
 private var width = 0.0
 private var widths: [String: Double] = [:]
 private var settled: [Item] = []
 private var pending: [Item] = []
 private var cursor = WordPlacer(space: 7, maxWidth: 800)
 private var total = 0
 private var revealed = 0
 private var pacing: Task<Void, Never>?
 private var expiry: Task<Void, Never>?
 func configure(width: Double) { if width > 0, width != self.width { self.width = width; relayout() } }
 func apply(_ update: CaptionUpdate) {
  let shown = revealed
  for word in update.committed { settled.append(place(word.text, index: total, confirmed: true, with: &cursor)); total += 1 }
  if settled.count > 150 { settled.removeFirst(settled.count - 150) }
  if let tentative = update.tentative { placeTentative(tentative.map(\.text), stable: update.stable) }
  // A final pass rewrites words the viewer already saw, so it must not restart their reveal.
  revealed = min(max(shown, total - (update.committed.isEmpty ? 0 : pending.count)), total + pending.count)
  if !settled.isEmpty || !pending.isEmpty { placeholder = "" }
  publish(); pace()
  if !update.committed.isEmpty || !(update.tentative ?? []).isEmpty { scheduleExpiry() }
 }
 func reset(placeholder: String) {
  pacing?.cancel(); pacing = nil; expiry?.cancel(); expiry = nil
  settled = []; pending = []; total = 0; revealed = 0; cursor = newCursor()
  self.placeholder = placeholder; publish()
 }
 private func newCursor() -> WordPlacer { WordPlacer(space: (fontSize * 0.26).rounded(), maxWidth: width > 0 ? width : 800) }
 private func measure(_ text: String) -> Double {
  if let known = widths[text] { return known }
  let font = NSFont.systemFont(ofSize: fontSize, weight: .semibold)
  let value = ceil(NSAttributedString(string: text, attributes: [.font: font]).size().width)
  if widths.count > 2000 { widths = [:] }
  widths[text] = value
  return value
 }
 private func place(_ text: String, index: Int, confirmed: Bool, with placer: inout WordPlacer) -> Item {
  let spot = placer.place(width: measure(text))
  return Item(id: "\(index)-\(text)", text: text, confirmed: confirmed, line: spot.line, x: spot.x)
 }
 private func placeTentative(_ texts: [String], stable: Int) {
  var placer = cursor
  pending = texts.enumerated().map { place($1, index: total + $0, confirmed: $0 < stable, with: &placer) }
 }
 // Resizing or a new text size rewraps everything once. Otherwise words stay where they are.
 private func relayout() {
  let first = total - settled.count
  let tentative = pending.map(\.text), stable = pending.prefix { $0.confirmed }.count
  cursor = newCursor()
  settled = settled.enumerated().map { place($1.text, index: first + $0, confirmed: true, with: &cursor) }
  placeTentative(tentative, stable: stable); publish()
 }
 private func publish() {
  let first = total - settled.count
  let visible = (settled + pending).prefix(max(0, revealed - first))
  items = Array(visible)
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
 private func scheduleExpiry() {
  expiry?.cancel()
  expiry = Task { @MainActor [weak self] in
   try? await Task.sleep(for: .seconds(8))
   if !Task.isCancelled { self?.reset(placeholder: "") }
  }
 }
}
// Roll-up captions: left-aligned, newest line at the bottom, older lines sliding up.
struct CaptionLines: View {
 @ObservedObject var feed: CaptionFeed
 let rows: Int
 private var font: Font { .system(size: feed.fontSize, weight: .semibold) }
 private var top: Int { (feed.items.last?.line ?? 0) - rows + 1 }
 private func word(_ item: CaptionFeed.Item) -> some View {
  Text(item.text).font(font).foregroundStyle(Color.white).opacity(item.confirmed ? 1 : 0.62).fixedSize()
   .offset(x: item.x, y: Double(item.line - top) * feed.lineHeight).transition(.opacity)
 }
 var body: some View {
  ZStack(alignment: .topLeading) {
   if feed.items.isEmpty, !feed.placeholder.isEmpty {
    Text(feed.placeholder).font(font).foregroundStyle(Color.white.opacity(0.6)).lineLimit(1).minimumScaleFactor(0.5).transition(.opacity)
   }
   ForEach(feed.items.filter { $0.line >= top - 1 }) { word($0) }
  }
  .frame(maxWidth: .infinity, alignment: .topLeading)
  .frame(height: Double(rows) * feed.lineHeight, alignment: .topLeading)
  .clipped()
  .animation(.easeOut(duration: 0.18), value: feed.items)
 }
}
struct CaptionOverlay: View {
 @ObservedObject var feed: CaptionFeed
 private let inset = 22.0
 private var backing: Double { feed.items.isEmpty && feed.placeholder.isEmpty ? 0.05 : 0.78 }
 private func rows(_ height: Double) -> Int { max(1, Int((height - 28) / feed.lineHeight)) }
 var body: some View {
  GeometryReader { geometry in
   CaptionLines(feed: feed, rows: rows(geometry.size.height))
    .padding(.horizontal, inset)
    .frame(maxHeight: .infinity)
    .background(Color.black.opacity(backing), in: RoundedRectangle(cornerRadius: 16))
    .onAppear { feed.configure(width: geometry.size.width - inset * 2) }
    .onChange(of: geometry.size.width) { feed.configure(width: geometry.size.width - inset * 2) }
  }
  .padding(6)
  .accessibilityElement(children: .ignore).accessibilityLabel("Live captions")
  .accessibilityValue(feed.items.suffix(30).map(\.text).joined(separator: " "))
  .help("Drag to move; resize from an edge. Hide using the main window.")
 }
}
