//! The window's shell: what puts Ectomatter's frames on the screen and hands
//! it the person's input, apart from what it draws. Only the shell knows the
//! system's windowing (Wayland, today); the pages, the packages and what
//! they tell each other reach it only through what's here.

use std::sync::{Arc, mpsc};

/// What wakes the shell's loop: each shell gives its own (Wayland's a ping
/// on its loop, another an event sent to its loop), and every sender made
/// with it calls it after each message.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// A sender to the window: what a thread of Ectomatter's (a device's, a
/// package's, the tray's) tells it, which wakes the shell's loop to take it.
pub struct Sender<T> {
    tx: mpsc::Sender<T>,
    wake: Wake,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender { tx: self.tx.clone(), wake: self.wake.clone() }
    }
}

impl<T> Sender<T> {
    pub fn send(&self, t: T) -> Result<(), mpsc::SendError<T>> {
        self.tx.send(t)?;
        (self.wake)();
        Ok(())
    }
}

/// A sender to the window and what it sends, woken by `wake`.
pub fn channel<T>(wake: &Wake) -> (Sender<T>, mpsc::Receiver<T>) {
    let (tx, rx) = mpsc::channel();
    (Sender { tx, wake: wake.clone() }, rx)
}

/// A sender that wakes nothing, for a test that reads its receiver itself.
#[cfg(test)]
pub fn quiet_channel<T>() -> (Sender<T>, mpsc::Receiver<T>) {
    channel(&(Arc::new(|| {}) as Wake))
}

/// A key pressed or let go, as every shell tells it: the text Slint takes
/// for it (a named key's own character, slint::platform::Key, or what it
/// types; with Ctrl held, the key's own letter), its code as Linux's input
/// layer numbers it where the system gives one (evdev; 0 elsewhere), and the
/// modifiers held.
pub struct Press {
    pub text: Option<slint::SharedString>,
    pub code: u32,
    pub down: bool,
    pub ctrl: bool,
    pub shift: bool,
}

impl Press {
    /// Whether it's the key that types `c` (either case) or Slint's named key
    /// `c` (`Key::Escape.into()` and the like).
    pub fn is(&self, c: char) -> bool {
        self.text.as_ref().is_some_and(|t| {
            let mut chars = t.chars();
            chars.next().is_some_and(|k| k == c || k.to_lowercase().eq(c.to_lowercase())) && chars.next().is_none()
        })
    }

    pub fn is_key(&self, k: slint::platform::Key) -> bool {
        self.is(char::from(k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::platform::Key;

    fn press(t: &str) -> Press {
        Press { text: Some(t.into()), code: 0, down: true, ctrl: false, shift: false }
    }

    /// Every message wakes the shell's loop once it's sent, so the loop
    /// takes it; a sender whose loop has gone says so.
    #[test]
    fn a_message_wakes_the_loop() {
        let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let w = woken.clone();
        let wake: Wake = Arc::new(move || {
            w.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        let (tx, rx) = channel::<u32>(&wake);
        let other = tx.clone();
        std::thread::spawn(move || other.send(1).unwrap()).join().unwrap();
        tx.send(2).unwrap();
        assert_eq!(woken.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec![1, 2]);
        drop(rx);
        assert!(tx.send(3).is_err(), "the loop gone");
        assert_eq!(woken.load(std::sync::atomic::Ordering::SeqCst), 2, "nothing woken for a message not sent");
    }

    #[test]
    fn a_press_is_its_key_in_either_case_or_by_its_name() {
        assert!(press("k").is('k') && press("K").is('k') && press("Z").is('z'));
        assert!(!press("kk").is('k') && !press("j").is('k'));
        let esc: slint::SharedString = Key::Escape.into();
        assert!(press(&esc).is_key(Key::Escape) && !press(&esc).is_key(Key::Delete));
        assert!(!Press { text: None, code: 0, down: true, ctrl: false, shift: false }.is('k'));
    }
}

/// A box of a picture, in its pixels: x, y, width and height.
pub type Rect = (usize, usize, usize, usize);

pub mod session;
pub mod wheel;
#[cfg(target_os = "linux")]
pub mod wayland;
pub mod winit;
