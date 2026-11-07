import XCTest
@testable import OverlayCore
final class ResizeTests: XCTestCase {
 let zones = ResizeZones(band: 12, corner: 24, grip: 30)
 let size = CGSize(width: 800, height: 200)
 let minimum = CGSize(width: 400, height: 90)
 let start = CGRect(x: 100, y: 100, width: 800, height: 200)
 func edges(_ x: CGFloat, _ y: CGFloat) -> ResizeEdges { zones.edges(at: CGPoint(x: x, y: y), in: size) }
 func testTheMiddleBelongsToTheContent() {
  XCTAssertEqual(edges(400, 100), [])
  XCTAssertEqual(edges(12, 100), [])
 }
 func testEachEdgeResizesItsOwnSide() {
  XCTAssertEqual(edges(3, 100), .left)
  XCTAssertEqual(edges(797, 100), .right)
  XCTAssertEqual(edges(400, 3), .bottom)
  XCTAssertEqual(edges(400, 197), .top)
 }
 func testPointsNearACornerMoveBothSides() {
  XCTAssertEqual(edges(3, 20), [.left, .bottom])
  XCTAssertEqual(edges(20, 197), [.left, .top])
  XCTAssertEqual(edges(780, 197), [.right, .top])
 }
 // The overlay's buttons sit just inside the top right corner and must stay clickable.
 func testOnlyTheBottomRightHasASquareGrip() {
  XCTAssertEqual(edges(780, 20), [.right, .bottom])
  XCTAssertEqual(edges(780, 180), [])
  XCTAssertEqual(edges(20, 20), [])
 }
 func testPointsOutsideThePanelAreNotEdges() { XCTAssertEqual(edges(-1, 100), []) }
 func testEachEdgeLeavesTheOppositeSideInPlace() {
  let left = resizedFrame(start, edges: .left, dx: -50, dy: 7, minimum: minimum)
  XCTAssertEqual(left, CGRect(x: 50, y: 100, width: 850, height: 200))
  let right = resizedFrame(start, edges: .right, dx: 50, dy: 7, minimum: minimum)
  XCTAssertEqual(right, CGRect(x: 100, y: 100, width: 850, height: 200))
  let top = resizedFrame(start, edges: .top, dx: 7, dy: 40, minimum: minimum)
  XCTAssertEqual(top, CGRect(x: 100, y: 100, width: 800, height: 240))
  let bottom = resizedFrame(start, edges: .bottom, dx: 7, dy: -40, minimum: minimum)
  XCTAssertEqual(bottom, CGRect(x: 100, y: 60, width: 800, height: 240))
 }
 func testACornerResizesBothWays() {
  XCTAssertEqual(resizedFrame(start, edges: [.right, .bottom], dx: 30, dy: 30, minimum: minimum), CGRect(x: 100, y: 130, width: 830, height: 170))
 }
 func testTheMinimumSizeStopsADragWithoutMovingTheFarSide() {
  let frame = resizedFrame(start, edges: [.left, .bottom], dx: 900, dy: 900, minimum: minimum)
  XCTAssertEqual(frame.size, minimum)
  XCTAssertEqual(frame.maxX, start.maxX)
  XCTAssertEqual(frame.maxY, start.maxY)
 }
}
