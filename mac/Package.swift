// swift-tools-version: 5.10
import PackageDescription
import Foundation
let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let build = root + "/vendor/whisper.cpp/build"
let coreML: [LinkerSetting] = ProcessInfo.processInfo.environment["SOUNDVEIL_COREML"] == "ON" ? [.linkedLibrary("whisper.coreml")] : []
let package = Package(name: "Soundveil", platforms: [.macOS(.v14)], products: [.executable(name: "Soundveil", targets: ["Soundveil"])], targets: [
 .target(name: "CaptionCore"),
 .target(name: "UpdateCore"),
 .target(name: "ShortcutCore"),
 .target(name: "CWhisperBridge", cSettings: [.unsafeFlags(["-I" + root + "/vendor/whisper.cpp/include", "-I" + root + "/vendor/whisper.cpp/ggml/include"])], linkerSettings: [.unsafeFlags(["-L" + build + "/src", "-L" + build + "/ggml/src", "-L" + build + "/ggml/src/ggml-metal", "-L" + build + "/ggml/src/ggml-blas", "-L" + build + "/ggml/src/ggml-cpu"]), .linkedLibrary("whisper"), .linkedLibrary("ggml"), .linkedLibrary("ggml-base"), .linkedLibrary("ggml-cpu"), .linkedLibrary("ggml-metal"), .linkedLibrary("ggml-blas"), .linkedLibrary("c++"), .linkedFramework("Accelerate"), .linkedFramework("Metal"), .linkedFramework("MetalKit"), .linkedFramework("Foundation"), .linkedFramework("CoreML")] + coreML),
 .target(name: "WhisperEngine", dependencies: ["CaptionCore", "CWhisperBridge"]),
 .executableTarget(name: "Soundveil", dependencies: ["CaptionCore", "WhisperEngine", "UpdateCore", "ShortcutCore"]),
 .executableTarget(name: "SmokeCheck", dependencies: ["CWhisperBridge"]),
 .executableTarget(name: "StreamCheck", dependencies: ["CaptionCore", "WhisperEngine"]),
 .executableTarget(name: "CoreTrace", dependencies: ["CaptionCore"]),
 .testTarget(name: "CaptionCoreTests", dependencies: ["CaptionCore"]),
 .testTarget(name: "UpdateCoreTests", dependencies: ["UpdateCore"]),
 .testTarget(name: "ShortcutCoreTests", dependencies: ["ShortcutCore"])
])
