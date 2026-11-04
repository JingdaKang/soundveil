# Validation — 3 November 2026

## Expressive captions and overlay controls — 4 November 2026

- 53 unit tests pass, including word detection for sound words, slang, shouted-only words,
  and fillers that stay plain.
- A real overlay window, fed by replaying synthetic speech, highlighted "Boom.", "sus, no
  cap.", "Let's go.", "Oops,", "Wow.", "Yikes.", and "Ha ha.", and left "the baby boom was
  based on real data" plain.
- In that window, Expand grew the panel from 156 to 472 pt with its bottom edge fixed, and
  brought back text that had left the screen. Collapse returned to a dragged 660×136. The
  corner grip, larger text, Hide, and Reset Caption Overlay all worked.
- Allowing cues on final passes left word error at 0.7%.

**Not verified:** Whisper reported no cue on synthetic applause or music, and invented
"Thank you." over the music, so the laughter, applause, and music pills are unproven on
real audio. Highlighting has only been seen on synthetic speech, which misheard "Bruh" as
"Bra" and most of "Ha ha ha". Chrome-only capture passed an audio probe but has not been
confirmed through full captions on the installed build.

The owner confirmed the controls appear on real pointer hover.

## Rolling captions — 4 November 2026

Replaces the five-second windows. Measured offline with `StreamCheck` on an Apple M4 Pro
with Large v3 Turbo: real decoding on a simulated live clock, so audio "arrives" while
each pass runs. Word error is against the known text. The speech is macOS `say` output,
except `jfk.wav`. The noisy clip adds noise and a 220 Hz tone that hide the pauses.

| Audio | Length | First shown, mean (p95) | Final text, mean | On-screen words replaced | Word error |
|---|---|---|---|---|---|
| US voice | 93 s | 0.33 s (0.79 s) | 2.35 s | 15% | 0.7% |
| British voice | 97 s | 0.34 s (0.81 s) | 2.57 s | 21% | 0.7% |
| Fast speech | 50 s | 0.60 s (1.74 s) | 3.64 s | 11% | 0.7% |
| Noisy | 93 s | 1.22 s (3.83 s) | 5.80 s | 42% | 8.0% |

- A single offline pass over the US voice scored 0.7%, so the live result matches it.
- Passes took 211 ms on average (p95 236 ms) with no skipped audio.
- `jfk.wav`, the only real speech tested offline, was word for word correct except for
  an invented "Yeah!" at the breath at the end.
- Unit tests: 45 cases pass. The overlay view was rendered to images and showed left-aligned
  roll-up, dimmed tentative words, stable wrapping, and a larger text size correctly.

What the measurements changed, with results in README.md: prompts removed, audio cut at
pauses instead of on Whisper's word timestamps, one fixed audio context, and two passes
required to change the screen.

### Live test

Run on a recorded talk in the owner's own Chrome tab, with Chrome on a different desktop
from Soundveil, for about five minutes.

- Captions appeared in the overlay and history, and the status line read about 0.2 s
  behind. That figure is pass time, not a measured end-to-end delay.
- CPU averaged 19% (peak 33%), memory was about 2.05 GB, and nothing was reported skipped.
- About 150 words were compared by eye with the official transcript. About 6 were wrong,
  mostly in the first sentence over applause ("I'm a Honoured" for "I am honored"). This
  is a spot check, not a score, because no export was made.
- Rebuilding kept recording permission with no reset: the binary hash changed and the
  stored permission, which names the signing certificate, still applied.
- Probing each capture filter on real Chrome audio with Chrome on another desktop:
  including Chrome and excluding other apps both heard it, and listing on-screen apps only
  found no Chrome at all. Inclusion stays, because exclusion also let through sound from an
  app with no windows.
- That run used a build with the exclusion filter. Chrome-only inclusion has been probed
  on real audio but not yet run through the full pipeline.

A Chrome tab driven by browser automation gave silence to every capture setup, even a
whole-system one, although it reported playing and unmuted. The cause was not established.
Live tests need a tab the owner started.

**Not verified:** smoothness is pending the owner's judgment. Overlay behavior over
full-screen video, a ten-minute run, a formal accuracy score on real speech, music, and
Large v3 were not tested.

## Live caption test — 4 November 2026

First end-to-end run: the installed v0.1.0 DMG build, Large v3 Turbo, Chrome mode, and
about three minutes of a spoken YouTube talk played by the owner.

- Live Chrome audio reached the app and captions appeared in history and the overlay.
- Decode took 0.6–1.3 s per window with 0 skipped windows. Average CPU was 15% (peak
  21%), memory about 1.85 GB, and no thermal warning was recorded.
- Captions arrive in steps exactly 5 s apart during continuous speech, as designed.
  From the window timing, a word can wait about 1 s to 7 s after it is spoken. The
  owner reported this as too slow and too jumpy.
- The first model load took up to about 50 s while macOS compiled the Metal shaders;
  later loads finished within 6 s.
- Denied recording permission showed an error, and Start worked after permission was
  granted, reopening the app.

Problems found:

- **Chrome on another Space:** capture listed on-screen windows only, so Start failed
  with "Open Chrome and start playing a video" if no Chrome window was on the current
  desktop. Since fixed; the fix has not been tested live.
- **Permission after an update:** macOS stores the signature with the permission. After
  installing a new ad-hoc build the switch stayed on while capture was refused, and
  turning it off and on did not update the stored signature.
  `tccutil reset ScreenCapture org.soundveil.mac` did.
- **Repeated words:** overlap removal missed a near match ("storage IOS" then "storage
  IO") and showed the phrase twice. Windows no longer overlap.
- **Punctuation:** capitals and punctuation mostly disappeared after the first lines,
  because unpunctuated text was fed back as the prompt. There is no prompt now.
- **Messages:** the launch status asked for a model download when one was installed, and
  errors ran macOS's raw text into the app's advice. Both fixed.

Not tested: accuracy against a reference transcript, a ten-minute run, overlay behavior
over full-screen video or across Spaces, mute/unmute, sleep/wake, display changes, and
Large v3.

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
- Published `v0.1.0` with `release.sh`. The latest-release API returned it with the
  DMG and checksum assets, the downloaded DMG matched the published SHA-256, and the
  tag points at the release commit. The app's decoder, run against that live
  response, read version 0.1.0 and chose the DMG over the checksum file.
- Not tested: an installed app offering a real newer release, **Download** opening the
  asset in a browser, and Gatekeeper's handling of the downloaded DMG.

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
