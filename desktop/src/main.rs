// A release build opens no console window on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(any(windows, test))]
mod audio;
mod capture;
mod captions;
mod check;
mod download;
mod engine;
mod export;
mod feed;
mod hotkeys;
mod overlay;
mod paths;
mod pointer;
#[cfg(target_os = "linux")]
mod portal;
mod settings;

use capture::{Capture, Sink};
use captions::Captions;
use engine::{Engine, Reports};
use hotkeys::Hotkeys;
use overlay::Overlay;
use settings::Settings;
use slint::{ComponentHandle, Model as _, ModelRc, SharedString, VecModel, Weak};
use soundveil_core::{
    CaptionUpdate, Held, Keyboard, Model, ResizeEdges, ResizeZones, ShortcutAction, ShortcutMap,
    UpdateSpeed,
};
use std::path::PathBuf;
use std::cell::{OnceCell, RefCell};
use std::process::{Command, ExitCode};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

slint::include_modules!();

const KEEP_LINES: usize = 2000;
const SIZE_STEP: f32 = 2.0;
const SMALLEST: f32 = 18.0;
const LARGEST: f32 = 48.0;

#[cfg(target_os = "macos")]
const KEYBOARD: Keyboard = Keyboard::MAC;
#[cfg(windows)]
const KEYBOARD: Keyboard = Keyboard::WINDOWS;
#[cfg(not(any(windows, target_os = "macos")))]
const KEYBOARD: Keyboard = Keyboard::LINUX;

struct Session {
    engine: Arc<Engine>,
    capture: Capture,
}

/// State shared with worker threads. The interface itself is only touched on its own thread.
#[derive(Default)]
struct Shared {
    session: Mutex<Option<Session>>,
    number: AtomicU64,
    cancel: Arc<AtomicBool>,
    replay: Option<PathBuf>,
}

/// What belongs to the interface thread alone.
struct View {
    captions: Rc<Captions>,
    overlay: Rc<Overlay>,
    shortcuts: RefCell<ShortcutMap>,
    /// `None` where the desktop owns the keys, as it does on Wayland.
    hotkeys: Option<RefCell<Hotkeys>>,
    /// Who else has a key, by action, and anything that explains the keys as a whole.
    clashes: RefCell<Vec<ShortcutAction>>,
}

thread_local! {
    static VIEW: OnceCell<View> = const { OnceCell::new() };
}

fn with_view<T>(use_view: impl FnOnce(&View) -> T) -> T {
    VIEW.with(|view| use_view(view.get().expect("the view is set up before anything runs")))
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
        overlay: with_view(|view| view.overlay.frame()),
    };
    if let Err(error) = settings.save() {
        ui.set_status(error.into());
    }
}

fn set_placeholder(ui: &MainWindow, text: &str) {
    ui.set_placeholder(text.into());
    with_view(|view| view.overlay.window.set_placeholder(text.into()));
}

fn clear_captions(ui: &MainWindow, placeholder: &str) {
    with_view(|view| view.captions.reset());
    set_placeholder(ui, placeholder);
}

fn with_history(ui: &MainWindow, change: impl FnOnce(&VecModel<HistoryLine>)) {
    let model = ui.get_history();
    if let Some(lines) = model.as_any().downcast_ref::<VecModel<HistoryLine>>() {
        change(lines);
        ui.set_history_count(lines.row_count() as i32);
    }
}

fn receive(ui: &MainWindow, update: CaptionUpdate) {
    if with_view(|view| view.captions.apply(&update)) {
        set_placeholder(ui, "");
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
    let recording = shared.replay.clone();
    ui.set_busy(true);
    ui.set_status("Loading the model. The first run can take up to a minute…".into());
    clear_captions(ui, "Waiting for speech…");
    with_history(ui, |lines| lines.set_vec(Vec::new()));
    let number = shared.number.fetch_add(1, Ordering::Relaxed) + 1;
    let current = move |shared: &Shared| shared.number.load(Ordering::Relaxed) == number;
    let model = paths::model_file(chosen_model(ui));
    let speed = UpdateSpeed::ALL[ui.get_speed_index() as usize];
    let threshold = 10_f32.powf(ui.get_threshold() / 20.0);
    let (weak, shared) = (ui.as_weak(), shared.clone());
    thread::spawn(move || {
        let status = || -> Box<dyn Fn(String) + Send> {
            let (weak, shared) = (weak.clone(), shared.clone());
            Box::new(move |text| {
                let shared = shared.clone();
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    if current(&shared) {
                        ui.set_status(text.into());
                    }
                });
            })
        };
        let reports = {
            let (on_update, update_shared) = (weak.clone(), shared.clone());
            Reports {
                update: Box::new(move |update| {
                    let shared = update_shared.clone();
                    let _ = on_update.upgrade_in_event_loop(move |ui| {
                        if current(&shared) {
                            receive(&ui, update);
                        }
                    });
                }),
                status: status(),
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
                status: status(),
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
        let opened = match recording {
            Some(path) => capture::replay(&path, threshold, sink),
            None => capture::live(threshold, sink),
        };
        match opened {
            Ok(capture) => {
                *shared
                    .session
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(Session { engine, capture });
                let _ = weak.upgrade_in_event_loop(|ui| {
                    ui.set_busy(false);
                    ui.set_listening(true);
                    ui.set_status("Listening".into());
                    show_overlay(&ui);
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
        set_placeholder(&ui, "");
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
    let weak = ui.as_weak();
    thread::spawn(move || {
        session.capture.stop();
        let update = session.engine.stop();
        let _ = weak.upgrade_in_event_loop(move |ui| {
            receive(&ui, update);
            ui.set_busy(false);
            ui.set_status("Stopped".into());
        });
    });
}

fn show_overlay(ui: &MainWindow) {
    let weak = ui.as_weak();
    let on_change: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(ui) = weak.upgrade() {
            save(&ui);
        }
    });
    with_view(|view| view.overlay.show(on_change));
    ui.set_overlay_shown(true);
}

fn hide_overlay(ui: &MainWindow) {
    with_view(|view| view.overlay.hide());
    ui.set_overlay_shown(false);
}

fn toggle_overlay(ui: &MainWindow) {
    if with_view(|view| view.overlay.is_visible()) {
        hide_overlay(ui);
    } else {
        show_overlay(ui);
    }
}

fn change_size(ui: &MainWindow, by: f32) {
    let size = (ui.get_caption_size() + by).clamp(SMALLEST, LARGEST);
    ui.set_caption_size(size);
    with_view(|view| view.overlay.window.set_size_points(size));
    save(ui);
}

fn toggle_expanded(ui: &MainWindow) {
    with_view(|view| {
        let line = (ui.get_caption_size() * 1.3).round();
        view.overlay.toggle_expanded(line);
        view.captions.set_expanded(view.overlay.is_expanded());
    });
}

fn set_click_through(ui: &MainWindow, on: bool) {
    ui.set_click_through(on);
    with_view(|view| view.overlay.set_click_through(on));
    save(ui);
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

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let mut replay = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--replay" => replay = arguments.next().map(PathBuf::from),
            "--capture-check" => {
                let seconds = arguments.next().and_then(|text| text.parse().ok());
                return check::run(seconds.unwrap_or(5.0));
            }
            _ => {}
        }
    }
    match run_app(replay) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run_app(replay: Option<PathBuf>) -> Result<(), slint::PlatformError> {
    let shared = Arc::new(Shared {
        replay,
        ..Shared::default()
    });
    let on_wayland = leave_wayland();
    let settings = Settings::load();
    overlay::prepare()?;
    let ui = MainWindow::new()?;
    let shortcuts = std::fs::read(paths::shortcuts_file())
        .ok()
        .and_then(|data| ShortcutMap::from_json(&data))
        .unwrap_or_else(|| KEYBOARD.default_shortcuts());
    let view = View {
        captions: Captions::new(!settings.highlight_words),
        overlay: Overlay::new(settings.overlay, settings.click_through)?,
        shortcuts: RefCell::new(shortcuts),
        hotkeys: (!on_wayland).then(|| RefCell::new(Hotkeys::new(firing(ui.as_weak())))),
        clashes: RefCell::new(Vec::new()),
    };
    ui.set_words(view.captions.model());
    view.overlay.window.set_words(view.captions.model());
    view.overlay.window.set_size_points(settings.caption_size);
    VIEW.with(|cell| cell.set(view).map_err(|_| ()).expect("one view"));
    connect_shortcuts(&ui, on_wayland);
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
    set_placeholder(&ui, "Your live captions appear here");
    ui.set_history(ModelRc::from(Rc::new(VecModel::<HistoryLine>::default())));
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
                with_view(|view| {
                    view.captions.set_plain(!ui.get_highlight());
                    view.overlay.set_click_through(ui.get_click_through());
                    view.overlay.window.set_size_points(ui.get_caption_size());
                });
                save(&ui);
            }
        }
    });
    ui.on_toggle_overlay({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                toggle_overlay(&ui);
            }
        }
    });
    connect_overlay(&ui);
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
    ui.on_export_history({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                export_history(&ui);
            }
        }
    });

    // The overlay would otherwise keep the app running after its main window closes.
    ui.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });

    #[cfg(feature = "snapshot")]
    snapshot::schedule(&ui);
    let ended = ui.run();
    // Recognition has to finish its pass, or the GPU library crashes while the app exits.
    let session = shared
        .session
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    if let Some(session) = session {
        session.capture.stop();
        session.engine.stop();
    }
    ended
}

/// An overlay needs to stay on top and sit where it's put, and Wayland lets no app do either. The
/// X11 layer that Wayland desktops carry does, so the app runs there. Returns whether this is a
/// Wayland desktop, because shortcuts have to go through the desktop all the same.
#[cfg(target_os = "linux")]
fn leave_wayland() -> bool {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|name| !name.is_empty());
    if wayland && std::env::var_os("DISPLAY").is_some() {
        // SAFETY: this is the first thing the app does, before any other thread exists.
        unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
    }
    wayland
}

#[cfg(not(target_os = "linux"))]
fn leave_wayland() -> bool {
    false
}

fn run_action(ui: &MainWindow, action: ShortcutAction) {
    match action {
        ShortcutAction::ToggleCaptions => ui.invoke_toggle_captions(),
        ShortcutAction::ToggleOverlay => toggle_overlay(ui),
        ShortcutAction::ToggleExpanded => {
            show_overlay(ui);
            toggle_expanded(ui);
        }
        ShortcutAction::LargerText => change_size(ui, SIZE_STEP),
        ShortcutAction::SmallerText => change_size(ui, -SIZE_STEP),
        ShortcutAction::ToggleClickThrough => {
            show_overlay(ui);
            set_click_through(ui, !ui.get_click_through());
        }
        ShortcutAction::ResetOverlay => {
            show_overlay(ui);
            with_view(|view| view.overlay.reset());
            with_view(|view| view.captions.set_expanded(false));
            save(ui);
        }
    }
}

/// Runs a shortcut's action from whichever thread heard the key.
fn firing(weak: Weak<MainWindow>) -> impl Fn(ShortcutAction) + Send + Sync + 'static {
    move |action| {
        let _ = weak.upgrade_in_event_loop(move |ui| run_action(&ui, action));
    }
}

/// Shows the shortcuts as they stand, saying which ones another app has taken.
fn show_shortcuts(ui: &MainWindow) {
    let rows: Vec<ShortcutRow> = with_view(|view| {
        let map = view.shortcuts.borrow();
        let clashes = view.clashes.borrow();
        ShortcutAction::ALL
            .iter()
            .map(|action| ShortcutRow {
                title: action.title().into(),
                keys: map
                    .get(*action)
                    .map_or("Not set".to_string(), |shortcut| shortcut.label(&KEYBOARD))
                    .into(),
                taken: clashes.contains(action),
            })
            .collect()
    });
    ui.set_shortcuts(ModelRc::new(VecModel::from(rows)));
}

fn bind_shortcuts(ui: &MainWindow) {
    with_view(|view| {
        if let Some(hotkeys) = &view.hotkeys {
            let mut hotkeys = hotkeys.borrow_mut();
            *view.clashes.borrow_mut() = hotkeys.bind(&view.shortcuts.borrow(), &KEYBOARD);
            if !hotkeys.is_working() {
                ui.set_shortcut_note("This desktop didn't allow global shortcuts.".into());
            }
        }
    });
    show_shortcuts(ui);
}

fn save_shortcuts(ui: &MainWindow) {
    let data = with_view(|view| view.shortcuts.borrow().encoded());
    let path = paths::shortcuts_file();
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    if let Err(error) = std::fs::write(path, data) {
        ui.set_shortcut_note(error.to_string().into());
    }
}

fn start_recording(ui: &MainWindow, row: i32) {
    with_view(|view| {
        if let Some(hotkeys) = &view.hotkeys {
            hotkeys.borrow_mut().pause();
        }
    });
    ui.set_shortcut_note("".into());
    ui.set_recording(row);
}

/// Takes the keys pressed while recording. Modifiers alone keep it waiting for the real key.
fn finish_recording(ui: &MainWindow, row: i32, key: &str, held: Held) {
    let Some(key) = hotkeys::key_name(key) else {
        return;
    };
    let action = ShortcutAction::ALL[row as usize];
    let note = match KEYBOARD.shortcut(&key, held) {
        None => Some(format!("{key} can't be used in a shortcut.")),
        Some(shortcut) if !shortcut.is_allowed(&KEYBOARD) => Some(format!(
            "Add Ctrl or Alt to {key}, or it couldn't be typed in other apps."
        )),
        Some(shortcut) => with_view(|view| {
            let mut map = view.shortcuts.borrow_mut();
            match map.action_using(&shortcut, Some(action)) {
                Some(other) => Some(format!("That's already {}.", other.title())),
                None => {
                    map.set(Some(shortcut), action);
                    None
                }
            }
        }),
    };
    ui.set_recording(-1);
    if let Some(note) = &note {
        ui.set_shortcut_note(note.into());
    } else {
        save_shortcuts(ui);
    }
    bind_shortcuts(ui);
}

/// Shows that the desktop has the keys, then what it chose. It may first ask in its own dialog.
#[cfg(target_os = "linux")]
fn connect_portal(ui: &MainWindow) {
    let weak = ui.as_weak();
    ui.set_shortcuts_editable(false);
    ui.set_shortcut_note("Your desktop manages these shortcuts.".into());
    let map = with_view(|view| view.shortcuts.borrow().clone());
    portal::connect(
        map,
        move |outcome| {
            let _ = weak.upgrade_in_event_loop(move |ui| show_portal(&ui, outcome));
        },
        firing(ui.as_weak()),
    );
}

#[cfg(target_os = "linux")]
fn show_portal(ui: &MainWindow, outcome: portal::Outcome) {
    match outcome {
        Ok(keys) => {
            let rows: Vec<ShortcutRow> = ShortcutAction::ALL
                .iter()
                .map(|action| ShortcutRow {
                    title: action.title().into(),
                    keys: keys
                        .iter()
                        .find(|(bound, _)| bound == action)
                        .map_or("Not set", |(_, keys)| keys.as_str())
                        .into(),
                    taken: false,
                })
                .collect();
            ui.set_shortcuts(ModelRc::new(VecModel::from(rows)));
        }
        Err(_) => ui.set_shortcut_note(
            "This desktop doesn't offer global shortcuts. The buttons on the overlay still work."
                .into(),
        ),
    }
}

#[cfg(not(target_os = "linux"))]
fn connect_portal(_ui: &MainWindow) {}

fn connect_shortcuts(ui: &MainWindow, on_wayland: bool) {
    if on_wayland {
        show_shortcuts(ui);
        connect_portal(ui);
    } else {
        bind_shortcuts(ui);
    }
    ui.on_record_shortcut({
        let weak = ui.as_weak();
        move |row| {
            if let Some(ui) = weak.upgrade() {
                start_recording(&ui, row);
            }
        }
    });
    ui.on_shortcut_pressed({
        let weak = ui.as_weak();
        move |row, key, control, alt, shift, system| {
            if let Some(ui) = weak.upgrade() {
                let held = Held {
                    control,
                    alt,
                    shift,
                    system,
                };
                finish_recording(&ui, row, &key, held);
            }
        }
    });
    ui.on_cancel_recording({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_recording(-1);
                bind_shortcuts(&ui);
            }
        }
    });
    ui.on_clear_shortcut({
        let weak = ui.as_weak();
        move |row| {
            if let Some(ui) = weak.upgrade() {
                let action = ShortcutAction::ALL[row as usize];
                with_view(|view| view.shortcuts.borrow_mut().set(None, action));
                save_shortcuts(&ui);
                bind_shortcuts(&ui);
            }
        }
    });
    ui.on_reset_shortcuts({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                with_view(|view| *view.shortcuts.borrow_mut() = KEYBOARD.default_shortcuts());
                ui.set_shortcut_note("".into());
                save_shortcuts(&ui);
                bind_shortcuts(&ui);
            }
        }
    });
}

fn export_history(ui: &MainWindow) {
    let lines = ui.get_history();
    let contents = export::text(&lines);
    let weak = ui.as_weak();
    let saving = async move {
        if let Err(error) = export::save(contents).await
            && let Some(ui) = weak.upgrade()
        {
            ui.set_status(error.into());
        }
    };
    let _ = slint::spawn_local(saving);
}

fn connect_overlay(ui: &MainWindow) {
    let handle = || ui.as_weak();
    let act = |weak: Weak<MainWindow>, action: fn(&MainWindow)| {
        move || {
            if let Some(ui) = weak.upgrade() {
                action(&ui);
            }
        }
    };
    with_view(|view| {
        let window = &view.overlay.window;
        window.on_smaller(act(handle(), |ui| change_size(ui, -SIZE_STEP)));
        window.on_larger(act(handle(), |ui| change_size(ui, SIZE_STEP)));
        window.on_toggle_expanded(act(handle(), toggle_expanded));
        window.on_let_clicks_through(act(handle(), |ui| set_click_through(ui, true)));
        window.on_turn_off_click_through(act(handle(), |ui| set_click_through(ui, false)));
        window.on_hide_overlay(act(handle(), hide_overlay));
        window.on_move_window(|| with_view(|view| view.overlay.start_move()));
        window.on_resize_window(|edges| {
            let edges = ResizeEdges::from_bits(edges as u8);
            with_view(|view| view.overlay.start_resize(edges));
        });
        window.on_edges_at(|x, y, width, height| {
            i32::from(ResizeZones::OVERLAY.edges(x, y, width, height).bits())
        });
    });
}

/// Saves pictures of the window, so the interface can be checked without a person.
/// `SOUNDVEIL_SNAPSHOT` names the file, and `SOUNDVEIL_SNAPSHOT_AT` is a list of seconds.
#[cfg(feature = "snapshot")]
mod snapshot {
    use super::*;
    use slint::{Timer, TimerMode};
    use std::time::Duration;

    fn act(window: &slint::Window, words: &[&str]) {
        use slint::LogicalPosition;
        use slint::platform::{Key, PointerEventButton, WindowEvent};
        let place = |x: &str, y: &str| {
            LogicalPosition::new(x.parse().unwrap_or(0.0), y.parse().unwrap_or(0.0))
        };
        match *words {
            ["point", x, y] => {
                let position = place(x, y);
                window.dispatch_event(WindowEvent::PointerMoved { position });
            }
            ["click", x, y] => {
                let position = place(x, y);
                let button = PointerEventButton::Left;
                window.dispatch_event(WindowEvent::PointerMoved { position });
                window.dispatch_event(WindowEvent::PointerPressed { position, button });
                window.dispatch_event(WindowEvent::PointerReleased { position, button });
            }
            ["key", name] => {
                let text: slint::SharedString = match name {
                    "F9" => Key::F9.into(),
                    "Control" => Key::Control.into(),
                    "Escape" => Key::Escape.into(),
                    text => text.into(),
                };
                window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
                window.dispatch_event(WindowEvent::KeyReleased { text });
            }
            _ => {}
        }
    }

    /// A step that starts with `overlay` goes to the overlay rather than the main window, and
    /// `wait 0.5` holds the steps after it back by that many seconds.
    fn perform(ui: &MainWindow, script: &str) {
        let mut steps = script.split(';');
        while let Some(step) = steps.next() {
            let words: Vec<&str> = step.split_whitespace().collect();
            match words[..] {
                ["wait", seconds] => {
                    let rest = steps.collect::<Vec<_>>().join(";");
                    let weak = ui.as_weak();
                    let wait = Duration::from_secs_f64(seconds.parse().unwrap_or(0.5));
                    Timer::single_shot(wait, move || {
                        if let Some(ui) = weak.upgrade() {
                            perform(&ui, &rest);
                        }
                    });
                    return;
                }
                ["overlay", ref rest @ ..] => {
                    with_view(|view| act(view.overlay.window.window(), rest));
                }
                ref all => act(ui.window(), all),
            }
        }
    }

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
        // Clicks and types as a person would: "click 577 576; key F9" a second after opening.
        if let Ok(script) = std::env::var("SOUNDVEIL_SCRIPT") {
            let weak = ui.as_weak();
            Timer::single_shot(Duration::from_secs(1), move || {
                if let Some(ui) = weak.upgrade() {
                    perform(&ui, &script);
                }
            });
        }
        if std::env::var_os("SOUNDVEIL_OVERLAY").is_some() {
            show_overlay(ui);
        }
        // Points at the overlay and optionally expands it, once the window exists.
        if let Ok(at) = std::env::var("SOUNDVEIL_HOVER_AT").map(|at| at.parse().unwrap_or(1.0)) {
            let weak = ui.as_weak();
            Timer::single_shot(Duration::from_secs_f64(at), move || {
                if std::env::var_os("SOUNDVEIL_EXPAND").is_some()
                    && let Some(ui) = weak.upgrade()
                {
                    toggle_expanded(&ui);
                }
                with_view(|view| {
                    view.overlay.window.window().dispatch_event(
                        slint::platform::WindowEvent::PointerMoved {
                            position: slint::LogicalPosition::new(300.0, 60.0),
                        },
                    );
                });
            });
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
                    // SOUNDVEIL_SHOT=overlay pictures the overlay rather than the main window.
                    let overlay = std::env::var("SOUNDVEIL_SHOT").is_ok_and(|v| v == "overlay");
                    let picture = if overlay {
                        with_view(|view| view.overlay.window.window().take_snapshot())
                    } else {
                        ui.window().take_snapshot()
                    };
                    let picture = picture.expect("window snapshot");
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
