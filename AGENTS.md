# Soundveil

Offline live captions for the audio playing on a computer. Each part is a self-contained
folder with its own toolchain and its own `AGENTS.md`. There is no root-level build.

## Layout

- `mac/`: the macOS app in Swift. This is the only released part.
- `core/`: shared caption logic in Rust.
- `desktop/`: the Windows app, in Rust with a Slint interface. Not released yet.
- `docs/`: product-level documents.

## Contracts between parts

- `mac/Sources/CaptionCore`, `ShortcutCore`, and `UpdateCore` are the tuned originals of the
  logic in `core/`. Until the Mac app moves onto `core/`, a behavior change must land in both.
- Every part uses whisper.cpp v1.8.7, commit `48f628a84833905ee4a0658ee6d4a5c915ce1997`.
- A model's raw value names its file on disk and upstream, in every part.
- Release tags are `vX.Y.Z`. Bump the last number only unless the owner asks otherwise, and
  never move to 1.0.0 unasked.
