# Soundveil for Mac

This folder is the native macOS application, and the only released part of Soundveil.
Keep its Swift package and app source together here. Read `../AGENTS.md` for how the
parts of the project relate.

## Components

- `Sources/Soundveil`: SwiftUI/AppKit UI, capture, inference coordination, models.
- `Sources/CaptionCore`: pure speech gate, rolling transcriber, word layout, and expressive word logic.
- `Sources/WhisperEngine`: Swift wrapper over the bridge for rolling decode jobs.
- `Sources/ShortcutCore`: pure shortcut actions, validation, and storage.
- `Sources/UpdateCore`: pure release-version parsing and GitHub release decoding.
- `Sources/CWhisperBridge`: C interface to the pinned whisper.cpp dependency.
- `Sources/SmokeCheck`: offline native transcription check.
- `Sources/StreamCheck`: offline latency and accuracy replay of the rolling pipeline.
- `Tests/CaptionCoreTests`: pipeline unit tests.
- `Tests/UpdateCoreTests`: update-check unit tests.
- `Tests/ShortcutCoreTests`: shortcut unit tests.

## Build and validation

Run `./scripts/build.sh` before `./scripts/test.sh`. The app is generated at
`dist/Soundveil.app`. Read README.md and VALIDATION.md for operating constraints
and evidence. Keep model weights, vendor dependencies, and build products out of Git.
After relocation, regenerate `.build/` and `vendor/whisper.cpp/build/`; their caches
contain absolute paths.
Releases are published with `./scripts/release.sh`; the app's update check compares
release tags (`vX.Y.Z`) with `CFBundleShortVersionString`, so bump it before releasing.
Bump the last number only (0.1.0 → 0.1.1 → 0.1.2). Do not raise the minor or major
number, and never move to 1.0.0, unless the owner asks for it.

Keep capture, inference, and UI work on their existing queues/actor. Preserve the
bounded audio buffer and document that dropping audio loses coverage.
Do not describe offline smoke tests or StreamCheck as verification of live capture,
permissions, overlay behavior, or large-model performance.

Large v3 Turbo stays the default until another model is measured with StreamCheck. Do not
publish figures for a model that has not been measured. `Model` raw values name the files
on disk and upstream, so renaming one orphans existing downloads.

The rolling pipeline's settings came from measurement, and README.md explains why.
Check any change to them with StreamCheck against a reference transcript. In particular:
keep the audio context the same for every pass, do not pass a prompt to Whisper, and do
not cut audio on its word timestamps. Keep highlighting sparing: only add a word to the
lists in `Expressive.swift` if it is rarely ordinary dialogue.
