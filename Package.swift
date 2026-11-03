// swift-tools-version: 5.10
import PackageDescription
let package = Package(name: "FreeCaptions", platforms: [.macOS(.v14)], products: [
 .library(name: "CaptionCore", targets: ["CaptionCore"])
], targets: [
 .target(name: "CaptionCore"),
 .testTarget(name: "CaptionCoreTests", dependencies: ["CaptionCore"])
])
