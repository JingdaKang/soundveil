// A release build opens no console window on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(any(windows, test))]
mod audio;
mod capture;
mod download;
mod engine;
mod paths;
mod settings;

use capture::{Capture, Sink};
use engine::{Engine, Reports};
use settings::Settings;
use slint::{ComponentHandle, Model as _, ModelRc, SharedString, VecModel, Weak};
use soundveil_core::{CaptionUpdate, Keyboard, Model, ShortcutAction, UpdateSpeed};
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

slint::include_modules!();

const KEEP_WORDS: usize = 40;
const KEEP_LINES: usize = 2000;
const NO_CAPTURE: &str = "Audio capture isn't built yet. Run with --replay and a recording.";

#[cfg(target_os = "macos")]
const KEYBOARD: Keyboard = Keyboard::MAC;
#[cfg(not(target_os = "macos"))]
const KEYBOARD: Keyboard = Keyboard::WINDOWS;

struct Session {
    engine: Arc<Engine>,
    capture: Capture,
}

/// State shared with worker threads. The interface itself is only touched on its own thread.
#[derive(Default)]
struct Shared {
    session: Mutex<Option<Session>>,
    number: AtomicU64,
    live: Mutex<Vec<String>>,
    cancel: Arc<AtomicBool>,
    replay: Option<PathBuf>,
}

fn strings(values: impl Iterator<Item = &'static str>) -> ModelRc<SharedString> {
    let list: Vec<SharedString> = values.map(SharedString::from).collect();
    ModelRc::new(VecModel::from(list))
}

fn clock(seconds: f64) -> String {
    let whole = seconds as u64;
    format!("{:02}:{:02}", whole / 60, whole % 60)
}

fn chosen_model(ui: &MainWindow) -> Model {
    Model::ALL[ui.get_model_index() as usize]
}

fn refresh(ui: &MainWindow) {
    let model = chosen_model(ui);
    let installed = paths::model_file(model).exists();
    ui.set_installed(installed);
    ui.set_model_detail(model.detail().into());
    if !ui.get_listening() && !ui.get_busy() && !ui.get_downloading() {
        ui.set_status(
            if installed {
                "Ready. Play a video, then start captions."
            } else {
                "Download a model to get started."
            }
            .into(),
        );
    }
}

fn save(ui: &MainWindow) {
    let settings = Settings {
        model: chosen_model(ui).raw_value().into(),
        update_speed: UpdateSpeed::ALL[ui.get_speed_index() as usize]
            .raw_value()
            .into(),
        caption_size: ui.get_caption_size(),
        quiet_threshold: ui.get_threshold(),
        highlight_words: ui.get_highlight(),
        click_through: ui.get_click_through(),
    };
    if let Err(error) = settings.save() {
        ui.set_status(error.into());
    }
}

fn clear_captions(ui: &MainWindow, shared: &Shared, placeholder: &str) {
    shared
        .live
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    ui.set_confirmed("".into());
    ui.set_tentative("".into());
    ui.set_placeholder(placeholder.into());
}

fn with_history(ui: &MainWindow, change: impl FnOnce(&VecModel<HistoryLine>)) {
    let model = ui.get_history();
    if let Some(lines) = model.as_any().downcast_ref::<VecModel<HistoryLine>>() {
        change(lines);
        ui.set_history_count(lines.row_count() as i32);
    }
}

fn receive(ui: &MainWindow, shared: &Shared, update: CaptionUpdate) {
    let mut live = shared.live.lock().unwrap_or_else(PoisonError::into_inner);
    live.extend(update.committed.into_iter().map(|word| word.text));
    let excess = live.len().saturating_sub(KEEP_WORDS);
    live.drain(..excess);
    ui.set_confirmed(live.join(" ").into());
    if let Some(tentative) = update.tentative {
        let words: Vec<String> = tentative.into_iter().map(|word| word.text).collect();
        ui.set_tentative(words.join(" ").into());
    }
    if !live.is_empty() || !ui.get_tentative().is_empty() {
        ui.set_placeholder("".into());
    }
    with_history(ui, |lines| {
        for caption in update.sentences {
            lines.push(HistoryLine {
                time: clock(caption.start).into(),
                text: caption.text.into(),
            });
        }
        while lines.row_count() > KEEP_LINES {
            lines.remove(0);
        }
    });
}

fn start(ui: &MainWindow, shared: &Arc<Shared>) {
    let Some(recording) = shared.replay.clone() else {
        ui.set_status(NO_CAPTURE.into());
        return;
    };
    ui.set_busy(true);
    ui.set_status("Loading the model. The first run can take up to a minute…".into());
    clear_captions(ui, shared, "Waiting for speech…");
    with_history(ui, |lines| lines.set_vec(Vec::new()));
    let number = shared.number.fetch_add(1, Ordering::Relaxed) + 1;
    let current = move |shared: &Shared| shared.number.load(Ordering::Relaxed) == number;
    let model = paths::model_file(chosen_model(ui));
    let speed = UpdateSpeed::ALL[ui.get_speed_index() as usize];
    let threshold = 10_f32.powf(ui.get_threshold() / 20.0);
    let (weak, shared) = (ui.as_weak(), shared.clone());
    thread::spawn(move || {
        let reports = {
            let (on_update, on_status) = (weak.clone(), weak.clone());
            let (update_shared, status_shared) = (shared.clone(), shared.clone());
            Reports {
                update: Box::new(move |update| {
                    let shared = update_shared.clone();
                    let _ = on_update.upgrade_in_event_loop(move |ui| {
                        if current(&shared) {
                            receive(&ui, &shared, update);
                        }
                    });
                }),
                status: Box::new(move |text| {
                    let shared = status_shared.clone();
                    let _ = on_status.upgrade_in_event_loop(move |ui| {
                        if current(&shared) {
                            ui.set_status(text.into());
                        }
                    });
                }),
            }
        };
        let engine = match Engine::start(model, speed, reports) {
            Ok(engine) => Arc::new(engine),
            Err(error) => return fail(&weak, error),
        };
        let sink = {
            let (engine, levels, ended) = (engine.clone(), weak.clone(), weak.clone());
            let (level_shared, end_shared) = (shared.clone(), shared.clone());
            Sink {
                events: Box::new(move |events| engine.submit(events)),
                level: Box::new(move |level| {
                    let shared = level_shared.clone();
                    let _ = levels.upgrade_in_event_loop(move |ui| {
                        if current(&shared) {
                            ui.set_level(level * 10.0);
                        }
                    });
                }),
                finished: Box::new(move || {
                    let shared = end_shared.clone();
                    let _ = ended.upgrade_in_event_loop(move |ui| {
                        if current(&shared) {
                            stop(&ui, &shared);
                        }
                    });
                }),
            }
        };
        match capture::replay(&recording, threshold, sink) {
            Ok(capture) => {
                *shared
                    .session
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(Session { engine, capture });
                let _ = weak.upgrade_in_event_loop(|ui| {
                    ui.set_busy(false);
                    ui.set_listening(true);
                    ui.set_status("Listening".into());
                });
            }
            Err(error) => {
                engine.stop();
                fail(&weak, error);
            }
        }
    });
}

fn fail(weak: &Weak<MainWindow>, error: String) {
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_busy(false);
        ui.set_placeholder("".into());
        ui.set_status(error.into());
    });
}

fn stop(ui: &MainWindow, shared: &Arc<Shared>) {
    let session = shared
        .session
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    let Some(session) = session else {
        return;
    };
    ui.set_busy(true);
    ui.set_listening(false);
    ui.set_level(0.0);
    ui.set_status("Stopping…".into());
    let (weak, shared) = (ui.as_weak(), shared.clone());
    thread::spawn(move || {
        session.capture.stop();
        let update = session.engine.stop();
        let _ = weak.upgrade_in_event_loop(move |ui| {
            receive(&ui, &shared, update);
            ui.set_busy(false);
            ui.set_status("Stopped".into());
        });
    });
}

fn download(ui: &MainWindow, shared: &Arc<Shared>) {
    let model = chosen_model(ui);
    ui.set_downloading(true);
    ui.set_progress(0.0);
    ui.set_status(format!("Downloading {}…", model.name()).into());
    shared.cancel.store(false, Ordering::Relaxed);
    let (weak, cancel) = (ui.as_weak(), shared.cancel.clone());
    thread::spawn(move || {
        let reporter = weak.clone();
        let result = download::fetch(
            &model.download_url(),
            &paths::model_file(model),
            &cancel,
            move |value| {
                let _ = reporter.upgrade_in_event_loop(move |ui| ui.set_progress(value as f32));
            },
        );
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_downloading(false);
            refresh(&ui);
            if let Err(error) = result {
                ui.set_status(error.into());
            }
        });
    });
}

fn open_models() {
    let folder = paths::models();
    let _ = std::fs::create_dir_all(&folder);
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = Command::new(opener).arg(folder).spawn();
}

fn main() -> Result<(), slint::PlatformError> {
    let mut arguments = std::env::args().skip(1);
    let mut replay = None;
    while let Some(argument) = arguments.next() {
        if argument == "--replay" {
            replay = arguments.next().map(PathBuf::from);
        }
    }
    let shared = Arc::new(Shared {
        replay,
        ..Shared::default()
    });
    let settings = Settings::load();
    let ui = MainWindow::new()?;
    let saved_model = Model::saved(Some(&settings.model));
    let saved_speed = UpdateSpeed::saved(Some(&settings.update_speed));
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    ui.set_model_titles(strings(Model::ALL.iter().map(|model| model.title())));
    ui.set_speed_titles(strings(UpdateSpeed::ALL.iter().map(|speed| speed.title())));
    ui.set_model_index(
        Model::ALL
            .iter()
            .position(|m| *m == saved_model)
            .unwrap_or(3) as i32,
    );
    ui.set_speed_index(
        UpdateSpeed::ALL
            .iter()
            .position(|s| *s == saved_speed)
            .unwrap_or(0) as i32,
    );
    ui.set_speed_detail(saved_speed.detail().into());
    ui.set_caption_size(settings.caption_size);
    ui.set_threshold(settings.quiet_threshold);
    ui.set_highlight(settings.highlight_words);
    ui.set_click_through(settings.click_through);
    ui.set_placeholder("Your live captions appear here".into());
    ui.set_history(ModelRc::from(Rc::new(VecModel::<HistoryLine>::default())));
    let defaults = KEYBOARD.default_shortcuts();
    let rows: Vec<ShortcutRow> = ShortcutAction::ALL
        .iter()
        .map(|action| ShortcutRow {
            title: action.title().into(),
            keys: defaults
                .get(*action)
                .map_or("Not set".to_string(), |shortcut| shortcut.label(&KEYBOARD))
                .into(),
        })
        .collect();
    ui.set_shortcuts(ModelRc::new(VecModel::from(rows)));
    refresh(&ui);

    ui.on_model_chosen({
        let weak = ui.as_weak();
        move |_| {
            if let Some(ui) = weak.upgrade() {
                refresh(&ui);
                save(&ui);
            }
        }
    });
    ui.on_speed_chosen({
        let weak = ui.as_weak();
        move |index| {
            if let Some(ui) = weak.upgrade() {
                ui.set_speed_detail(UpdateSpeed::ALL[index as usize].detail().into());
                save(&ui);
            }
        }
    });
    ui.on_settings_changed({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                save(&ui);
            }
        }
    });
    ui.on_toggle_captions({
        let (weak, shared) = (ui.as_weak(), shared.clone());
        move || {
            if let Some(ui) = weak.upgrade() {
                if ui.get_listening() {
                    stop(&ui, &shared);
                } else {
                    start(&ui, &shared);
                }
            }
        }
    });
    ui.on_download_model({
        let (weak, shared) = (ui.as_weak(), shared.clone());
        move || {
            if let Some(ui) = weak.upgrade() {
                download(&ui, &shared);
            }
        }
    });
    ui.on_cancel_download({
        let shared = shared.clone();
        move || shared.cancel.store(true, Ordering::Relaxed)
    });
    ui.on_open_models(open_models);

    #[cfg(feature = "snapshot")]
    snapshot::schedule(&ui);
    ui.run()
}

/// Saves pictures of the window, so the interface can be checked without a person.
/// `SOUNDVEIL_SNAPSHOT` names the file, and `SOUNDVEIL_SNAPSHOT_AT` is a list of seconds.
#[cfg(feature = "snapshot")]
mod snapshot {
    use super::*;
    use slint::{Timer, TimerMode};
    use std::time::Duration;

    pub fn schedule(ui: &MainWindow) {
        let Ok(path) = std::env::var("SOUNDVEIL_SNAPSHOT") else {
            return;
        };
        let times = std::env::var("SOUNDVEIL_SNAPSHOT_AT").unwrap_or("1".into());
        let times: Vec<f64> = times.split(',').filter_map(|t| t.parse().ok()).collect();
        let last = times.iter().copied().fold(0.0, f64::max);
        if let Ok(page) = std::env::var("SOUNDVEIL_PAGE") {
            ui.set_page(page.parse().unwrap_or(0));
        }
        if std::env::var_os("SOUNDVEIL_AUTOSTART").is_some() {
            ui.invoke_toggle_captions();
        }
        // Adds history lines quickly, to check the list without a long recording.
        if let Some(lines) = std::env::var("SOUNDVEIL_FILL")
            .ok()
            .and_then(|n| n.parse().ok())
        {
            let weak = ui.as_weak();
            let added = std::cell::Cell::new(0_usize);
            let filler = Box::leak(Box::new(Timer::default()));
            filler.start(TimerMode::Repeated, Duration::from_millis(40), move || {
                if added.get() < lines
                    && let Some(ui) = weak.upgrade()
                {
                    with_history(&ui, |list| {
                        for _ in 0..6 {
                            added.set(added.get() + 1);
                            list.push(HistoryLine {
                                time: clock(added.get() as f64).into(),
                                text: format!("Line {} of the history.", added.get()).into(),
                            });
                        }
                    });
                }
            });
        }
        for (index, seconds) in times.into_iter().enumerate() {
            let weak = ui.as_weak();
            let file = path.replace(".png", &format!("-{index}.png"));
            Timer::single_shot(Duration::from_secs_f64(seconds), move || {
                if let Some(ui) = weak.upgrade() {
                    let picture = ui.window().take_snapshot().expect("window snapshot");
                    let out = std::fs::File::create(&file).expect("snapshot file");
                    let mut encoder = png::Encoder::new(out, picture.width(), picture.height());
                    encoder.set_color(png::ColorType::Rgba);
                    let mut writer = encoder.write_header().expect("snapshot header");
                    writer
                        .write_image_data(picture.as_bytes())
                        .expect("snapshot data");
                    eprintln!("snapshot {file}");
                }
            });
        }
        let timer = Box::leak(Box::new(Timer::default()));
        timer.start(
            TimerMode::SingleShot,
            Duration::from_secs_f64(last + 0.5),
            || {
                let _ = slint::quit_event_loop();
            },
        );
    }
}
