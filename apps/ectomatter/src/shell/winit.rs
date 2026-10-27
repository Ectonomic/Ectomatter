//! The winit shell: Ectomatter's main window on any system winit runs on,
//! its frames presented through wgpu in the best color space the window's
//! surface offers (HDR10, scRGB, or SDR: crates/ectomatter-present), the
//! person's pointer and keys handed to the pages. The frames are composed
//! as the Wayland shell's are (shell::session); only how they reach the
//! screen differs.
//!
//! Started with `ECTOMATTER_SHELL=winit` on Linux, where the Wayland shell
//! is the one used (it describes the picture's HDR to the compositor
//! itself, which a Vulkan swapchain there can't yet: docs/deferred.md).
//! The windows of their own (the Macro Editor, the lab, the screen layout
//! editor), the clipboard and drags aren't in this shell yet.

use std::sync::Arc;

use ectomatter_present::{Presenter, Target, wgpu};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent as Event};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WKey, ModifiersState, NamedKey};
use winit::window::{ResizeDirection, Window, WindowId};

use super::session::{self, Session};
use crate::*;

/// The window and what draws into it, once the system has given it.
struct Shown {
    window: Arc<Window>,
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    presenter: Presenter,
}

struct App {
    s: Session,
    shown: Option<Shown>,
    /// the pointer's place in the window's logical pixels, and the modifiers held
    at: (f64, f64),
    mods: ModifiersState,
    /// the desktop's white, for SDR and for the frame's encoding; the
    /// screen's peak, for how bright the keys' light may go; and the nits
    /// at scRGB's 1.0 (80 on Windows, the screen's white on macOS)
    reference: f64,
    peak: f64,
    unit: f32,
    exit: bool,
    /// something came in since the last frame (input, a wake): draw again
    poke: bool,
    /// a redraw asked of the system and not yet given, and whether the window
    /// is out of sight (minimized, covered): neither waits for a frame then
    redraw_asked: bool,
    occluded: bool,
    /// ECTOMATTER_FPS: frames a second and the slowest, counted since when
    fps: Option<(Instant, u32, Duration)>,
}

/// How far in from the window's edge a press resizes it, in logical pixels.
const RESIZE_GRAB: f64 = 6.0;

pub(crate) fn run(slint: Rc<MinimalSoftwareWindow>, ui: AppWindow, settings: settings::Settings, reference: f64, gain: f32) {
    let event_loop = EventLoop::<()>::with_user_event().build().expect("a winit event loop");
    let proxy = event_loop.create_proxy();
    let wake: shell::Wake = Arc::new(move || {
        let _ = proxy.send_event(());
    });
    let session = Session::start(slint, ui, settings, reference, gain, &wake);
    let mut app = App { s: session, shown: None, at: (0.0, 0.0), mods: ModifiersState::default(), reference, peak: 1000.0, unit: 80.0, exit: false, poke: true, redraw_asked: false, occluded: false, fps: std::env::var_os("ECTOMATTER_FPS").map(|_| (Instant::now(), 0, Duration::ZERO)) };
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("ectomatter: the window's loop stopped: {e}");
    }
    app.s.finish();
}

impl App {
    fn scale(&self) -> f64 {
        self.shown.as_ref().map_or(1.0, |s| s.window.scale_factor())
    }

    /// The window made, and wgpu on it: the surface configured for the best
    /// target it offers.
    fn show(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        let attrs = Window::default_attributes()
            .with_title("Ectomatter")
            .with_decorations(false)
            .with_inner_size(LogicalSize::new(SIZE.0, SIZE.1))
            .with_min_inner_size(LogicalSize::new(MIN_SIZE.0, MIN_SIZE.1));
        let window = Arc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
        let adapter = ectomatter_present::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })).map_err(|e| e.to_string())?;
        let (device, queue) = ectomatter_present::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(&adapter);
        // the screen as the system tells it (Windows and macOS do; Wayland
        // doesn't, and the levels Ectomatter read from it stand)
        let screen = ectomatter_present::Screen::of(&surface.display_hdr_info(&adapter));
        if let Some(white) = screen.white {
            self.reference = white as f64;
            self.s.pic.set_reference(self.reference);
        }
        if let Some(peak) = screen.peak {
            self.peak = peak as f64;
        }
        self.unit = if cfg!(target_os = "macos") { self.reference as f32 } else { 80.0 };
        let (target, format) = ectomatter_present::best_target(&caps, screen.hdr).ok_or("the window's surface offers no format to draw in")?;
        let color_space = if target == Target::Sdr { wgpu::SurfaceColorSpace::Auto } else { target.surface().1 };
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        eprintln!("ectomatter: the window shows through wgpu on {} ({:?}), as {target:?} in {format:?}; the screen's white {:.0} nits, its peak {:.0}", adapter.get_info().name, adapter.get_info().backend, self.reference, self.peak);
        let presenter = Presenter::new(&device, format, target);
        self.shown = Some(Shown { window, instance, surface, device, queue, config, presenter });
        self.resized();
        Ok(())
    }

    /// The window at its size now: the pages laid out for it, drawn whole.
    fn resized(&mut self) {
        let Some(sh) = &mut self.shown else { return };
        let size = sh.window.inner_size();
        // (minimized, as Windows has it: nothing to lay out for)
        if size.width == 0 || size.height == 0 {
            return;
        }
        let scale = sh.window.scale_factor();
        sh.config.width = size.width.max(1);
        sh.config.height = size.height.max(1);
        sh.surface.configure(&sh.device, &sh.config);
        self.s.slint.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: scale as f32 });
        self.s.slint.set_size(PhysicalSize::new(size.width.max(1), size.height.max(1)));
        self.s.full = true;
        self.s.front.ui.set_is_maximized(sh.window.is_maximized());
        sh.window.request_redraw();
    }

    /// The next frame composed and shown. The surface's texture is taken
    /// first: where none can be had now (the window out of sight, the
    /// surface out of date or lost), the next frame is drawn whole.
    fn draw(&mut self) {
        self.redraw_asked = false;
        let scale = self.scale() as f32;
        let Some(sh) = &mut self.shown else { return };
        let size = sh.window.inner_size();
        let (w, h) = (size.width as usize, size.height as usize);
        if w == 0 || h == 0 || self.occluded {
            return;
        }
        let frame = match sh.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Lost => {
                // (a surface lost is made anew on the same window)
                match sh.instance.create_surface(sh.window.clone()) {
                    Ok(surface) => {
                        surface.configure(&sh.device, &sh.config);
                        sh.surface = surface;
                    }
                    Err(e) => eprintln!("ectomatter: the window's surface was lost and can't be made again: {e}"),
                }
                self.s.full = true;
                self.poke = true;
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                sh.surface.configure(&sh.device, &sh.config);
                self.s.full = true;
                self.poke = true;
                return;
            }
            _ => {
                self.s.full = true;
                self.poke = true;
                return;
            }
        };
        let t = Instant::now();
        // (nothing changed: the frame as last uploaded, drawn again)
        if let Some(dirty) = self.s.compose(scale, Some((w, h)), self.peak, self.peak) {
            let boxes: Vec<(u32, u32, u32, u32)> = dirty.iter().map(|&(x, y, bw, bh)| (x as u32, y as u32, bw as u32, bh as u32)).collect();
            sh.presenter.upload(&sh.device, &sh.queue, (w as u32, h as u32), &self.s.pic.out, &boxes);
        }
        let view = frame.texture.create_view(&Default::default());
        let mut enc = sh.device.create_command_encoder(&Default::default());
        sh.presenter.draw(&sh.queue, &mut enc, &view, self.reference as f32, self.unit);
        sh.queue.submit([enc.finish()]);
        sh.window.pre_present_notify();
        sh.queue.present(frame);
        self.s.frames += 1;
        // ECTOMATTER_DUMP: the frame as composed, as the Wayland shell saves it
        if let Some(dir) = &self.s.dump {
            let _ = std::fs::write(dir.join("frame.raw"), bytemuck::cast_slice::<[u16; 4], u8>(&self.s.pic.out));
            let _ = std::fs::write(dir.join("frame.txt"), format!("{w} {h} {} {}\n", self.s.frames, self.s.pic.reference()));
        }
        if let Some((since, n, slow)) = &mut self.fps {
            *n += 1;
            *slow = (*slow).max(t.elapsed());
            if since.elapsed() >= Duration::from_secs(1) {
                eprintln!("ectomatter: {n} frames a second, the slowest drawn in {slow:?}");
                (*since, *n, *slow) = (Instant::now(), 0, Duration::ZERO);
            }
        }
    }

    /// Where a press resizes the window from, if it's at an edge: not while
    /// it's maximized.
    fn edge(&self, (x, y): (f64, f64)) -> Option<ResizeDirection> {
        let sh = self.shown.as_ref()?;
        if sh.window.is_maximized() {
            return None;
        }
        let size = sh.window.inner_size().to_logical::<f64>(sh.window.scale_factor());
        let (l, r, t, b) = (x < RESIZE_GRAB, x > size.width - RESIZE_GRAB, y < RESIZE_GRAB, y > size.height - RESIZE_GRAB);
        Some(match (l, r, t, b) {
            (true, _, true, _) => ResizeDirection::NorthWest,
            (_, true, true, _) => ResizeDirection::NorthEast,
            (true, _, _, true) => ResizeDirection::SouthWest,
            (_, true, _, true) => ResizeDirection::SouthEast,
            (true, ..) => ResizeDirection::West,
            (_, true, ..) => ResizeDirection::East,
            (_, _, true, _) => ResizeDirection::North,
            (_, _, _, true) => ResizeDirection::South,
            _ => return None,
        })
    }

    /// What the pages ask of the window: moved, minimized, maximized, closed.
    fn act(&mut self) {
        let asks = self.s.front.act();
        let Some(sh) = &self.shown else { return };
        for ask in &asks {
            match ask {
                WindowAsk::Move if !asks.contains(&WindowAsk::Maximize) => {
                    let _ = sh.window.drag_window();
                    // (the system keeps the button's release: tell the page it is up)
                    let position = LogicalPosition::new(self.at.0 as f32, self.at.1 as f32);
                    self.s.slint.dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
                    self.s.slint.dispatch_event(WindowEvent::PointerExited);
                }
                WindowAsk::Move => {}
                WindowAsk::Minimize => sh.window.set_minimized(true),
                WindowAsk::Maximize => sh.window.set_maximized(!sh.window.is_maximized()),
                WindowAsk::Close => {
                    // (no tray to close into in this shell yet: closing quits)
                    self.s.quit();
                }
                WindowAsk::OpenMacros(..) | WindowAsk::OpenLab | WindowAsk::OpenLayouts(_) => {
                    eprintln!("ectomatter: the windows of their own aren't in the winit shell yet");
                }
                WindowAsk::MacroPlays(n, how) => self.s.front.macro_plays(*n, *how),
                WindowAsk::MacroGone(n) => self.s.front.macro_gone(*n),
                WindowAsk::MacroSaved(n) => self.s.front.macro_saved(*n),
                WindowAsk::ShowSettings => {
                    sh.window.focus_window();
                    self.s.front.show_sensors();
                }
            }
        }
    }
}

/// A key as Slint takes it: a named key's own character, else what it types
/// (with Ctrl held, the key's own letter, as a box takes Ctrl+C).
fn key_text(key: &WKey, without_mods: &WKey) -> Option<SharedString> {
    let k = match key {
        WKey::Named(n) => match n {
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Tab => Key::Tab,
            NamedKey::Enter => Key::Return,
            NamedKey::Escape => Key::Escape,
            NamedKey::Delete => Key::Delete,
            NamedKey::ArrowLeft => Key::LeftArrow,
            NamedKey::ArrowRight => Key::RightArrow,
            NamedKey::ArrowUp => Key::UpArrow,
            NamedKey::ArrowDown => Key::DownArrow,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::Shift => Key::Shift,
            NamedKey::Control => Key::Control,
            NamedKey::Alt => Key::Alt,
            NamedKey::Super => Key::Meta,
            NamedKey::Space => return Some(" ".into()),
            _ => return None,
        },
        WKey::Character(t) => {
            let own = Some(t.as_str()).filter(|t| !t.chars().any(char::is_control)).map(String::from);
            let t = own.or_else(|| match without_mods {
                WKey::Character(t) => Some(t.to_string()),
                _ => None,
            })?;
            return (!t.is_empty()).then(|| t.into());
        }
        _ => return None,
    };
    Some(k.into())
}

/// A key's code as Linux's input layer numbers it (evdev), on every system:
/// how Ectomatter names a key it's told to assign or record (keyboard.rs,
/// `evdev_to_hid`). 0 for one it has no number for.
fn key_code(physical: &winit::keyboard::PhysicalKey) -> u32 {
    use winit::keyboard::{KeyCode as K, PhysicalKey};
    let PhysicalKey::Code(k) = physical else { return 0 };
    let letters = [K::KeyQ, K::KeyW, K::KeyE, K::KeyR, K::KeyT, K::KeyY, K::KeyU, K::KeyI, K::KeyO, K::KeyP];
    if let Some(i) = letters.iter().position(|l| l == k) {
        return 16 + i as u32;
    }
    let letters = [K::KeyA, K::KeyS, K::KeyD, K::KeyF, K::KeyG, K::KeyH, K::KeyJ, K::KeyK, K::KeyL];
    if let Some(i) = letters.iter().position(|l| l == k) {
        return 30 + i as u32;
    }
    let letters = [K::KeyZ, K::KeyX, K::KeyC, K::KeyV, K::KeyB, K::KeyN, K::KeyM];
    if let Some(i) = letters.iter().position(|l| l == k) {
        return 44 + i as u32;
    }
    let digits = [K::Digit1, K::Digit2, K::Digit3, K::Digit4, K::Digit5, K::Digit6, K::Digit7, K::Digit8, K::Digit9, K::Digit0];
    if let Some(i) = digits.iter().position(|d| d == k) {
        return 2 + i as u32;
    }
    let f = [K::F1, K::F2, K::F3, K::F4, K::F5, K::F6, K::F7, K::F8, K::F9, K::F10];
    if let Some(i) = f.iter().position(|d| d == k) {
        return 59 + i as u32;
    }
    let f = [K::F13, K::F14, K::F15, K::F16, K::F17, K::F18, K::F19, K::F20, K::F21, K::F22, K::F23, K::F24];
    if let Some(i) = f.iter().position(|d| d == k) {
        return 183 + i as u32;
    }
    match k {
        K::Escape => 1,
        K::Minus => 12,
        K::Equal => 13,
        K::Backspace => 14,
        K::Tab => 15,
        K::BracketLeft => 26,
        K::BracketRight => 27,
        K::Enter => 28,
        K::ControlLeft => 29,
        K::Semicolon => 39,
        K::Quote => 40,
        K::Backquote => 41,
        K::ShiftLeft => 42,
        K::Backslash => 43,
        K::Comma => 51,
        K::Period => 52,
        K::Slash => 53,
        K::ShiftRight => 54,
        K::NumpadMultiply => 55,
        K::AltLeft => 56,
        K::Space => 57,
        K::CapsLock => 58,
        K::NumLock => 69,
        K::ScrollLock => 70,
        K::Numpad7 => 71,
        K::Numpad8 => 72,
        K::Numpad9 => 73,
        K::NumpadSubtract => 74,
        K::Numpad4 => 75,
        K::Numpad5 => 76,
        K::Numpad6 => 77,
        K::NumpadAdd => 78,
        K::Numpad1 => 79,
        K::Numpad2 => 80,
        K::Numpad3 => 81,
        K::Numpad0 => 82,
        K::NumpadDecimal => 83,
        K::IntlBackslash => 86,
        K::F11 => 87,
        K::F12 => 88,
        K::NumpadEnter => 96,
        K::ControlRight => 97,
        K::NumpadDivide => 98,
        K::PrintScreen => 99,
        K::AltRight => 100,
        K::Home => 102,
        K::ArrowUp => 103,
        K::PageUp => 104,
        K::ArrowLeft => 105,
        K::ArrowRight => 106,
        K::End => 107,
        K::ArrowDown => 108,
        K::PageDown => 109,
        K::Insert => 110,
        K::Delete => 111,
        K::AudioVolumeMute => 113,
        K::AudioVolumeDown => 114,
        K::AudioVolumeUp => 115,
        K::Pause => 119,
        K::SuperLeft => 125,
        K::SuperRight => 126,
        K::ContextMenu => 127,
        K::MediaTrackNext => 163,
        K::MediaPlayPause => 164,
        K::MediaTrackPrevious => 165,
        K::MediaStop => 166,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{KeyCode as K, PhysicalKey};

    /// A key's code is the one Linux's input layer gives it, so "Press a
    /// key" finds the same key on every system (keyboard.rs, `evdev_to_hid`).
    #[test]
    fn a_keys_code_is_linuxs() {
        let code = |k| key_code(&PhysicalKey::Code(k));
        assert_eq!([code(K::Escape), code(K::KeyQ), code(K::KeyA), code(K::KeyZ), code(K::KeyM), code(K::Digit1), code(K::Digit0), code(K::F1), code(K::F12), code(K::F13), code(K::SuperLeft)], [1, 16, 30, 44, 50, 2, 11, 59, 88, 183, 125]);
        for k in [K::KeyA, K::Digit5, K::F5, K::Enter, K::ShiftLeft, K::ArrowUp] {
            assert!(crate::keyboard::evdev_to_hid(code(k)).is_some(), "{k:?} is a key the keyboard knows");
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.shown.is_none() {
            if let Err(e) = self.show(el) {
                eprintln!("ectomatter: the window can't be shown through wgpu: {e}");
                el.exit();
            }
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, (): ()) {
        self.poke = true;
        // (the tray's own asks are the window's: shown, a device's page opened)
        for a in self.s.take(self.scale() as f32) {
            if let tray::TrayAsk::Device(d) = a {
                self.s.front.ui.set_place(d.into());
            }
            if let Some(sh) = &self.shown {
                sh.window.set_visible(true);
                sh.window.focus_window();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: Event) {
        let scale = self.scale();
        if !matches!(event, Event::RedrawRequested) {
            self.poke = true;
        }
        match event {
            Event::CloseRequested => self.s.quit(),
            Event::Resized(_) | Event::ScaleFactorChanged { .. } => self.resized(),
            Event::Occluded(o) => {
                self.occluded = o;
                // (out of sight, the screen plays again rather than show the keyboard's own)
                self.s.front.set_hidden(o);
                if !o {
                    self.s.full = true;
                }
            }
            Event::RedrawRequested => self.draw(),
            Event::CursorMoved { position, .. } => {
                let p: winit::dpi::LogicalPosition<f64> = position.to_logical(scale);
                self.at = (p.x, p.y);
                self.s.slint.dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(p.x as f32, p.y as f32) });
                let icon = match self.edge(self.at) {
                    Some(ResizeDirection::NorthWest) => winit::window::CursorIcon::NwResize,
                    Some(ResizeDirection::NorthEast) => winit::window::CursorIcon::NeResize,
                    Some(ResizeDirection::SouthWest) => winit::window::CursorIcon::SwResize,
                    Some(ResizeDirection::SouthEast) => winit::window::CursorIcon::SeResize,
                    Some(ResizeDirection::West) => winit::window::CursorIcon::WResize,
                    Some(ResizeDirection::East) => winit::window::CursorIcon::EResize,
                    Some(ResizeDirection::North) => winit::window::CursorIcon::NResize,
                    Some(ResizeDirection::South) => winit::window::CursorIcon::SResize,
                    None => self.s.slint.pointer(),
                };
                if let Some(sh) = &self.shown {
                    sh.window.set_cursor(icon);
                }
            }
            Event::CursorLeft { .. } => self.s.slint.dispatch_event(WindowEvent::PointerExited),
            Event::MouseInput { state, button, .. } => {
                let position = LogicalPosition::new(self.at.0 as f32, self.at.1 as f32);
                let b = match button {
                    MouseButton::Left => PointerEventButton::Left,
                    MouseButton::Right => PointerEventButton::Right,
                    MouseButton::Middle => PointerEventButton::Middle,
                    MouseButton::Back => PointerEventButton::Back,
                    MouseButton::Forward => PointerEventButton::Forward,
                    MouseButton::Other(_) => PointerEventButton::Other,
                };
                match state {
                    ElementState::Pressed => {
                        if b == PointerEventButton::Left {
                            if let (Some(dir), Some(sh)) = (self.edge(self.at), &self.shown) {
                                let _ = sh.window.drag_resize_window(dir);
                                return;
                            }
                        }
                        self.s.front.pressed_at(self.at.0 as f32, self.at.1 as f32);
                        self.s.slint.dispatch_event(WindowEvent::PointerPressed { position, button: b });
                    }
                    ElementState::Released => {
                        self.s.slint.dispatch_event(WindowEvent::PointerReleased { position, button: b });
                        self.s.slint.dispatch_event(WindowEvent::PointerMoved { position });
                    }
                }
            }
            Event::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 40.0, y * 40.0),
                    MouseScrollDelta::PixelDelta(PhysicalPosition { x, y }) => ((x / scale) as f32, (y / scale) as f32),
                };
                let position = LogicalPosition::new(self.at.0 as f32, self.at.1 as f32);
                self.s.slint.dispatch_event(WindowEvent::PointerScrolled { position, delta_x: dx, delta_y: dy });
            }
            Event::ModifiersChanged(m) => self.mods = m.state(),
            Event::KeyboardInput { event, .. } => {
                use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
                let p = shell::Press {
                    text: key_text(&event.logical_key, &event.key_without_modifiers()),
                    code: key_code(&event.physical_key),
                    down: event.state == ElementState::Pressed,
                    ctrl: self.mods.control_key(),
                    shift: self.mods.shift_key(),
                };
                self.s.key(p);
            }
            _ => {}
        }
        if self.exit {
            el.exit();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if let session::Step::Stop = self.s.step() {
            self.exit = true;
            el.exit();
            return;
        }
        slint::platform::update_timers_and_animations();
        self.act();
        // a lighting effect's next frame, 30 a second; Slint's timers; or
        // what Ectomatter waits for. Out of sight, or with a redraw already
        // asked for and not yet given, neither an animation nor an effect is
        // waited on: the system gives the frame when it can.
        let shows = !self.occluded && !self.redraw_asked;
        let effect = (self.s.front.animated() && shows).then(|| self.s.effect_due.saturating_duration_since(Instant::now()));
        let animating = self.s.slint.has_active_animations() && shows;
        // drawn again only for what may have changed: input or a wake, the
        // page asking to be, an animation running, an effect's frame due
        let due = self.poke || self.s.slint.pending() || animating || effect.is_some_and(|e| e.is_zero());
        self.poke = false;
        if let (true, false, Some(sh)) = (due, self.redraw_asked, &self.shown) {
            sh.window.request_redraw();
            self.redraw_asked = true;
        }
        let wait = if animating {
            Some(Duration::ZERO)
        } else {
            [slint::platform::duration_until_next_timer_update(), self.s.front.waiting(), effect].into_iter().flatten().min()
        };
        // (at least once a second, so a stop the system asked for, which only
        // sets a flag, is acted on)
        let wait = wait.unwrap_or(Duration::from_secs(1)).min(Duration::from_secs(1));
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + wait));
    }
}
