//! The desktop's clipboard for the windows' text boxes. Slint asks the
//! platform to hold text that is copied or cut and for the text to paste;
//! these answer through Wayland's clipboard: copied text is offered to the
//! desktop at the next key press's serial (main.rs offers it), and what
//! another program copies is read in the background as it is offered, so a
//! paste has it at once.

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

/// Text copied or cut in a window.
pub fn copied(text: &str) {
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
