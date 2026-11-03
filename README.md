# Soundveil

*Give sound a written form.*

A native SwiftUI macOS app for offline English captions of Chrome videos and live streams. MIT licensed, no account, subscriptions, paid APIs, translation, or microphone capture. Model downloads and the launch-time update check require internet; recognition does not. The Apple operating system and SDK remain proprietary prerequisites.

## Run the built app

Open `dist/Soundveil.app` in Finder. This locally built app is ad-hoc signed, not notarized. Keep it in a stable location so recording permissions remain associated with it.

1. Open Chrome and play a video. Mute other audible Chrome tabs.
2. Choose **Large v3 Turbo**, then **Download model (free)**. It needs about 1.6 GB of disk space; **Large v3** needs about 3.1 GB. Downloads stream to disk, support cancellation, and install only after completion and a GGML header check. Files are not cryptographically verified by this MVP.
3. Select **Chrome (all audible tabs)** and **Start captions**.
4. Accept macOS's native recording-permission prompt. Depending on macOS, the setting is named **Screen Recording** or **Screen & System Audio Recording** under **System Settings → Privacy & Security**. Enable Soundveil and relaunch if requested. The app uses ScreenCaptureKit's native authorization flow; it cannot grant itself access.
5. Drag the caption panel to your preferred position. Resize at its edges, adjust caption size in the main window, or hide/show it. It floats above ordinary windows and requests full-screen/Spaces participation; exclusive full-screen applications may behave differently.
6. Stop before changing source, model, or quiet-speech threshold. Stop discards unfinished audio and waits for any current inference call to finish before unloading the model.

The level indicator shows captured audio. If it stays empty, check Chrome playback, permissions, or try **System audio**. System mode includes other applications' sounds, excluding Soundveil itself. No BlackHole/loopback driver is required. ScreenCaptureKit needs a display-based filter even for audio capture; the app subscribes only to audio and never saves screen images.

Models are stored at:

```text
~/Library/Application Support/FreeCaptions/Models/
  ggml-large-v3-turbo.bin
  ggml-large-v3.bin
```

The original `FreeCaptions` storage directory is retained for compatibility with existing downloads.
Soundveil has a new app identity; macOS may request recording permission again even
if you previously granted it to FreeCaptions.

Use **Open model folder** to install compatible GGML files manually or remove them to reclaim space. Model files are ignored by Git. Existing files are checked by whisper.cpp when loaded. History is kept in memory (latest 2,000 segments), reset on Start, and saved only when you choose **Export**. Quit discards it.

## Reproducible build

Prerequisites: Apple Silicon, macOS 14+, full Xcode 16.2 or later with command-line tools selected, Git, and CMake 3.20+. The implementation was compiled against the installed macOS 15.2 SDK; it does not assume newer APIs are available. No developer subscription is required for a local ad-hoc-signed build.

```bash
# Install CMake if necessary using Homebrew:
brew install cmake
# From this project's directory:
./scripts/build.sh
./scripts/test.sh
open dist/Soundveil.app
```

`build.sh` fetches whisper.cpp v1.8.7 and verifies commit `48f628a84833905ee4a0658ee6d4a5c915ce1997`. It builds static ARM64 libraries with Metal, embedded Metal shaders, and Accelerate, then builds and signs the app. Subsequent builds reuse the local checkout. No inference server or process-per-chunk is used. For editing, open `Package.swift` in Xcode after the dependency build; use the script-built app bundle for permission/playback testing. The package uses Swift 5.10 language mode, tested with Swift 6.0.3.

## Build a drag-and-drop DMG

```bash
./scripts/package-dmg.sh
# Or package the app you already built:
./scripts/package-dmg.sh --skip-build
```

The script creates `dist/Soundveil-0.1.0-arm64.dmg` (version read from the app),
plus a `.sha256` checksum. It contains `Soundveil.app`, an `Applications` shortcut,
and installation instructions. Open the DMG, drag the app onto Applications,
eject the image, and launch the installed app. Models download separately.

This is an ad-hoc-signed development build, not a notarized public release.
Creating a DMG does not change macOS Gatekeeper trust. A normal public release
requires a separate Developer ID signing and Apple notarization workflow;
see [Apple's distribution guidance](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution).
The packaging script verifies the app signature and disk-image integrity, but does
not perform notarization or upload a GitHub release.

## Updates

Soundveil does not install updates itself. At launch it asks the GitHub Releases API
for this repository's latest published release; the request contains no captions,
audio, or settings. If that release's tag is newer than the running version, the main
window shows a banner whose **Download** button opens the disk image in your browser.
Replace the app in Applications with the new one. **Soundveil → Check for Updates…**
checks on demand and reports the result; **Check for Updates Automatically** turns the
launch-time check off. Drafts and prereleases are never offered, and automatic-check
failures are silent.

Ad-hoc signatures change with every build, so macOS may ask for recording permission
again after an update. Signed, in-place updates (for example with Sparkle) would
require Developer ID signing and notarization.

To publish a release, bump the last number of `CFBundleShortVersionString` in
`Info.plist` (0.1.0 → 0.1.1; minor and major bumps are deliberate decisions), commit,
and push, then run:

```bash
./scripts/release.sh
```

It refuses to run with uncommitted or unpushed changes, an existing tag, or a version
older than the latest release. It builds and packages the DMG, then creates the
`vX.Y.Z` GitHub release with the DMG and checksum attached. Pass `--skip-build` to
package the app already in `dist/`. Requires the GitHub CLI (`gh`), signed in.

## Pipeline and concurrency

```mermaid
flowchart LR
 C[Chrome or system audio] --> S[ScreenCaptureKit: serial capture queue]
 S --> R[AVAudioConverter: 48 kHz stereo to 16 kHz mono]
 R --> V[Energy VAD and 300 ms pre-roll]
 V --> W[6 s windows, 1 s overlap; 600 ms silence endpoint]
 W --> Q[One active decode and one replaceable pending window]
 Q --> M[whisper.cpp C bridge: serial worker, Metal]
 M --> D[Timestamp and text-overlap deduplication]
 D --> U[Main actor: SwiftUI history and floating NSPanel]
```

- Capture and conversion use one serial queue. Inference and context destruction use another; model loading never blocks the main actor. A lock protects the single pending window. UI work runs on the main actor. C pointers never cross into UI code.
- Continuous speech emits every five seconds after the initial six-second window. Silence closes shorter utterances; speech bursts shorter than 100 ms are rejected. VAD is a lightweight RMS energy gate, **not a neural speech classifier**. Music/noise can trigger it; quiet speech can be missed. Lower the threshold for quieter speech, at the cost of admitting more noise.
- Samples are copied out of ScreenCaptureKit buffers before returning. Stateful AVAudioConverter resamples and mixes to mono. Capture presentation timestamps detect discontinuities; gaps reset the audio window and resampler rather than joining unrelated audio.
- Model segment timestamps are translated into the session timeline. Dedup removes already-covered segments and normalized suffix/prefix word overlap. This is heuristic; it is not perfect word alignment or a rolling partial-transcript decoder.
- The last 600 characters of accepted captions condition the next decode. Explicit English language, no translation, beam search of five, and non-speech-token suppression are configured. Gaps reset context to reduce carry-over hallucinations.
- If inference falls behind, the latest pending window replaces the older one. Status reports cumulative skipped windows. This bounds latency/memory **by deliberately losing coverage** under load. Switch to Turbo or close GPU-heavy workloads if this happens.
- Stop prevents stale results from reaching the UI, drains native inference safely, and then frees its context. It is not an immediate GPU cancellation. Captions clear after ten seconds without new text.

## Quality, latency, and limitations

Large v3 is the quality-oriented option; Turbo reduces decoding cost. Neither guarantees correct recognition of muffled, fast, accented, overlapping, or music-obscured speech. Source captions, if accurate and available, remain preferable to ASR. No denoising or source separation is included: aggressive filters can remove speech detail, and those features need separate evaluation.

Expect seconds of delay: capture window/endpoint time **plus** model decoding. This MVP does not offer subsecond partial captions. Model loading and the first GPU inference may be slow; large models need several GB of working memory beyond download size. 16 GB or more unified memory is a practical starting recommendation, not a measured minimum. This build has not been benchmarked with the two large models. Heat, Chrome playback, and other GPU work affect throughput.

Chrome mode captures the application, **not an individual tab or window's independent audio track**. macOS window selection does not reliably provide per-tab audio isolation. Chrome Beta/Canary bundle identifiers under `com.google.Chrome.*` are included. Protected/DRM content may be silent or unavailable. Chrome helper audio routing, fullscreen placement, sleep/wake, and display changes need interactive verification on your Mac; restart capture after a display/source interruption.

The overlay has a transparent window with a translucent dark caption background, white scalable text, and accessibility labels. Long segments fit up to four lines; the full text remains in history. This first version has no click-through toggle, neural VAD, automatic reconnect, download resume, or background transcript persistence.

## Optional Core ML encoder

Metal is enabled and sufficient for the default build. To compile the optional Core ML bridge:

```bash
COREML=ON ./scripts/build.sh
COREML=ON ./scripts/test.sh
```

You must separately generate a matching compiled encoder following the [pinned upstream Core ML instructions](https://github.com/ggml-org/whisper.cpp/tree/v1.8.7#core-ml-support). They recommend Python 3.11 with `ane_transformers`, `openai-whisper`, and `coremltools`, then `models/generate-coreml-model.sh MODEL_NAME`. Put `ggml-large-v3-encoder.mlmodelc` or `ggml-large-v3-turbo-encoder.mlmodelc` beside its matching `.bin` file. Conversion downloads extra weights and consumes substantial disk/memory. Do not substitute an encoder from another model. Conversion compatibility and Neural Engine performance are not validated here. Re-run the default build to return to Metal-only. Core ML is optional; it is not automatically downloaded or enabled by the UI.

## Validation and manual acceptance

See `VALIDATION.md` for checks actually performed. Run the automated core tests after changes:

```bash
./scripts/test.sh
```

An additional native inference smoke executable exercises the C bridge and AVAudioConverter without recording permission:

```bash
# Supply any compatible local GGML model; tiny.en is sufficient for plumbing checks.
CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache" \
  swift run --disable-sandbox --cache-path "$PWD/.build/cache" \
  SmokeCheck /absolute/path/ggml-tiny.en.bin vendor/whisper.cpp/samples/jfk.wav
```

For manual acceptance, test permission denial/retry, Chrome and system capture, mute/unmute, quiet speech, fast/accented clips, long continuous speech, repeated words, switching models after Stop, canceling a download, fullscreen/Spaces, and stopping during inference. Verify caption lag and skipped-window count during a ten-minute stream. No automated test can establish recognition quality for your content without representative audio and a reference transcript.

## Source files

- `Package.swift`: Swift package, static native linkage, optional Core ML linkage.
- `Info.plist`: app identity, deployment target, recording usage description.
- `Sources/CaptionCore/Pipeline.swift`: chunking/VAD, timestamped captions, deduplication, bounded queue.
- `Sources/CWhisperBridge/include/Bridge.h`, `Sources/CWhisperBridge/Bridge.c`: narrow native inference API.
- `Sources/Soundveil/App.swift`: SwiftUI controls, session lifecycle, overlay, history/export.
- `Sources/Soundveil/Capture.swift`: ScreenCaptureKit, PCM extraction, resampling, discontinuity handling, audio level.
- `Sources/Soundveil/Inference.swift`: model worker, backpressure, prompt context, caption delivery.
- `Sources/Soundveil/Models.swift`: model choices, local storage and download progress/cancellation.
- `Sources/Soundveil/Updates.swift`: GitHub latest-release request.
- `Sources/UpdateCore/Release.swift`: release-tag version parsing, ordering, and response decoding.
- `Sources/SmokeCheck/main.swift`: offline native inference smoke test.
- `Tests/CaptionCoreTests/PipelineTests.swift`: pipeline unit tests.
- `Tests/UpdateCoreTests/ReleaseTests.swift`: version and release-response unit tests.
- `scripts/build.sh`, `scripts/test.sh`: reproducible build/package/sign and test commands.
- `scripts/package-dmg.sh`, `scripts/release.sh`: disk-image packaging and GitHub release publishing.
- `README.md`, `VALIDATION.md`, `LICENSE`, `.gitignore`: documentation, evidence, MIT license, generated-artifact exclusions.
- `vendor/whisper.cpp/`: downloaded upstream source and its own license, excluded from Git.
- `.build/`, `dist/`: generated build products; `dist/Soundveil.app` is the user-facing app.

## References and licenses

[Apple ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit) and the installed SDK headers informed capture integration. Apple recommends its content-sharing picker for user-selected screen/window workflows; this app instead uses an explicit Chrome/system application filter and the normal recording-permission flow. [whisper.cpp v1.8.7](https://github.com/ggml-org/whisper.cpp/tree/v1.8.7) provides the MIT-licensed local inference engine. [GGML model distribution](https://huggingface.co/ggerganov/whisper.cpp) supplies downloadable converted weights. Upstream model licenses apply separately. Bundled app resources include this project's and whisper.cpp's licenses.
