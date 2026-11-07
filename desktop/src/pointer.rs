//! Where the pointer is on the whole screen, in the same physical pixels as a window's position.
//! A window that lets clicks through hears nothing from the mouse, so it has to ask.

use slint::winit_030::winit::dpi::PhysicalPosition;
use slint::winit_030::winit::window::Window;

#[cfg(windows)]
pub fn location(_window: &Window) -> Option<PhysicalPosition<f64>> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT::default();
    // SAFETY: the pointer is to a value that outlives the call.
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some(PhysicalPosition::new(point.x.into(), point.y.into()))
}

/// Nothing comes back on Wayland with no X11 layer, which keeps the pointer from other apps. Through
/// the X11 layer, the answer may go stale while the pointer is over a window that isn't using it.
#[cfg(target_os = "linux")]
pub fn location(_window: &Window) -> Option<PhysicalPosition<f64>> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt;
    use x11rb::rust_connection::RustConnection;

    thread_local! {
        // Kept, because this is asked ten times a second. winit's own connection is private.
        static SERVER: Option<(RustConnection, u32)> = x11rb::connect(None)
            .ok()
            .map(|(connection, screen)| {
                let root = connection.setup().roots[screen].root;
                (connection, root)
            });
    }
    SERVER.with(|server| {
        let (connection, root) = server.as_ref()?;
        let reply = connection.query_pointer(*root).ok()?.reply().ok()?;
        Some(PhysicalPosition::new(
            reply.root_x.into(),
            reply.root_y.into(),
        ))
    })
}

#[cfg(target_os = "macos")]
pub fn location(window: &Window) -> Option<PhysicalPosition<f64>> {
    use slint::winit_030::winit::dpi::LogicalPosition;
    use std::ffi::c_void;
    use std::ptr;

    #[repr(C)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreate(source: *const c_void) -> *mut c_void;
        fn CGEventGetLocation(event: *const c_void) -> Point;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(object: *const c_void);
    }

    // SAFETY: an empty source is allowed, the event is checked, and it's released once.
    let point = unsafe {
        let event = CGEventCreate(ptr::null());
        if event.is_null() {
            return None;
        }
        let point = CGEventGetLocation(event);
        CFRelease(event);
        point
    };
    Some(LogicalPosition::new(point.x, point.y).to_physical(window.scale_factor()))
}
