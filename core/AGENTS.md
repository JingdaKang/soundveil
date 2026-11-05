# Soundveil core

Shared caption logic in Rust, for the Windows and Linux app and later platforms. Read `../AGENTS.md`
for how this fits with the other parts.

## Crates

- `soundveil-core`: pure logic with no audio, model, or UI dependency, so it builds and tests
  on every platform. Speech gate, rolling transcriber, word styles, token joining, model
  catalog, shortcuts, and the release check.
- `soundveil-whisper`: runs decode jobs on the pinned whisper.cpp through `bridge/`, a narrow
  C API. Run `./scripts/fetch-whisper.sh` once before building it. Metal on macOS; the
  `vulkan` feature adds GPU support on Windows and Linux and needs the Vulkan SDK.
- `stream-check`: offline latency and accuracy replay, the twin of the Mac app's StreamCheck.

## Validation

`cargo test -p soundveil-core` needs no model and runs in CI on Windows x64 and Arm64, Linux,
and macOS.

The logic was ported from `mac/Sources` and tuned there, so the Swift code is the reference
until the Mac app moves onto this crate. After changing logic in either language, run:

- `./scripts/compare-with-swift.sh`: both run on the same seeded input with a scripted decoder.
  Every event, job, and caption update must match exactly.
- `stream-check` and the Mac app's `StreamCheck` with `pass=0.3`, which fixes the simulated
  clock, on real speech. Their output must match too.

Do not describe either as verification of live capture, permissions, or an overlay.

The settings in `RollingTranscriber` came from measurement. Keep the audio context the same
for every pass, do not pass a prompt to Whisper, and do not cut audio on its word timestamps.
Only add a word to the lists in `expressive.rs` if it is rarely ordinary dialogue.

Text in this crate is shown on every platform, so it must not name one. Key codes and modifier
bits stay in each operating system's own values; `Keyboard` holds what differs.
