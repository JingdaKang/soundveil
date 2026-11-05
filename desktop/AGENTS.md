# Soundveil desktop

The Windows app, in Rust with a Slint interface. Linux can follow from the same code. It uses
the crates in `../core`. Read `../AGENTS.md` for how this fits with the other parts, and run
`../core/scripts/fetch-whisper.sh` once before building.

## State

Everything after audio capture works: the interface, settings, model download, recognition,
and captions. These are not built yet, and all of them need a Windows computer to test:

- Capture with WASAPI process loopback. Until then, `--replay recording.wav` plays a 16 kHz
  mono recording through the pipeline at live speed. Start reports that capture is missing
  without it.
- The overlay, as its own Win32 and Direct2D window in Rust. Slint documents no transparent or
  click-through windows, so it can't draw this one. The stage in the main window is a preview.
- Global shortcuts with RegisterHotKey, and recording them. Settings lists the defaults.
- Export, the update check, and an installer.

## Layout

- `ui/app.slint`: the interface. It holds no logic; `src/main.rs` connects it.
- `src/engine.rs`: the recognition thread. Keep one pass at a time, on the newest audio, and
  preserve the bounded audio buffer. Dropping audio loses coverage, and the status line says so.
- `src/capture.rs`: audio sources.
- `src/download.rs`: model download. Only a complete file with a GGML header is installed.
- `src/settings.rs`, `src/paths.rs`: saved choices and file locations.

The interface may only be touched on its own thread. Worker threads use
`upgrade_in_event_loop`, and each session has a number so late reports from an old one are dropped.

## Validation

`cargo test` needs no model or network. To see the interface without a person, build with
`--features snapshot` and set `SOUNDVEIL_SNAPSHOT=out.png` and `SOUNDVEIL_SNAPSHOT_AT=1.5,20`.
`SOUNDVEIL_PAGE=1` opens Settings, and `SOUNDVEIL_AUTOSTART=1` presses Start.

A replay exercises recognition and the interface. Do not describe it as verification of capture,
an overlay, or Windows. Performance figures from a Mac say nothing about Windows.

The Slint royalty-free license requires the `AboutSlint` panel on the About page. Keep it there.
