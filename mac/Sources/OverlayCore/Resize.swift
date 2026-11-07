import CoreGraphics

// The sides of the overlay a drag moves. Coordinates match AppKit: the origin is bottom-left.
public struct ResizeEdges: OptionSet, Sendable {
 public let rawValue: Int
 public init(rawValue: Int) { self.rawValue = rawValue }
 public static let left = ResizeEdges(rawValue: 1), right = ResizeEdges(rawValue: 2)
 public static let bottom = ResizeEdges(rawValue: 4), top = ResizeEdges(rawValue: 8)
}
public struct ResizeZones: Sendable {
 public var band: CGFloat // How far in from the panel's edge a drag resizes.
 public var corner: CGFloat // How far along an edge still counts as the corner.
 public var grip: CGFloat // The square at the bottom right, where the visible grip sits.
 public init(band: CGFloat, corner: CGFloat, grip: CGFloat) { self.band = band; self.corner = corner; self.grip = grip }
 // Empty means the point is not on the border, so the click belongs to the content.
 public func edges(at point: CGPoint, in size: CGSize) -> ResizeEdges {
  guard CGRect(origin: .zero, size: size).contains(point) else { return [] }
  if point.x >= size.width - grip, point.y < grip { return [.right, .bottom] }
  var edges: ResizeEdges = []
  if point.x < band { edges.insert(.left) } else if point.x >= size.width - band { edges.insert(.right) }
  if point.y < band { edges.insert(.bottom) } else if point.y >= size.height - band { edges.insert(.top) }
  if edges == .left || edges == .right {
   if point.y < corner { edges.insert(.bottom) } else if point.y >= size.height - corner { edges.insert(.top) }
  } else if edges == .top || edges == .bottom {
   if point.x < corner { edges.insert(.left) } else if point.x >= size.width - corner { edges.insert(.right) }
  }
  return edges
 }
}
// The side opposite a dragged edge stays put, even when the minimum size stops the drag.
public func resizedFrame(_ start: CGRect, edges: ResizeEdges, dx: CGFloat, dy: CGFloat, minimum: CGSize) -> CGRect {
 var frame = start
 if edges.contains(.right) { frame.size.width = max(minimum.width, start.width + dx) }
 if edges.contains(.left) {
  frame.size.width = max(minimum.width, start.width - dx)
  frame.origin.x = start.maxX - frame.width
 }
 if edges.contains(.top) { frame.size.height = max(minimum.height, start.height + dy) }
 if edges.contains(.bottom) {
  frame.size.height = max(minimum.height, start.height - dy)
  frame.origin.y = start.maxY - frame.height
 }
 return frame
}
