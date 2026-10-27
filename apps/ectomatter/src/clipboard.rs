//! The desktop's clipboard for the windows' text boxes. Slint asks the
//! platform to hold text that is copied or cut and for the text to paste;
//! in the Wayland shell these answer through Wayland's clipboard: copied
//! text is offered to the desktop at the next key press's serial (the shell
//! offers it), and what another program copies is read in the background as
//! it is offered, so a paste has it at once. A shell that hands the
//! clipboard to the system's own (`use_system`: the winit shell) has them
//! answered there, through arboard.

use std::cell::RefCell;
use std::io::Read;
use std::sync::Mutex;

/// The types of text Ectomatter offers and reads, the first the one it prefers.
pub const TEXT: [&str; 5] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain", "TEXT", "STRING"];

thread_local! {
    /// text copied in a window, not yet offered to the desktop
    static PENDING: RefCell<Option<String>> = const { RefCell::new(None) };
    /// the text Ectomatter holds on the desktop's clipboard, while it does
    static OURS: RefCell<Option<String>> = const { RefCell::new(None) };
}
/// the text another program holds on the clipboard, as last read
static THEIRS: Mutex<Option<String>> = Mutex::new(None);

/// The system's own clipboard, where the shell hands it there.
static SYSTEM: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

/// The clipboard is the system's own from here on (the winit shell's), or
/// none where the system has none Ectomatter can reach.
pub fn use_system() {
    match arboard::Clipboard::new() {
        Ok(c) => *SYSTEM.lock().unwrap_or_else(|p| p.into_inner()) = Some(c),
        Err(e) => eprintln!("ectomatter: no clipboard: {e}"),
    }
}

/// Text copied or cut in a window.
pub fn copied(text: &str) {
    if let Some(c) = SYSTEM.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
        if let Err(e) = c.set_text(text) {
            eprintln!("ectomatter: the text wasn’t copied: {e}");
        }
        return;
    }
    PENDING.with(|p| *p.borrow_mut() = Some(text.to_string()));
}

/// The text copied, to offer now, if there is any.
pub fn take_pending() -> Option<String> {
    PENDING.with(|p| p.borrow_mut().take())
}

/// Ectomatter holds this text on the clipboard now, or none.
pub fn hold(text: Option<String>) {
    OURS.with(|o| *o.borrow_mut() = text);
}

/// The text Ectomatter offers, to send to a program that asks for it.
pub fn ours() -> Option<String> {
    OURS.with(|o| o.borrow().clone())
}

/// The text to paste: Ectomatter's own while it holds the clipboard, or what
/// another program holds.
pub fn text() -> Option<String> {
    if let Some(c) = SYSTEM.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
        return c.get_text().ok();
    }
    ours().or_else(|| THEIRS.lock().ok().and_then(|t| t.clone()))
}

/// Read what another program offers, from the pipe it writes it into, in
/// the background; none where it offers no text.
pub fn read_theirs(pipe: Option<std::io::PipeReader>) {
    let Some(mut pipe) = pipe else {
        if let Ok(mut t) = THEIRS.lock() {
            *t = None;
        }
        return;
    };
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        // (a program that never closes its end gives up to 8 MB, then what it gave)
        let _ = pipe.by_ref().take(8 << 20).read_to_end(&mut bytes);
        if let Ok(mut t) = THEIRS.lock() {
            *t = Some(String::from_utf8_lossy(&bytes).into_owned());
        }
    });
}
