//! The floating caption window. It stays above everything, never takes focus from what you're
//! watching, and can let clicks pass through to whatever is behind it.
//!
//! The drawing is Slint's, in `ui/overlay.slint`. What Slint has no words for, such as moving the
//! window by its background, goes straight to winit, which Slint runs on.

use crate::settings::Frame;
use crate::{CaptionOverlay, pointer};
use slint::winit_030::winit::dpi::PhysicalSize;
use slint::winit_030::winit::event::{
    ElementState, MouseButton, WindowEvent as NativeEvent,
};
use slint::winit_030::winit::window::{ResizeDirection, Window as NativeWindow};
use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
use slint::{ComponentHandle, LogicalPosition, LogicalSize, PhysicalPosition, Timer, TimerMode};
use soundveil_core::ResizeEdges;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

const WIDTH: f32 = 850.0;
const HEIGHT: f32 = 156.0;
const FROM_BOTTOM: f32 = 65.0;
const SAVE_AFTER: Duration = Duration::from_millis(500);
const WATCH_POINTER_EVERY: Duration = Duration::from_millis(100);

thread_local! {
    static CREATING: Cell<bool> = const { Cell::new(false) };
}

#[allow(unused_mut)]
fn native_attributes(
    mut attributes: winit::window::WindowAttributes,
) -> winit::window::WindowAttributes {
    #[cfg(windows)]
    {
        use winit::platform::windows::WindowAttributesExtWindows;
        attributes = attributes.with_skip_taskbar(true);
    }
    #[cfg(target_os = "linux")]
    {
        use winit::platform::x11::{WindowAttributesExtX11, WindowType};
        attributes = attributes.with_x11_window_type(vec![WindowType::Utility]);
    }
    attributes.with_active(false)
}

/// Picks the winit backend, which the overlay depends on. Call this before making any window.
pub fn prepare() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(|attributes| {
            if CREATING.get() {
                native_attributes(attributes)
            } else {
                attributes
            }
        })
        .select()
}

/// Keeps a click on the overlay from pulling focus away from the video.
#[cfg(windows)]
fn refuse_focus(window: &NativeWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let window = HWND(handle.hwnd.get() as *mut _);
    // SAFETY: the handle belongs to a window that is alive for the length of this call.
    unsafe {
        let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
        let extra = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
        SetWindowLongPtrW(window, GWL_EXSTYLE, style | extra);
    }
}

#[cfg(not(windows))]
fn refuse_focus(_window: &NativeWindow) {}

/// Where the pointer is on the window, in logical pixels from its top left.
fn place_on_window(
    pointer: winit::dpi::PhysicalPosition<f64>,
    window: PhysicalPosition,
    scale: f32,
) -> LogicalPosition {
    LogicalPosition::new(
        ((pointer.x - f64::from(window.x)) as f32) / scale,
        ((pointer.y - f64::from(window.y)) as f32) / scale,
    )
}

/// The one spot that still takes a click while clicks pass through, at the window's top right.
#[derive(Clone, Copy, Debug)]
struct Badge {
    width: f32,
    height: f32,
    margin: f32,
}

impl Badge {
    fn contains(&self, point: LogicalPosition, window_width: f32) -> bool {
        let left = window_width - self.margin - self.width;
        (left..left + self.width).contains(&point.x)
            && (self.margin..self.margin + self.height).contains(&point.y)
    }
}

/// The way a drag from these sides pulls the window. Nothing means the drag isn't a resize.
fn resize_direction(edges: ResizeEdges) -> Option<ResizeDirection> {
    let (west, east) = (
        edges.contains(ResizeEdges::LEFT),
        edges.contains(ResizeEdges::RIGHT),
    );
    let (north, south) = (
        edges.contains(ResizeEdges::TOP),
        edges.contains(ResizeEdges::BOTTOM),
    );
    Some(match (north, south, west, east) {
        (true, _, true, _) => ResizeDirection::NorthWest,
        (true, _, _, true) => ResizeDirection::NorthEast,
        (_, true, true, _) => ResizeDirection::SouthWest,
        (_, true, _, true) => ResizeDirection::SouthEast,
        (true, ..) => ResizeDirection::North,
        (_, true, ..) => ResizeDirection::South,
        (.., true, _) => ResizeDirection::West,
        (.., true) => ResizeDirection::East,
        _ => return None,
    })
}

/// Grows upward to a useful reading height, without taking most of the screen.
fn expanded_height(height: f32, line: f32, screen: f32) -> f32 {
    (screen * 0.6).min((height * 2.0).max(12.0 * line + 40.0))
}

/// The usual place: low on the screen and centred, clear of most video controls.
fn default_frame(screen: Screen, scale: f32) -> Frame {
    let (left, top) = (screen.x as f32, screen.y as f32);
    Frame {
        x: (left + (screen.width as f32 - WIDTH * scale) / 2.0) as i32,
        y: (top + screen.height as f32 - (HEIGHT + FROM_BOTTOM) * scale) as i32,
        width: WIDTH,
        height: HEIGHT,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Screen {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl Screen {
    fn contains_corner_of(&self, frame: &Frame) -> bool {
        let (x, y) = (frame.x + 40, frame.y + 20);
        x >= self.x
            && y >= self.y
            && x < self.x + self.width as i32
            && y < self.y + self.height as i32
    }
}

fn screens(window: &NativeWindow) -> Vec<Screen> {
    window
        .available_monitors()
        .map(|monitor| {
            let (place, PhysicalSize { width, height }) = (monitor.position(), monitor.size());
            Screen {
                x: place.x,
                y: place.y,
                width,
                height,
            }
        })
        .collect()
}

fn screen_under(window: &NativeWindow) -> Option<Screen> {
    let monitor = window.current_monitor().or(window.primary_monitor())?;
    let (place, size) = (monitor.position(), monitor.size());
    Some(Screen {
        x: place.x,
        y: place.y,
        width: size.width,
        height: size.height,
    })
}

pub struct Overlay {
    pub window: CaptionOverlay,
    frame: Cell<Option<Frame>>,
    click_through: Cell<bool>,
    expanded: Cell<bool>,
    compact_height: Cell<f32>,
    prepared: Cell<bool>,
    saving: Timer,
    /// Runs only while clicks pass through.
    watching: Timer,
    takes_clicks: Cell<bool>,
}

impl Overlay {
    pub fn new(frame: Option<Frame>, click_through: bool) -> Result<Rc<Self>, slint::PlatformError> {
        CREATING.set(true);
        let window = CaptionOverlay::new();
        CREATING.set(false);
        let window = window?;
        window.set_click_through(click_through);
        Ok(Rc::new(Self {
            window,
            frame: Cell::new(frame),
            click_through: Cell::new(click_through),
            expanded: Cell::new(false),
            compact_height: Cell::new(HEIGHT),
            prepared: Cell::new(false),
            saving: Timer::default(),
            watching: Timer::default(),
            takes_clicks: Cell::new(true),
        }))
    }

    pub fn frame(&self) -> Option<Frame> {
        self.frame.get()
    }

    pub fn is_visible(&self) -> bool {
        self.window.window().is_visible()
    }

    fn apply(&self, frame: Frame) {
        let window = self.window.window();
        window.set_position(PhysicalPosition::new(frame.x, frame.y));
        window.set_size(LogicalSize::new(frame.width, frame.height));
    }

    /// Reads where the window is now. Expanding is a temporary reading mode, so only the compact
    /// size and position are remembered.
    fn remember(&self, on_change: &dyn Fn()) {
        if self.expanded.get() || !self.is_visible() {
            return;
        }
        let window = self.window.window();
        let (place, scale) = (window.position(), window.scale_factor());
        let size = window.size().to_logical(scale);
        self.frame.set(Some(Frame {
            x: place.x,
            y: place.y,
            width: size.width,
            height: size.height,
        }));
        on_change();
    }

    fn with_native<T>(&self, change: impl FnOnce(&NativeWindow) -> T) -> Option<T> {
        self.window.window().with_winit_window(change)
    }

    /// Only a real window has a screen and a handle, so the rest of the setup waits for one.
    pub fn show(self: &Rc<Self>, on_change: Rc<dyn Fn()>) {
        if let Some(frame) = self.frame.get() {
            self.apply(frame);
        }
        if self.window.show().is_err() {
            return;
        }
        let (this, handle) = (self.clone(), self.window.as_weak());
        let setup = async move {
            let Some(window) = handle.upgrade() else {
                return;
            };
            let Ok(native) = window.window().winit_window().await else {
                return;
            };
            this.watch_pointer();
            let scale = native.scale_factor() as f32;
            let lost = this
                .frame
                .get()
                .is_some_and(|frame| !screens(&native).iter().any(|s| s.contains_corner_of(&frame)));
            if (this.frame.get().is_none() || lost)
                && let Some(screen) = screen_under(&native)
            {
                this.frame.set(Some(default_frame(screen, scale)));
                this.apply(default_frame(screen, scale));
            }
            if !this.prepared.replace(true) {
                let events = this.clone();
                let changed = on_change.clone();
                window.window().on_winit_window_event(move |_, event| {
                    match event {
                        NativeEvent::Moved(_) | NativeEvent::Resized(_) => {
                            let this = events.clone();
                            let changed = changed.clone();
                            events
                                .saving
                                .start(TimerMode::SingleShot, SAVE_AFTER, move || {
                                    this.remember(&*changed);
                                });
                        }
                        // Clicks only land on the badge now. Slint can't be trusted with this one,
                        // because it only knows where a pointer is from watching it move, and the
                        // pointer may have stopped before the badge began taking clicks.
                        NativeEvent::MouseInput {
                            state: ElementState::Pressed,
                            button: MouseButton::Left,
                            ..
                        } if events.click_through.get() => {
                            events.window.invoke_turn_off_click_through();
                            return EventResult::PreventDefault;
                        }
                        _ => {}
                    }
                    EventResult::Propagate
                });
            }
        };
        let _ = slint::spawn_local(setup);
    }

    pub fn hide(self: &Rc<Self>) {
        let _ = self.window.hide();
        self.watch_pointer();
    }

    pub fn set_click_through(self: &Rc<Self>, on: bool) {
        if self.click_through.replace(on) == on {
            return;
        }
        self.window.set_click_through(on);
        self.forget_pointer();
        self.watch_pointer();
    }

    /// winit rebuilds the window's styles here, which on Windows drops the ones that keep focus
    /// away, so they go back on after every change.
    fn take_clicks(&self, on: bool) {
        self.takes_clicks.set(on);
        self.with_native(|native| {
            native.set_cursor_hittest(on).ok();
            refuse_focus(native);
        });
    }

    /// A window that clicks pass through can't see the pointer either, so it's polled, and only
    /// while that's on and the window is showing.
    fn watch_pointer(self: &Rc<Self>) {
        let on = self.click_through.get();
        if self.is_visible() {
            self.take_clicks(!on);
        }
        if on && self.is_visible() {
            let this = Rc::downgrade(self);
            self.watching
                .start(TimerMode::Repeated, WATCH_POINTER_EVERY, move || {
                    if let Some(this) = this.upgrade() {
                        this.follow_pointer();
                    }
                });
            self.follow_pointer();
            return;
        }
        self.watching.stop();
        self.window.set_pointer_inside(false);
        // The pointer may be sitting on the window, so its controls come back without a nudge.
        if self.is_visible()
            && let Some(position) = self.pointer_on_window()
        {
            self.window
                .window()
                .dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
        }
    }

    fn pointer_on_window(&self) -> Option<LogicalPosition> {
        let window = self.window.window();
        let scale = window.scale_factor();
        let size = window.size().to_logical(scale);
        let pointer = self.with_native(pointer::location).flatten()?;
        let place = place_on_window(pointer, window.position(), scale);
        let inside = (0.0..size.width).contains(&place.x) && (0.0..size.height).contains(&place.y);
        inside.then_some(place)
    }

    /// Clicks land only while the pointer is on the badge, and pass through everywhere else.
    fn follow_pointer(&self) {
        let place = self.pointer_on_window();
        self.window.set_pointer_inside(place.is_some());
        let badge = Badge {
            width: self.window.get_badge_width(),
            height: self.window.get_badge_height(),
            margin: self.window.get_badge_margin(),
        };
        let scale = self.window.window().scale_factor();
        let width = self.window.window().size().to_logical(scale).width;
        let on_badge = place.is_some_and(|place| badge.contains(place, width));
        if on_badge != self.takes_clicks.get() {
            self.take_clicks(on_badge);
            if !on_badge {
                self.forget_pointer();
            }
        }
    }

    pub fn start_move(&self) {
        self.with_native(|native| native.drag_window().ok());
        self.forget_pointer();
    }

    /// winit can't do this on a Mac, so there the handles and cursors show but nothing moves.
    pub fn start_resize(&self, edges: ResizeEdges) {
        let Some(direction) = resize_direction(edges) else {
            return;
        };
        self.with_native(|native| native.drag_resize_window(direction).ok());
        self.forget_pointer();
    }

    /// The system takes over the pointer here, so Slint won't hear it released.
    fn forget_pointer(&self) {
        self.window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerExited);
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded.get()
    }

    /// Grows upward, so the newest line stays where the eye already is.
    pub fn toggle_expanded(&self, line: f32) {
        let window = self.window.window();
        let scale = window.scale_factor();
        let size = window.size().to_logical(scale);
        let place = window.position();
        let height = if self.expanded.get() {
            self.compact_height.get()
        } else {
            self.compact_height.set(size.height);
            let screen = self
                .with_native(screen_under)
                .flatten()
                .map_or(1080.0, |screen| screen.height as f32 / scale);
            expanded_height(size.height, line, screen)
        };
        let lifted = ((height - size.height) * scale) as i32;
        let top = self
            .with_native(screen_under)
            .flatten()
            .map_or(i32::MIN, |screen| screen.y);
        window.set_position(PhysicalPosition::new(place.x, (place.y - lifted).max(top)));
        window.set_size(LogicalSize::new(size.width, height));
        self.expanded.set(!self.expanded.get());
        self.window.set_expanded(self.expanded.get());
    }

    pub fn reset(&self) {
        if self.expanded.get() {
            self.expanded.set(false);
            self.window.set_expanded(false);
        }
        let scale = self.window.window().scale_factor();
        if let Some(screen) = self.with_native(screen_under).flatten() {
            let frame = default_frame(screen, scale);
            self.frame.set(Some(frame));
            self.apply(frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Screen = Screen {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };

    const BADGE: Badge = Badge {
        width: 182.0,
        height: 30.0,
        margin: 13.0,
    };

    #[test]
    fn the_badge_sits_at_the_top_right() {
        let point = |x, y| LogicalPosition::new(x, y);
        assert!(BADGE.contains(point(655.0, 13.0), 850.0));
        assert!(BADGE.contains(point(836.9, 42.9), 850.0));
    }

    #[test]
    fn clicks_pass_through_everywhere_around_the_badge() {
        let point = |x, y| LogicalPosition::new(x, y);
        assert!(!BADGE.contains(point(654.9, 20.0), 850.0));
        assert!(!BADGE.contains(point(837.0, 20.0), 850.0));
        assert!(!BADGE.contains(point(700.0, 12.9), 850.0));
        assert!(!BADGE.contains(point(700.0, 43.0), 850.0));
        assert!(!BADGE.contains(point(100.0, 100.0), 850.0));
    }

    #[test]
    fn the_badge_follows_the_right_edge_of_a_resized_window() {
        assert!(BADGE.contains(LogicalPosition::new(310.0, 20.0), 500.0));
        assert!(!BADGE.contains(LogicalPosition::new(655.0, 20.0), 500.0));
    }

    #[test]
    fn the_pointer_is_placed_in_logical_pixels_from_the_window() {
        let pointer = winit::dpi::PhysicalPosition::new(1300.0, 700.0);
        let place = place_on_window(pointer, PhysicalPosition::new(100, 100), 2.0);
        assert_eq!((place.x, place.y), (600.0, 300.0));
    }

    #[test]
    fn a_window_on_a_screen_left_of_the_main_one_is_placed_right() {
        let pointer = winit::dpi::PhysicalPosition::new(-1000.0, -200.0);
        let place = place_on_window(pointer, PhysicalPosition::new(-1500, -300), 1.0);
        assert_eq!((place.x, place.y), (500.0, 100.0));
    }

    #[test]
    fn each_side_and_corner_pulls_its_own_way() {
        use ResizeDirection::*;
        use ResizeEdges as Edges;
        let pairs = [
            (Edges::LEFT, West),
            (Edges::RIGHT, East),
            (Edges::TOP, North),
            (Edges::BOTTOM, South),
            (Edges::LEFT | Edges::TOP, NorthWest),
            (Edges::RIGHT | Edges::TOP, NorthEast),
            (Edges::LEFT | Edges::BOTTOM, SouthWest),
            (Edges::RIGHT | Edges::BOTTOM, SouthEast),
        ];
        for (edges, direction) in pairs {
            assert_eq!(resize_direction(edges), Some(direction), "{edges:?}");
        }
        assert_eq!(resize_direction(Edges::NONE), None);
    }

    #[test]
    fn expanding_reaches_a_useful_height() {
        assert_eq!(expanded_height(156.0, 36.0, 1080.0), 472.0);
        assert_eq!(expanded_height(300.0, 36.0, 1080.0), 600.0);
    }

    #[test]
    fn expanding_keeps_to_most_of_a_small_screen() {
        assert!((expanded_height(156.0, 62.0, 800.0) - 480.0).abs() < 0.01);
    }

    #[test]
    fn the_usual_place_is_low_and_centred() {
        let frame = default_frame(SCREEN, 1.0);
        assert_eq!((frame.x, frame.y), (535, 859));
    }

    #[test]
    fn the_usual_place_accounts_for_scaling_and_a_second_screen() {
        let right = Screen {
            x: 1920,
            y: -100,
            width: 2560,
            height: 1440,
        };
        let frame = default_frame(right, 1.5);
        assert_eq!((frame.x, frame.y), (2562, 1008));
        assert_eq!(frame.width, WIDTH);
    }

    #[test]
    fn a_window_left_on_an_unplugged_screen_is_noticed() {
        let frame = Frame {
            x: 2500,
            y: 300,
            width: WIDTH,
            height: HEIGHT,
        };
        assert!(!SCREEN.contains_corner_of(&frame));
        let frame = Frame { x: 300, ..frame };
        assert!(SCREEN.contains_corner_of(&frame));
    }
}
