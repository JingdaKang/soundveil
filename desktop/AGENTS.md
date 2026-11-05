# Soundveil desktop

The Windows and Linux app, in Rust with a Slint interface. It uses the crates in `../core`. Read
`../AGENTS.md` for how this fits with the other parts, and run `../core/scripts/fetch-whisper.sh`
once before building.

## State

Everything is written: capture, overlay, shortcuts, export, and packaging. Keep this list honest
about what has actually been seen working.

- Seen working on a Mac, which also runs this app: the interface, settings, model download
  against a local server, recognition, captions, the overlay's look and layout, shortcut
  recording, and export text.
- Seen working in CI: the app and its overlay open on Windows and Linux, and `--capture-check`
  hears a tone at the right level from a real PulseAudio server. The Windows installer and Linux
  packages build, with Vulkan. Those machines have no GPU or speakers.
- Compiled but never run: WASAPI capture, and installing either package. Windows and Linux
  shortcuts, click-through, and moving the overlay have never been tried by a person. Wayland
  shortcuts through the portal have never been run at all.
- Not built yet: the update check, and slowing down when hot or on battery (`engine.rs` passes
  `false` to `update_interval`, and the Automatic speed's description promises it).

On a Mac there is no live capture, so use `--replay recording.wav`, which plays 16 kHz mono audio
through the pipeline at live speed.

## Layout

- `ui/`: the interface, with no logic. `app.slint` is the main window, `overlay.slint` the floating
  window, and `captions.slint` the word view both of them show.
- `src/main.rs`: connects the interface to everything else.
- `src/engine.rs`: the recognition thread. Keep one pass at a time, on the newest audio, and
  preserve the bounded audio buffer. Dropping audio loses coverage, and the status line says so.
- `src/capture/`: audio sources. `wasapi.rs` is Windows, `pulse.rs` is Linux, and `mod.rs` holds
  replay and what they share. `src/audio.rs` converts Windows audio to 16 kHz mono.
- `src/feed.rs`: which words are on screen, with no interface in it. `src/captions.rs` copies that
  into the model the windows share, touching only rows that changed.
- `src/overlay.rs`: the overlay window's behaviour. Slint draws it, and winit does what Slint has no
  property for: click-through, moving by the background, and resizing.
- `src/hotkeys.rs`: shortcuts through the operating system. `src/portal.rs` asks the desktop instead
  on Wayland, where the desktop picks the final keys and Settings only lists them.
- `src/check.rs`: `--capture-check 5` tests capture with no model or window.
- `src/download.rs`: model download. Only a complete file with a GGML header is installed.
- `src/settings.rs`, `src/paths.rs`, `src/export.rs`: saved choices, file locations, history export.
- `packaging/`: the icon, the Windows installer, and the Linux packages. `.github/workflows/package.yml`
  builds them with Vulkan as artifacts. Publishing a release stays a separate step.

The interface may only be touched on its own thread. Worker threads use
`upgrade_in_event_loop`, and each session has a number so late reports from an old one are dropped.

## Decisions to keep

- The overlay is one Slint window for both systems, so it can be looked at on a Mac. This relies on
  Slint's `unstable-winit-030` feature, which is why the version is pinned to `~1.18`. Check
  `overlay.rs` when moving to a newer Slint.
- Linux runs on the X11 layer even on Wayland (`leave_wayland` in `main.rs`), because Wayland lets no
  app stay on top or choose its position. A Wayland desktop with no X11 layer gets an ordinary window.
- Windows uses loopback on the default output, not process loopback, so it works on Windows 10 and
  needs no permission. It hears every app, and it sends nothing during silence, so `QuietClock` fills
  that in. Speech detection can't end a sentence otherwise.
- `core/soundveil-whisper/build.rs` turns off the build machine's processor features for x86_64, so
  a release needs AVX2 and nothing newer.
- The installer ships `vulkan-1.dll`, because the program links to it and won't start without one.
- A Windows build with Vulkan needs a short `CARGO_TARGET_DIR`. whisper.cpp's shader step nests deep
  enough to pass the path limit from `desktop\\target`.

## Validation

`cargo test` needs no model or network. To see the interface without a person, build with
`--features snapshot` and set `SOUNDVEIL_SNAPSHOT=out.png` and `SOUNDVEIL_SNAPSHOT_AT=1.5,20`.
`SOUNDVEIL_PAGE=1` opens Settings, `SOUNDVEIL_AUTOSTART=1` presses Start, `SOUNDVEIL_FILL=48`
adds history lines, `SOUNDVEIL_OVERLAY=1` shows the overlay, and `SOUNDVEIL_SHOT=overlay` pictures it
in place of the main window. `SOUNDVEIL_HOVER_AT=10` points at the overlay at that second, and
`SOUNDVEIL_EXPAND=1` also expands it. `SOUNDVEIL_SCRIPT="click 577 576; key F9"` clicks and types. A
window that is covered may not draw between pictures, so take several. Set `HOME` to an empty folder
when a run might save settings, because on a Mac it otherwise shares the Mac app's folder.

To compile for another system from a Mac, add the Rust target and set `SOUNDVEIL_CHECK_ONLY=1`, which
skips whisper.cpp and the Windows icon. Other C libraries still need a stand-in compiler, because
`cargo check` links nothing. This proves that the Rust types are right, and nothing else.

CI opens the app on Windows and Linux and keeps pictures of both windows as artifacts, and on Linux
it runs `--capture-check` against a real audio server. Those are hosted machines with no GPU or
speakers, so they don't show that the overlay looks right on a real screen or that WASAPI works.

A replay exercises recognition and the interface. Do not describe it as verification of capture,
or of Windows or Linux. Performance figures from a Mac say nothing about either.

The Slint royalty-free license requires the `AboutSlint` panel on the About page. Keep it there.
