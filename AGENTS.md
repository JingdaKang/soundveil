# Soundveil

This project is a single native macOS application. Keep its Swift package and app
source together here; do not modify neighboring projects.

## Components

- `Sources/Soundveil`: SwiftUI/AppKit UI, capture, inference coordination, models.
- `Sources/CaptionCore`: pure audio windowing, deduplication, bounded queue logic.
- `Sources/UpdateCore`: pure release-version parsing and GitHub release decoding.
- `Sources/CWhisperBridge`: C interface to the pinned whisper.cpp dependency.
- `Sources/SmokeCheck`: offline native transcription check.
- `Tests/CaptionCoreTests`: pipeline unit tests.
- `Tests/UpdateCoreTests`: update-check unit tests.

## Build and validation

Run `./scripts/build.sh` before `./scripts/test.sh`. The app is generated at
`dist/Soundveil.app`. Read README.md and VALIDATION.md for operating constraints
and evidence. Keep model weights, vendor dependencies, and build products out of Git.
After relocation, regenerate `.build/` and `vendor/whisper.cpp/build/`; their caches
contain absolute paths.
Releases are published with `./scripts/release.sh`; the app's update check compares
release tags (`vX.Y.Z`) with `CFBundleShortVersionString`, so bump it before releasing.

Keep capture, inference, and UI work on their existing queues/actor. Preserve the
bounded inference queue and document that replacing pending work loses coverage.
Do not describe offline smoke tests as verification of live capture, permissions,
overlay behavior, or large-model performance.
