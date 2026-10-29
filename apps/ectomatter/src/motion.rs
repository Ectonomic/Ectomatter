//! Whether the person has asked their system for less motion, so what moves on its own
//! (About's Blob) holds still. On Linux, the desktop portal's `reduced-motion` setting
//! (org.freedesktop.appearance, xdg-desktop-portal 1.21 and on), which KDE and GNOME back
//! with their own animation switches: 1 asks for less motion. Asked away from the window's
//! thread, as it answers over D-Bus; until it answers, or where nothing says, things move.

use std::sync::atomic::{AtomicBool, Ordering};

static REDUCED: AtomicBool = AtomicBool::new(false);

/// Whether things should hold still, as last heard.
pub fn reduced() -> bool {
    REDUCED.load(Ordering::Relaxed)
}

/// Asks the system again, away from the window's thread.
pub fn look() {
    std::thread::spawn(|| {
        if let Some(r) = ask() {
            REDUCED.store(r, Ordering::Relaxed);
        }
    });
}

#[cfg(target_os = "linux")]
fn ask() -> Option<bool> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let settings = zbus::blocking::Proxy::new(&conn, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Settings").ok()?;
    let v: zbus::zvariant::OwnedValue = settings.call("ReadOne", &("org.freedesktop.appearance", "reduced-motion")).ok()?;
    Some(said(&v)?)
}

/// What the portal's answer says: 1 asks for less motion, 0 has no preference.
#[cfg(target_os = "linux")]
fn said(v: &zbus::zvariant::OwnedValue) -> Option<bool> {
    let n = u32::try_from(v).ok().or_else(|| {
        // (an older portal wraps the value in one more variant)
        let inner: zbus::zvariant::Value = v.downcast_ref().ok()?;
        u32::try_from(&inner).ok()
    })?;
    Some(n == 1)
}

// (Windows' Animation effects switch and macOS's Reduce motion aren't read yet:
// docs/deferred.md)
#[cfg(not(target_os = "linux"))]
fn ask() -> Option<bool> {
    None
}
