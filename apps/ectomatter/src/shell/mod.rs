//! The window's shell: what puts Ectomatter's frames on the screen and hands
//! it the person's input, apart from what it draws. Only the shell knows the
//! system's windowing (Wayland, today); the pages, the packages and what
//! they tell each other reach it only through what's here.

/// A sender to the shell's own loop: what a thread of Ectomatter's (a
/// device's, a package's, the tray's) tells the window, which wakes it.
pub use smithay_client_toolkit::reexports::calloop::channel::Sender;

/// A box of a picture, in its pixels: x, y, width and height.
pub type Rect = (usize, usize, usize, usize);

pub mod session;
pub mod wayland;
