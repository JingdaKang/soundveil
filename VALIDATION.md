# Validation — 3 November 2026

## Update check

- Six new XCTest cases cover version parsing and numeric ordering, reading the DMG
  asset from a GitHub response, falling back to the release page, ignoring drafts,
  prereleases, and malformed tags, and offering only newer versions. All 15 cases passed.
- Release build passed. In the app, **Check for Updates…** against the live GitHub API
  (no releases published, HTTP 404) reported "Soundveil 0.1.0 is up to date".
- A temporary build returning a fake 0.2.0 release showed the banner at launch; **Later**
  hid it, the manual check showed the Download/Later alert, and with automatic checks
  turned off no banner appeared at launch. The fake was removed before committing.
- `release.sh` syntax and its usage and uncommitted-changes refusals were checked.
  Publishing a release, the GitHub-generated notes, and **Download** opening a real
  asset were not exercised; no release exists yet.

## DMG packaging checks

- Packaged the existing release app as `Soundveil-0.1.0-arm64.dmg`.
- Disk-image integrity verification and SHA-256 checksum verification passed.
- Mounted the DMG read-only and verified the bundled app's strict code signature
  and the `Applications` symlink target, then ejected the verification mount.
- The package is ad-hoc signed, not notarized; downloaded-file Gatekeeper behavior
  and installation on a separate Mac were not tested.

## Soundveil rename checks

- Rebuilt the default Metal release app from fresh Swift and CMake caches after
  renaming the source module, executable, app bundle, and bundle identifier.
- All nine pipeline XCTest cases passed with zero failures.
- Strict signature verification and Info.plist validation passed for
  `dist/Soundveil.app`.
- Existing model storage remains at `~/Library/Application Support/FreeCaptions/Models`.
- Live capture and native inference were not rerun for the rename; previous
  acceptance limitations still apply.

## Relocation checks

Rebuilt from a fresh project location on 3 November 2026.
The copied source, dependency checkout, and original deliverable matched the old
location by SHA-256 before rebuilding. Old Swift and CMake build directories were
excluded; new caches were generated at the destination.

- Default Metal release build and app packaging passed; Core ML disabled.
- All nine pipeline XCTest cases passed, with zero failures.
- Strict app signature verification and Info.plist validation passed.
- Native inference, app launch, optional Core ML compilation, and live capture
  were not repeated during relocation. The earlier results below are historical.

`AGENTS.md` records the component layout and build/validation constraints.

## Passed

- Apple Silicon compilation with Xcode 16.2 (16C5032a), Swift 6.0.3, macOS 15.2 SDK, deployment target macOS 14.
- Pinned whisper.cpp native static build with Metal/Accelerate and embedded Metal shaders.
- Swift release build, app-bundle generation, plist validation, ad-hoc signing, strict signature verification.
- Optional `COREML=ON` release compilation/linking. Final delivered app was rebuilt with the default Metal configuration.
- Nine XCTest cases: silence rejection, overlapping-window sample bounds/timestamps, silence endpoint/pre-roll, fragmented-input invariance, overlapping-text deduplication, legitimate repetition after a gap, bounded pending queue, timeline offset after a gap, and short-noise rejection. Zero failures.
- Real C-bridge inference with downloaded `tiny.en` and upstream `samples/jfk.wav`, using native Metal access. Result: “And so my fellow Americans ask not what your country can do for you ask what you can do for your country.”
- Same transcription passed with the WAV converted to 48 kHz stereo, exercising AVAudioConverter downmix/resampling to 16 kHz mono.
- Launch Services opened the app; its running process was confirmed. Linked dynamic dependencies are Apple system libraries/frameworks; whisper.cpp is statically linked.

## Fixed during validation

- Corrected an optional-binding mismatch against the installed AVAudioFormat SDK signature.
- Set the native library deployment target to macOS 14 to match the Swift package and eliminate deployment-version linker warnings.
- Added capture timestamp discontinuity handling, adjustable speech threshold, audio-level feedback, and two additional boundary tests.

The restricted test environment could not allocate Metal buffers and the first inference attempt terminated inside the native library. Repeating the same test with native GPU access passed. This was not treated as evidence of a successful restricted-environment run.

## Not verified

- Live Chrome or system audio acquisition after the user's interactive recording permission; no claim of end-to-end playback success.
- Denied/revoked permissions, DRM/protected media, Chrome helper routing, full-screen overlay placement, Spaces, sleep/wake, multi-display changes, and overlay visual/accessibility behavior.
- Large v3 / Large v3 Turbo loading, recognition quality, memory use, or live latency. Those multi-GB weights were not downloaded during implementation; the app's download UI remains an interactive acceptance item.
- Core ML encoder generation, encoder/model matching, Neural Engine execution, and performance. Only the optional build path was compiled.
- Long-running load/backpressure behavior, network download cancellation/failure scenarios, or transcript quality on muffled/fast/accented audio.

`SmokeCheck` tests native transcription and conversion, not ScreenCaptureKit. Core unit tests exercise pure pipeline logic, not all session/UI concurrency. See README for manual acceptance steps and performance limitations.
