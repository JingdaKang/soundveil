# Soundveil

Offline live captions for the audio playing on your computer. Speech is recognized on your own
machine with whisper.cpp, and no audio or text leaves it.

| Part | What it is | Status |
| --- | --- | --- |
| [`mac/`](mac/README.md) | The macOS app, in Swift | Released |
| [`core/`](core/AGENTS.md) | Shared caption logic, in Rust | In progress, for Windows and later platforms |
| [`desktop/`](desktop/AGENTS.md) | The Windows app, in Rust and Slint | In progress, not usable yet |

To install or build the Mac app, see [`mac/README.md`](mac/README.md).
