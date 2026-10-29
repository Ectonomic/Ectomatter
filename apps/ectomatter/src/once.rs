//! Ectomatter runs once for each person. Opened again (from the app menu, a
//! launcher or a terminal) while one runs, the new one asks the one running
//! to come to the person, with the activation token its launcher gave it, and
//! stops; started at login (`--hide`) while one runs, it stops without a word
//! to it. The one running is found on the session bus by the name it holds
//! there (front_app::SERVICE) and answers on an object of its own. Started
//! again by itself (`--after PID`, after a change to what it may use), it first
//! waits for the one before to go. With no session bus, and on other systems
//! for now (docs/deferred.md), nothing is asked and each one runs.

use crate::shell::Sender;
use crate::tray::TrayAsk;

/// How long a new Ectomatter waits for the one it was started again by to go.
#[cfg(target_os = "linux")]
const AFTER: std::time::Duration = std::time::Duration::from_secs(15);

/// Where the one running answers, on the name front_app holds.
#[cfg(target_os = "linux")]
const IFACE: &str = "com.ectonomic.Ectomatter.Window";

/// The one running, told to come to the person.
#[cfg(target_os = "linux")]
struct Window {
    tell: Sender<TrayAsk>,
}

#[cfg(target_os = "linux")]
#[zbus::interface(name = "com.ectonomic.Ectomatter.Window")]
impl Window {
    /// Come to the person, as a press on the tray's icon brings it: with the
    /// activation token the new one's launcher gave it (empty where none).
    fn come_forward(&self, token: String) {
        eprintln!("ectomatter: opened again, so this one comes forward");
        let _ = self.tell.send(TrayAsk::Open((!token.is_empty()).then_some(token)));
    }
}

/// The one running answers on `bus`, telling the window through `tell`.
#[cfg(target_os = "linux")]
pub fn answer(bus: &crate::front_app::Bus, tell: Sender<TrayAsk>) {
    if let Err(e) = bus.object_server().at(crate::front_app::PATH, Window { tell }) {
        eprintln!("ectomatter: opening Ectomatter again won't bring this one forward: {e}");
    }
}

/// Whether an Ectomatter runs already, so this one is to stop: the one
/// running asked to come forward first, unless this one starts at login
/// (`quiet`). `after`: the Ectomatter this one was started again by, waited for.
#[cfg(target_os = "linux")]
pub fn running_already(quiet: bool, after: Option<u32>) -> bool {
    if let Some(pid) = after {
        let start = std::time::Instant::now();
        while crate::computer::here(format!("/proc/{pid}")).exists() && start.elapsed() < AFTER {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    let Ok(conn) = zbus::blocking::Connection::session() else { return false };
    let Ok(bus) = zbus::blocking::fdo::DBusProxy::new(&conn) else { return false };
    let Ok(name) = zbus::names::BusName::try_from(crate::front_app::SERVICE) else { return false };
    if !bus.name_has_owner(name).unwrap_or(false) {
        return false;
    }
    if quiet {
        eprintln!("ectomatter: Ectomatter runs already, so this one stops");
        return true;
    }
    // (the token a launcher gives the program it starts: Wayland's, or X11's startup id)
    let token = std::env::var("XDG_ACTIVATION_TOKEN").or_else(|_| std::env::var("DESKTOP_STARTUP_ID")).unwrap_or_default();
    match conn.call_method(Some(crate::front_app::SERVICE), crate::front_app::PATH, Some(IFACE), "ComeForward", &(token,)) {
        Ok(_) => eprintln!("ectomatter: Ectomatter runs already; it was asked to come forward, and this one stops"),
        Err(e) => eprintln!("ectomatter: Ectomatter runs already, so this one stops (it couldn't be asked to come forward: {e})"),
    }
    true
}

#[cfg(not(target_os = "linux"))]
pub fn answer(_: &crate::front_app::Bus, _: Sender<TrayAsk>) {}

#[cfg(not(target_os = "linux"))]
pub fn running_already(_: bool, _: Option<u32>) -> bool {
    false
}
