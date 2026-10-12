//! Slint's software window, as MinimalSoftwareWindow is, that also keeps the
//! mouse cursor Slint asks for: the one of the area under the pointer, so the
//! hand shows exactly over what can be clicked, and the resize arrows over what
//! is dragged to a size. Slint hands the cursor to a window only through its
//! internal adapter hook, which this pins to Slint 1.18.1.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use i_slint_core::cursor::MouseCursorInner;
use i_slint_core::items::BuiltInMouseCursor;
use i_slint_core::window::WindowAdapterInternal;
use slint::platform::software_renderer::{RepaintBufferType, SoftwareRenderer};
use slint::platform::{Renderer, WindowAdapter, WindowEvent};
use slint::{PhysicalSize, Window, WindowSize};
use smithay_client_toolkit::seat::pointer::CursorIcon;

pub struct SoftWindow {
    window: Window,
    renderer: SoftwareRenderer,
    needs_redraw: Cell<bool>,
    size: Cell<PhysicalSize>,
    cursor: RefCell<MouseCursorInner>,
}

impl SoftWindow {
    pub fn new(repaint: RepaintBufferType) -> Rc<SoftWindow> {
        Rc::new_cyclic(|w: &Weak<SoftWindow>| SoftWindow {
            window: Window::new(w.clone()),
            renderer: SoftwareRenderer::new_with_repaint_buffer_type(repaint),
            needs_redraw: Cell::new(false),
            size: Cell::new(PhysicalSize::default()),
            cursor: RefCell::new(MouseCursorInner::default()),
        })
    }

    /// Draw, if anything has changed since the last time.
    pub fn draw_if_needed(&self, render: impl FnOnce(&SoftwareRenderer)) -> bool {
        if self.needs_redraw.replace(false) {
            render(&self.renderer);
            true
        } else {
            false
        }
    }

    /// Resize, as the window's own set_size does (the adapter's takes only a WindowSize).
    pub fn set_size(&self, size: impl Into<WindowSize>) {
        self.window.set_size(size);
    }

    /// The cursor for where the pointer is, as the compositor names it: every one
    /// of Slint's own cursors has the same name there (one drawn from an image, and
    /// none at all, show as the default arrow).
    pub fn pointer(&self) -> CursorIcon {
        use BuiltInMouseCursor as B;
        let MouseCursorInner::BuiltIn(c) = &*self.cursor.borrow() else { return CursorIcon::Default };
        match c {
            B::Help => CursorIcon::Help,
            B::Pointer => CursorIcon::Pointer,
            B::Progress => CursorIcon::Progress,
            B::Wait => CursorIcon::Wait,
            B::Crosshair => CursorIcon::Crosshair,
            B::Text => CursorIcon::Text,
            B::Alias => CursorIcon::Alias,
            B::Copy => CursorIcon::Copy,
            B::Move => CursorIcon::Move,
            B::NoDrop => CursorIcon::NoDrop,
            B::NotAllowed => CursorIcon::NotAllowed,
            B::Grab => CursorIcon::Grab,
            B::Grabbing => CursorIcon::Grabbing,
            B::ColResize => CursorIcon::ColResize,
            B::RowResize => CursorIcon::RowResize,
            B::NResize => CursorIcon::NResize,
            B::EResize => CursorIcon::EResize,
            B::SResize => CursorIcon::SResize,
            B::WResize => CursorIcon::WResize,
            B::NeResize => CursorIcon::NeResize,
            B::NwResize => CursorIcon::NwResize,
            B::SeResize => CursorIcon::SeResize,
            B::SwResize => CursorIcon::SwResize,
            B::EwResize => CursorIcon::EwResize,
            B::NsResize => CursorIcon::NsResize,
            B::NeswResize => CursorIcon::NeswResize,
            B::NwseResize => CursorIcon::NwseResize,
            _ => CursorIcon::Default,
        }
    }
}

impl WindowAdapter for SoftWindow {
    fn window(&self) -> &Window {
        &self.window
    }
    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }
    fn size(&self) -> PhysicalSize {
        self.size.get()
    }
    fn set_size(&self, size: WindowSize) {
        let sf = self.window.scale_factor();
        self.size.set(size.to_physical(sf));
        self.window.dispatch_event(WindowEvent::Resized { size: size.to_logical(sf) });
    }
    fn request_redraw(&self) {
        self.needs_redraw.set(true);
    }
    fn internal(&self, _: i_slint_core::InternalToken) -> Option<&dyn WindowAdapterInternal> {
        Some(self)
    }
}

impl WindowAdapterInternal for SoftWindow {
    fn set_mouse_cursor(&self, cursor: MouseCursorInner) {
        *self.cursor.borrow_mut() = cursor;
    }
}

impl std::ops::Deref for SoftWindow {
    type Target = Window;
    fn deref(&self) -> &Window {
        &self.window
    }
}
