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

/// A box of a picture, in its pixels: x, y, width and height.
pub type Rect = (usize, usize, usize, usize);

pub mod session;
pub mod wayland;
