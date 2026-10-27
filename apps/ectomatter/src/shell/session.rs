//! What Ectomatter does in its main window apart from how the window is
//! shown (`Session`), for every shell: the pages, the picture they're
//! composed into, the keyboards drawn for the page and for Home, their light
//! and its simulation, the pictures of it kept, the tray, and the lab's
//! tuning. A shell hands it the window's size and scale and the screen's
//! levels, and shows the boxes it composes.

use crate::*;
use super::Rect;

/// Settled pictures of the keys' light, the last few kept, so going back to
/// one (an undo, Hold to compare, a choice taken back) shows at once rather
/// than settling again: each is about 27 MB. The saved tuning's stays.
#[derive(Default)]
pub(crate) struct Kept {
    pictures: Vec<std::sync::Arc<Snapshot>>,
    pinned: Option<Setup>,
}

impl Kept {
    const MOST: usize = 4;

    /// Whether a picture is of this setup, as the simulation runs it.
    pub(crate) fn of(snap: &Snapshot, setup: &Setup) -> bool {
        snap.setup.same_paths(&setup.simulated())
    }

    pub(crate) fn get(&self, want: &Setup) -> Option<std::sync::Arc<Snapshot>> {
        self.pictures.iter().rev().find(|s| Kept::of(s, want)).cloned()
    }

    /// Whether the keyboard takes the simulation's new picture. The
    /// simulation may still be on a make asked of it before the one wanted
    /// (a slider dragged): its picture is newer than the one shown, so it is
    /// taken, but never over a picture of the one wanted.
    pub(crate) fn takes(want: Option<&Setup>, shown: Option<&Snapshot>, snap: &Snapshot) -> bool {
        want.is_none_or(|w| Kept::of(snap, w) || !shown.is_some_and(|s| Kept::of(s, w)))
    }

    /// Keep a settled picture, the newest last; past the most kept, the oldest
    /// goes, but never the saved tuning's.
    pub(crate) fn keep(&mut self, snap: std::sync::Arc<Snapshot>) {
        self.pictures.retain(|s| !s.setup.same_paths(&snap.setup));
        self.pictures.push(snap);
        while self.pictures.len() > Kept::MOST {
            let pinned = &self.pinned;
            let Some(i) = self.pictures.iter().position(|s| !pinned.as_ref().is_some_and(|w| Kept::of(s, w))) else { break };
            self.pictures.remove(i);
        }
    }
}

/// What Ectomatter is doing in its main window, apart from how the window is
/// shown: the pages (`Front`), the picture they're composed into, the
/// keyboards drawn for the page and for Home, their light and its simulation,
/// the tray, and the lab's tuning. A shell holds one, hands it the window's
/// size, and shows what it composes.
pub(crate) struct Session {
    /// what Ectomatter's threads tell the window, taken as the shell is woken
    pub(crate) inbox: Inbox,
    /// KWin's script, which tells Ectomatter the app in front, and when it was last looked at
    pub(crate) front_bus: Option<front_app::Bus>,
    pub(crate) kwin_looked: Instant,
    /// quitting asked for, when: Ectomatter goes once what changed is saved
    pub(crate) quitting: Option<Instant>,
    /// the display's drawing the keyboard was last lit with
    pub(crate) panel_rev: u64,

    pub(crate) slint: Rc<MinimalSoftwareWindow>,
    pub(crate) front: Front,
    pub(crate) pic: Picture,
    pub(crate) gain: f32,
    /// the keyboard at the size last drawn, and its light
    pub(crate) board: Option<Board>,
    pub(crate) lights: Vec<Light>,
    pub(crate) lit: Option<Rc<Lit>>,
    /// the last frame's keyboard picture, taken back once nothing shows it,
    /// for the next to be drawn into rather than made anew
    pub(crate) spare: Vec<f32>,
    /// the keys of the area the Lighting page has the pointer over, which the rest of the picture dims around
    pub(crate) dim: Option<Vec<[f32; 4]>>,
    /// when a lighting effect's next frame is due, at the rate Settings ›
    /// Performance asks for
    pub(crate) effect_due: Instant,
    pub(crate) asked: Option<(u32, u32)>,
    pub(crate) build: std::sync::mpsc::Sender<(u32, u32, serde_json::Value, f32)>,
    /// the keyboard drawn for Home's desk, on its ground: the size last asked
    /// for, its builder, and how much of the ground shows in each of its pixels;
    /// and whether the picture shown is it
    pub(crate) home_board: Option<Board>,
    pub(crate) home_asked: Option<(u32, u32)>,
    pub(crate) home_build: std::sync::mpsc::Sender<(u32, u32, serde_json::Value, f32)>,
    pub(crate) home_share: Option<Rc<Vec<f32>>>,
    pub(crate) lit_home: bool,
    /// the Sensei 310's core (kept so it keeps running)
    pub(crate) _mouse: Option<core_host::Core>,
    /// legends to set as the light asks, away from the window
    pub(crate) set_legends: std::sync::mpsc::Sender<ectomatter_draw::hdr::LegendsAsk>,
    /// the whole window needs drawing again, not only what Slint changed
    pub(crate) full: bool,
    /// ECTOMATTER_DUMP: a folder to save the frames given to the compositor in
    pub(crate) dump: Option<PathBuf>,
    /// ECTOMATTER_CHECK: encode every frame whole too, and say if the
    /// parts encoded differ from it
    pub(crate) check: bool,
    pub(crate) frames: u64,
    pub(crate) tray: Option<tray::Handle>,
    /// the devices the tray lists, as last told it
    pub(crate) tray_devices: Vec<(String, String, bool)>,
    /// the scenes the tray's Scene menu lists (each one's id and name) and the one on
    pub(crate) tray_scenes: (Vec<(String, String)>, Option<String>),
    /// the light the lab has tuned, which the keyboard is lit by, and
    /// its change (and whether Hold to compare was held for it)
    pub(crate) light: Option<serde_json::Value>,
    pub(crate) light_rev: (u64, bool),
    pub(crate) readout_at: Option<Instant>,
    /// the keys' light, simulated from the first keyboard drawn on, its way
    /// of saying it has a clearer picture, and the picture last taken
    pub(crate) sim: Option<Simulator>,
    pub(crate) sim_tx: shell::Sender<()>,
    pub(crate) sim_seen: u64,
    /// the simulation the keyboard is to show (how every key is lit), and
    /// the last few settled pictures, the saved tuning's kept among them
    pub(crate) want: Option<Setup>,
    pub(crate) kept: Kept,
}

impl Session {
    /// The light the keyboard is lit by: the lab's, if it has tuned
    /// one, or that of the setup in use (the core's, or one of the person's own).
    pub(crate) fn light_now(&self) -> serde_json::Value {
        self.light.clone().or_else(|| cores::keyboard().map(|c| c.device_now()["light"].clone())).unwrap_or_default()
    }

    /// Tell the lab what is on the keyboard now, in whole nits, at
    /// most four times a second (an effect changes it every frame).
    pub(crate) fn readout(&mut self) {
        if self.readout_at.is_some_and(|t| t.elapsed() < Duration::from_millis(250)) {
            return;
        }
        // (the mouse's lab: its lights' brightest now)
        if self.front.lab.as_ref().is_some_and(|l| l.windows()) {
            self.readout_at = Some(Instant::now());
            let (lights, reference) = (self.front.mouse_brightest(), self.pic.reference());
            let peak = (self.gain as f64 * reference).round();
            if let Some(l) = &mut self.front.lab {
                l.set_readout(lab::Readout { peak, white: reference.round(), lights, settled: true, ..Default::default() });
            }
            return;
        }
        let (Some(l), Some(b)) = (self.front.lab.as_mut().filter(|l| !l.windows()), &self.board) else { return };
        self.readout_at = Some(Instant::now());
        let reference = self.pic.reference();
        let p = b.peaks(&self.lights, reference as f32, self.gain);
        let (photons, settled) = b.sim_progress().unwrap_or((0, false));
        l.set_readout(lab::Readout {
            legend: (p.legend as f64).round(),
            reach: (p.clean as f64).round(),
            deck: tenth(p.deck),
            skirt: tenth(p.skirt),
            chassis: tenth(p.chassis),
            lcd: (b.lcd_nits() as f64).min(self.gain as f64 * reference).round(),
            peak: (self.gain as f64 * reference).round(),
            white: reference.round(),
            photons,
            settled,
            lights: [0.0; 2],
        });
    }

    /// Whether the picture the window shows is Home's keyboard: Home is up
    /// with a keyboard on its desk.
    pub(crate) fn shows_home(&self) -> bool {
        self.front.desk_keyboard().is_some()
    }

    /// The keyboard the window shows lit again: the page's, or Home's on the
    /// desk (dark while it isn't plugged in).
    pub(crate) fn relight(&mut self) {
        let home = self.shows_home();
        let board = if home { self.home_board.as_ref() } else { self.board.as_ref() };
        let Some(b) = board else {
            if home {
                self.lit = None;
                self.lit_home = true;
            }
            return;
        };
        let lights = if home && !self.front.desk_keyboard_on() { &[][..] } else { &self.lights[..] };
        let px = light(b, lights, self.front.page.locks, self.gain, self.pic.reference(), self.front.overlays.panel_for(b.w, b.lcd().dot));
        self.lit = Some(Rc::new(Lit { px, w: b.w as usize, h: b.h as usize }));
        self.lit_home = home;
    }

    /// How long between a lighting effect's frames: 30 a second, which
    /// shows an effect smoothly and keeps Ectomatter light on the computer
    /// (the keyboard plays the effect itself; the window only copies it).
    pub(crate) fn effect_frame(&self) -> Duration {
        Duration::from_secs_f64(1.0 / 30.0)
    }

    /// The keyboard came back drawn at a size, for its page or (`home`) for
    /// Home's desk: light it, by the lab's light if it has tuned one.
    pub(crate) fn built(&mut self, home: bool, mut board: Board, scale: f32) {
        // its shadow round it, on the box it sits in or on the desk
        if let Some(s) = &board.cast {
            let (img, m) = (shadow_image(s, board.w, board.h), s.margin as f32 / scale);
            if home {
                self.front.ui.set_desk_kb_shadow(img);
                self.front.ui.set_desk_kb_shadow_m(m);
            } else {
                self.front.ui.set_board_shadow(img);
                self.front.ui.set_board_shadow_margin(m);
            }
        }
        // the lab, open before the device was first drawn, takes its caps
        if let (Some(l), Some(core)) = (self.front.lab.as_mut().filter(|l| !l.has_caps() && !home && !l.windows()), cores::keyboard()) {
            l.set_caps(lab_device(core, Some(&board)).caps);
        }
        if let Some(l) = &self.light {
            board.set_light(l);
        }
        let now = self.light_now();
        self.front.light_caps(&now);
        // (drawn from the core as it was read: the legends the light asks for now)
        if let Some(ask) = board.legends_for(&self.light_now()) {
            let _ = self.set_legends.send(ask);
        }
        // The keys' light, simulated from the first keyboard drawn on, for
        // every size (the simulation is in the art's units). Its first
        // picture, the light straight from the LEDs, is waited for, a tenth
        // of a second or so, so the keys don't show dark first.
        if self.sim.is_none() {
            let tx = std::sync::Mutex::new(self.sim_tx.clone());
            let s = Simulator::new(Scene::of(&board), board.setup(&self.light_now()), Box::new(move || {
                let _ = tx.lock().map(|t| t.send(()));
            }));
            let t = Instant::now();
            s.wait(Duration::from_millis(1500));
            eprintln!("ectomatter: the keys' light simulated, its first picture in {:?}", t.elapsed());
            self.sim = Some(s);
        }
        let want = board.setup(&self.light_now());
        if let Some(snap) = self.kept.get(&want) {
            board.set_sim(snap);
        } else if let Some(s) = &self.sim {
            // (the simulation's latest, though it may be of the make before:
            // the keyboard drawn new has none to keep instead)
            s.set(&want);
            if let Some(snap) = s.latest() {
                self.sim_seen = s.version();
                board.set_sim(snap);
            }
        }
        self.want = Some(want);
        if home {
            self.home_share = Some(Rc::new(board.ground_share()));
            self.home_board = Some(board);
        } else {
            self.board = Some(board);
        }
        self.relight();
        self.full = true;
    }

    /// Home's keyboard, drawn for its size on the desk once the desk has
    /// settled, ahead of Home's opening too, no larger than its page's (closer,
    /// its last picture is stretched).
    pub(crate) fn ask_home_board(&mut self, scale: f32) {
        let Some((kw, kh)) = self.front.desk_keyboard_size().filter(|_| self.front.desk_settled()) else { return };
        let s = scale;
        let page = (self.front.ui.get_board_width() * s).round().max(1.0);
        // (its place's size in pixels as keyboard_place rounds it, so the
        // picture fits it exactly)
        let (pw, ph) = ((kw * s).round().max(1.0), (kh * s).round().max(1.0));
        let (hw, hh) = if pw <= page { (pw as u32, ph as u32) } else { (page as u32, (page * kh / kw.max(1.0)).round().max(1.0) as u32) };
        if self.home_board.as_ref().map(|b| (b.w, b.h)) != Some((hw, hh)) && self.home_asked != Some((hw, hh)) {
            self.home_asked = Some((hw, hh));
            let _ = self.home_build.send((hw, hh, self.light_now(), s));
        }
    }

    /// Legends set as the light asks: put them on the keyboard, and light it again.
    pub(crate) fn legends_ready(&mut self, legends: ectomatter_draw::hdr::Legends) {
        // (set for one size: the keyboard of that size takes them)
        let light = self.light_now();
        // (each that takes them: the page's and Home's may be drawn at one size)
        for board in [self.board.as_mut(), self.home_board.as_mut()].into_iter().flatten() {
            if board.takes(&legends) {
                board.set_legends(legends.clone());
                // (the light may have changed again while they were set)
                if let Some(ask) = board.legends_for(&light) {
                    let _ = self.set_legends.send(ask);
                }
            }
        }
        self.relight();
        self.full = true;
    }

    /// The simulation has a clearer picture of the keys' light: light the keyboard by it.
    /// Whether the window is to be drawn again for it.
    pub(crate) fn sim_ready(&mut self) -> bool {
        let Some(s) = &self.sim else { return false };
        let version = s.version();
        let Some(snap) = s.latest() else { return false };
        if version == self.sim_seen || (self.board.is_none() && self.home_board.is_none()) {
            return false;
        }
        self.sim_seen = version;
        // (each keyboard, the page's and the desk's, lit by the same simulation)
        let mut taken = false;
        for board in [self.board.as_mut(), self.home_board.as_mut()].into_iter().flatten() {
            if Kept::takes(self.want.as_ref(), board.sim(), &snap) {
                board.set_sim(snap.clone());
                taken = true;
            }
        }
        if !taken {
            return false;
        }
        if snap.settled {
            self.kept.keep(snap.clone());
        }
        self.relight();
        self.readout_at = None;
        self.readout();
        true
    }

    /// The window's next frame composed into the picture, for a window
    /// `scale` pixels to the point: the boxes that changed, or none where
    /// nothing did or the window can't take a frame now (`size` none: hidden,
    /// not shown yet, or the last frame not taken). The screen's peak and the
    /// brightest screen's are the shell's to say.
    pub(crate) fn compose(&mut self, scale: f32, size: Option<(usize, usize)>, screen_peak: f64, top_peak: f64) -> Option<Vec<Rect>> {
        // the keys' light as bright as Settings › HDR lets it go on this screen, and
        // Peak nits' slider from this screen's white to the brightest screen's peak
        self.front.screen_peak = screen_peak;
        self.front.levels = (self.pic.reference(), top_peak);
        let g = self.front.light_gain(self.pic.reference());
        if (g - self.gain).abs() > 1e-3 {
            self.gain = g;
            self.relight();
        }
        // (the tray's devices, as they sit on Home's desk, each told whether it is there)
        if let Some(t) = &self.tray {
            let devices: Vec<(String, String, bool)> = self.front.devices_in_order().into_iter().map(|(place, name, _, on)| (place, name, on)).collect();
            if devices != self.tray_devices {
                self.tray_devices = devices.clone();
                t.devices(devices);
            }
            let scenes = (self.front.scenes().into_iter().map(|s| (s.id, s.name)).collect::<Vec<_>>(), self.front.scene_id());
            if scenes != self.tray_scenes {
                self.tray_scenes = scenes.clone();
                t.scenes(scenes.0, scenes.1);
            }
        }
        // (asked for whether or not this frame is drawn: a window out of sight
        // gets no frames, and the keyboard should be ready when Home opens)
        self.ask_home_board(scale);
        let (w, h) = size?;
        if self.pic.w != w || self.pic.h != h {
            self.pic.resize(w, h);
            self.full = true;
        }
        // where the keyboard shows: in its page's box, or on Home's desk (nothing
        // there till Home's keyboard is drawn)
        let s = scale;
        let home = self.front.desk_keyboard();
        let rect = keyboard_place(&self.front, self.home_board.is_some(), s);
        // the knobs and the display's glass for the keyboard's size as drawn,
        // before the page is drawn, so they move with it in the same frame; on
        // the desk, shown at its size there
        let drawn = home.and_then(|_| self.home_board.as_ref().map(|b| (b.w, b.h))).unwrap_or((rect.2 as u32, rect.3 as u32));
        self.front.board_size(drawn.0, drawn.1, s);
        if home.is_some() && drawn.0 > 0 {
            self.front.ui.set_desk_kb_k(rect.2 as f32 / drawn.0 as f32);
        }
        // the lab's tuning, when it has changed
        let mut relit = false;
        let mut relit_caps = false;
        // (the mouse takes the lab's tuning of its lights itself)
        if let Some(l) = self.front.lab.as_ref().filter(|l| l.windows() && (l.rev, l.comparing) != self.light_rev) {
            self.light_rev = (l.rev, l.comparing);
            let shown = l.shown().clone();
            self.front.set_mouse_light(shown);
        }
        if let Some(lab) = self.front.lab.as_ref().filter(|l| !l.windows()) {
            let rev = (lab.rev, lab.comparing);
            if rev != self.light_rev {
                self.light_rev = rev;
                self.light = Some(lab.shown().clone());
                // (both keyboards, the page's and the desk's)
                if let Some(l) = &self.light {
                    for b in [self.board.as_mut(), self.home_board.as_mut()].into_iter().flatten() {
                        // the keys drawn again only if their picture changes now; how
                        // the light falls shows with the simulation's next picture,
                        // or at once from a picture kept of it
                        relit |= b.set_light(l);
                        // the legends, as the light asks for them, when they are set
                        if let Some(ask) = b.legends_for(l) {
                            let _ = self.set_legends.send(ask);
                        }
                    }
                }
                let setups = match (self.board.as_ref().or(self.home_board.as_ref()), &self.light) {
                    (Some(b), Some(l)) => Some((b.setup(l), b.setup(lab.saved()))),
                    _ => None,
                };
                if let Some((want, pinned)) = setups {
                    self.kept.pinned = Some(pinned);
                    let changed = self.want.as_ref().is_none_or(|w| !w.same_paths(&want));
                    if changed {
                        if let Some(snap) = self.kept.get(&want) {
                            for b in [self.board.as_mut(), self.home_board.as_mut()].into_iter().flatten() {
                                b.set_sim(snap.clone());
                            }
                            relit = true;
                            if let Some(s) = &self.sim {
                                s.rest();
                            }
                        } else if let Some(s) = &self.sim {
                            s.set(&want);
                        }
                    }
                    self.want = Some(want);
                }
                self.readout_at = None;
                // (the caps' colors, for what is printed over the keys)
                relit_caps = true;
            }
        }
        if relit_caps {
            let now = self.light_now();
            self.front.light_caps(&now);
            self.front.refresh();
        }
        // the keys' light, or the display's text, when it has changed
        if let Some(b) = if home.is_some() { self.home_board.as_ref() } else { self.board.as_ref() } {
            let _ = self.front.overlays.panel_for(b.w, b.lcd().dot);
            // the glass's sheen as the core sets it
            let glass = b.lcd().glass;
            if (glass - self.front.overlays.glass_opacity).abs() > 1e-4 {
                self.front.overlays.glass_opacity = glass;
                self.front.show_overlays();
            }
        }
        let panel_moved = self.front.overlays.panel_rev != self.panel_rev || relit;
        self.panel_rev = self.front.overlays.panel_rev;
        // a lighting effect's next frame only when it is due, 30 a second;
        // anything else as it changes
        let effect = self.front.animated();
        let due = !effect || Instant::now() >= self.effect_due;
        let lights = if due { self.front.lights() } else { None };
        if effect && lights.is_some() {
            self.effect_due = Instant::now() + self.effect_frame();
        }
        // the last frame's picture, kept until the picture shows the new one
        let mut shown_before = None;
        // the area the pointer is over on the Lighting page: the rest of the keyboard dims
        let dim = if home.is_none() { self.board.as_ref().and_then(|b| self.front.dim_rects(b.w)) } else { None };
        let dim_moved = dim != self.dim;
        // (Home's keyboard, or the page's again, in place of the other)
        let switched = self.lit_home != home.is_some();
        if lights.is_some() || panel_moved || dim_moved || switched {
            if let Some(l) = lights {
                self.lights = l;
            }
            self.dim = dim;
            let on_home = home.is_some();
            match if on_home { self.home_board.as_ref() } else { self.board.as_ref() } {
                Some(board) => {
                    let mut px = std::mem::take(&mut self.spare);
                    // (a keyboard not plugged in shows dark on the desk)
                    let lights = if on_home && !self.front.desk_keyboard_on() { &[][..] } else { &self.lights[..] };
                    light_into(&mut px, board, lights, self.front.page.locks, self.gain, self.pic.reference(), self.front.overlays.panel_for(board.w, board.lcd().dot));
                    if let Some(rects) = &self.dim {
                        dim_around(&mut px, board.w as usize, rects, board.chassis());
                    }
                    shown_before = self.lit.replace(Rc::new(Lit { px, w: board.w as usize, h: board.h as usize }));
                }
                None if on_home => shown_before = self.lit.take(),
                None => {}
            }
            self.lit_home = on_home;
            self.readout();
        }
        let full = self.full;
        let mut dirty = Vec::new();
        let pic = &mut self.pic;
        self.slint.draw_if_needed(|r| {
            if full {
                r.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            }
            let region = r.render(&mut pic.ui, w);
            if full {
                r.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
            }
            // the whole of what Slint changed: its list of rectangles has been
            // seen to leave out pixels it drew (the title bar's buttons)
            let (o, sz) = (region.bounding_box_origin(), region.bounding_box_size());
            if sz.width > 0 && sz.height > 0 {
                dirty.push((o.x.max(0) as usize, o.y.max(0) as usize, sz.width as usize, sz.height as usize));
            }
        });

        if lay_ground(&mut self.pic, &self.front.ui, scale) {
            self.full = true;
        }
        // the page's keyboard drawn for the window's size even while another
        // page shows, so the keyboard is there, whole, the moment its page opens
        let ui = &self.front.ui;
        let want = ((ui.get_board_width() * s).round().max(1.0) as u32, (ui.get_board_height() * s).round().max(1.0) as u32);
        if want.0 > 1 && self.board.as_ref().map(|b| (b.w, b.h)) != Some(want) && self.asked != Some(want) {
            self.asked = Some(want);
            let _ = self.build.send((want.0, want.1, self.light_now(), scale));
        }
        // (the knobs and glass for the keyboard as drawn, as above)
        self.front.board_size(drawn.0, drawn.1, s);
        self.front.ui.set_board_ready(self.lit.is_some());
        // the keyboard where it shows, in its page's box or on Home's desk (the
        // one drawn at its size there, or the last one stretched to it while that
        // size is drawn), and what Home's desk lays on the ground round it and under it
        let share = if home.is_some() { self.home_share.clone() } else { None };
        dirty.extend(lay_keyboard(&mut self.pic, &self.front, self.home_board.is_some(), self.lit.as_ref(), share, s));
        // the mouse in its page's hole, lit as it shows now (and the lab tuning it told)
        let laid = lay_mouse(&mut self.pic, &mut self.front, s);
        if !laid.is_empty() && self.front.lab.as_ref().is_some_and(|l| l.windows()) {
            self.readout();
        }
        dirty.extend(laid);
        // the last frame's picture taken back, now nothing shows it, for the next
        if let Some(old) = shown_before {
            if let Ok(l) = Rc::try_unwrap(old) {
                self.spare = l.px;
            }
        }

        if self.full {
            self.pic.encode_all();
            dirty = vec![(0, 0, w, h)];
        } else if dirty.is_empty() {
            return None;
        } else {
            self.pic.encode_many(&dirty);
            if self.check {
                let parts = self.pic.out.clone();
                self.pic.encode_all();
                let w = self.pic.w;
                let (mut n, mut bb) = (0, (usize::MAX, usize::MAX, 0, 0));
                for (i, (a, b)) in parts.iter().zip(&self.pic.out).enumerate() {
                    if a != b {
                        n += 1;
                        let (x, y) = (i % w, i / w);
                        bb = (bb.0.min(x), bb.1.min(y), bb.2.max(x), bb.3.max(y));
                    }
                }
                if n > 0 {
                    eprintln!("ectomatter: CHECK frame {}: {n} pixels differ from a whole encode, in x {}..{} y {}..{}; encoded {:?}; keyboard at {:?}", self.frames, bb.0, bb.2, bb.1, bb.3, dirty, self.pic.keyboard_rect());
                }
            }
        }
        self.full = false;
        Some(dirty)
    }
}

#[cfg(test)]
mod kept_tests {
    use super::*;
    use ectomatter_draw::hdr::sim::Physics;

    /// The device's keys to simulate, and a make of their light for each beam.
    pub(crate) fn scene() -> (Scene, impl Fn(f32) -> Setup) {
        let core = cores::keyboard().expect("the keyboard's core");
        let (device, tree) = (core.device_now(), core.tree());
        let board = Board::new(&tree, &device, ink_env::rgb("stage", STAGE), 632, 253).unwrap();
        let base = Physics::from(&device["light"]);
        (Scene::of(&board), move |beam: f32| Setup::alike(Physics { beam, led: 7.0, ..base }))
    }

    /// A picture of another make than the one wanted is taken while the
    /// keyboard shows an older one (a slider dragged), but never over a
    /// picture of the one wanted.
    #[test]
    pub(crate) fn a_picture_on_the_way_is_taken_but_not_over_the_one_wanted() {
        let (scene, make) = scene();
        let (a, b, c) = (make(1.0), make(2.0), make(3.0));
        let pa = Simulator::settle(&scene, &a, 200);
        let pb = Simulator::settle(&scene, &b, 200);
        assert!(Kept::takes(Some(&c), Some(&pa), &pb), "on the way to the one wanted");
        assert!(Kept::takes(Some(&c), None, &pb), "on a keyboard with none");
        assert!(!Kept::takes(Some(&a), Some(&pa), &pb), "not over the one wanted");
        assert!(Kept::takes(Some(&a), Some(&pa), &pa), "the one wanted, clearer");
    }

    /// The last few settled pictures are kept, the newest found first; past
    /// the most kept the oldest goes, but never the saved tuning's.
    #[test]
    pub(crate) fn pictures_are_kept_but_the_saved_one_stays() {
        let (scene, make) = scene();
        let mut kept = Kept { pinned: Some(make(1.0)), ..Kept::default() };
        for beam in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            kept.keep(std::sync::Arc::new(Simulator::settle(&scene, &make(beam), 200)));
        }
        assert_eq!(kept.pictures.len(), Kept::MOST, "as many as are kept");
        assert!(kept.get(&make(1.0)).is_some(), "the saved tuning's stays");
        assert!(kept.get(&make(2.0)).is_none() && kept.get(&make(3.0)).is_none(), "the oldest others go");
        assert!(kept.get(&make(6.0)).is_some_and(|s| Kept::of(&s, &make(6.0))), "the newest found");
        // an LED output of its own is the same picture
        assert!(kept.get(&Setup::alike(Physics { lumens: 1.23, ..make(6.0).all })).is_some());
    }
}

/// What Ectomatter's threads tell the main window, each by its own channel:
/// the keyboards drawn, legends set, the simulation's next picture, the
/// keyboard's and the mouse's threads, the lock lights, the macros' jobs,
/// the screen's player, the app in front, games starting and the tray.
pub(crate) struct Inbox {
    built: Receiver<(bool, Board)>,
    legends: Receiver<ectomatter_draw::hdr::Legends>,
    sim: Receiver<()>,
    heard: Option<Receiver<device::Heard>>,
    locks: Option<Receiver<u8>>,
    mouse: Option<Receiver<mouse::Heard>>,
    jobs: Receiver<String>,
    news: Option<Receiver<lcd_player::News>>,
    front: Receiver<front_app::Front>,
    starts: Receiver<launcher::Start>,
    tray: Receiver<tray::TrayAsk>,
}

use std::sync::mpsc::Receiver;

/// What a step of the session asks of the shell's loop.
pub(crate) enum Step {
    /// carry on
    On,
    /// stop the loop: Ectomatter is quitting, or starting again
    Stop,
}

impl Session {
    /// Ectomatter started in its main window: every core and plugin that's
    /// on, the keyboard's and the mouse's threads, the screen's player, the
    /// macros' jobs, the app in front, the launcher and the tray, each
    /// telling the window through a sender that calls `wake`.
    pub(crate) fn start(slint: Rc<MinimalSoftwareWindow>, ui: AppWindow, settings: settings::Settings, reference: f64, gain: f32, wake: &shell::Wake) -> Session {
        let (built_tx, built) = shell::channel::<(bool, Board)>(wake);
        let build = board_builder(built_tx.clone(), false);
        let home_build = board_builder(built_tx, true);
        // the legends, set anew as the lab asks
        let (legends_tx, legends) = shell::channel::<ectomatter_draw::hdr::Legends>(wake);
        let set_legends = legends_builder(legends_tx);
        // each clearer picture of the keys' light, as the simulation has one
        let (sim_tx, sim) = shell::channel::<()>(wake);

        // every core and plugin that's on, started the one way (hosted.rs); the
        // keyboard's page talks to the first core describing a keyboard, the
        // mouse's to the first describing a mouse
        old_name::move_old_settings(&settings);
        let mut hosted = hosted::Hosted::new(std::sync::Arc::new(computer::ThisComputer::new()));
        hosted.start_all(&settings);
        devices_described(&hosted);
        let keyboard = cores::keyboard().and_then(|k| hosted.drive_id(&k.id));
        // (the keyboard's core off, Ectomatter leaves the keyboard to other programs)
        let controlled = cores::keyboard().is_none_or(|c| settings.on(&packages::switch_of(packages::Kind::Core, &c.id)));
        let z10_core = keyboard.map(|(_, core)| Ok(core));
        // (whether the keyboard is plugged in, as its core tells, for the screen's player)
        let z10_there = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (mut heard, mut locks) = (None, None);
        let kb = z10_core.clone().map(|core| {
            let (heard_tx, heard_rx) = shell::channel::<device::Heard>(wake);
            heard = Some(heard_rx);
            // the computer's lock lights, as the keyboard shows them
            let (locks_tx, locks_rx) = shell::channel::<u8>(wake);
            locks = Some(locks_rx);
            locks::start(move |bits| {
                let _ = locks_tx.send(bits);
            });
            device::start(core, heard_tx, z10_there.clone())
        });
        // the mouse's core, and the mouse's page's thread on it: it meets the
        // mouse, and writes to it only what the page changes; none while the core
        // is turned off in Settings, which leaves the mouse alone
        let mouse_core = hosted.drive("mouse").map(|(_, core)| core);
        let mut mouse_rx = None;
        let mouse_ask = mouse_core.clone().map(|m| {
            let (heard_tx, heard_rx) = shell::channel::<mouse::Heard>(wake);
            mouse_rx = Some(heard_rx);
            (mouse::start(m, heard_tx.clone()), heard_tx)
        });
        // the keyboard lit by the light setup in use, if it is one of the person's own
        let lit_by = cores::keyboard().and_then(|core| setups::in_use(&settings, &core.id).map(|s| setups::merged(&core.device_now()["light"], &s.changes)));
        let mut front = Front::new(ui, hosted, kb, controlled, settings);
        (front.mouse.ask, front.mouse.tell) = mouse_ask.unzip();
        // the jobs Ectomatter's macros do as steps (a profile switched, a screen
        // job), done here as their player reaches them on its own thread
        let (jobs_tx, jobs) = shell::channel::<String>(wake);
        front.macro_jobs = Some(jobs_tx);
        // the screen, played by Ectomatter while it controls the keyboard
        let mut news = None;
        if controlled {
            let (news_tx, news_rx) = shell::channel::<lcd_player::News>(wake);
            news = Some(news_rx);
            // (each frame through the keyboard's core, which sends it between its other exchanges)
            let screen = lcd_player::CoreSink::new(z10_core.clone().and_then(Result::ok), z10_there.clone());
            let p = lcd_player::Player::start(
                move |n| {
                    let _ = news_tx.send(n);
                },
                front.settings.on(&crate::cores::keyboard_setting("screen.announce")),
                front.game_launch(),
                false,
                Box::new(screen),
                front.worker.steps.clone(),
                front.hosted.readings(),
            );
            front.set_player(p);
        }
        // the app in front, as KWin tells it, for the profiles that follow one
        let (front_tx, front_rx) = shell::channel::<front_app::Front>(wake);
        let front_bus = front_app::start(front_tx);
        // games started through Ectomatter's launcher in Steam, each waiting for
        // its changes before it goes on
        let (starts_tx, starts) = shell::channel::<launcher::Start>(wake);
        launcher::listen(starts_tx);
        // the tray, which the window closes into
        let (tray_tx, tray_rx) = shell::channel::<tray::TrayAsk>(wake);
        let tray = tray::start(tray_tx);
        Session {
            inbox: Inbox { built, legends, sim, heard, locks, mouse: mouse_rx, jobs, news, front: front_rx, starts, tray: tray_rx },
            front_bus,
            kwin_looked: Instant::now(),
            quitting: None,
            panel_rev: 0,
            slint,
            front,
            pic: Picture::new(reference, ink_env::rgb("stage", STAGE)),
            gain,
            board: None,
            lights: Vec::new(),
            lit: None,
            spare: Vec::new(),
            dim: None,
            effect_due: Instant::now(),
            asked: None,
            build,
            home_board: None,
            home_asked: None,
            home_build,
            home_share: None,
            lit_home: false,
            _mouse: mouse_core,
            set_legends,
            full: true,
            dump: std::env::var_os("ECTOMATTER_DUMP").map(Into::into),
            check: std::env::var_os("ECTOMATTER_CHECK").is_some(),
            frames: 0,
            tray,
            tray_devices: Vec::new(),
            tray_scenes: (Vec::new(), None),
            light: lit_by,
            light_rev: (0, false),
            readout_at: None,
            sim: None,
            sim_tx,
            sim_seen: 0,
            want: None,
            kept: Kept::default(),
        }
    }

    /// What Ectomatter's threads have told the window, taken in, for a
    /// window `scale` pixels to the point: the tray's asks the shell must
    /// answer itself (the window shown, a device's page opened).
    pub(crate) fn take(&mut self, scale: f32) -> Vec<tray::TrayAsk> {
        while let Ok(l) = self.inbox.legends.try_recv() {
            self.legends_ready(l);
        }
        while let Ok((home, board)) = self.inbox.built.try_recv() {
            self.built(home, board, scale);
        }
        while self.inbox.sim.try_recv().is_ok() {
            self.sim_ready();
        }
        while let Some(h) = self.inbox.heard.as_ref().and_then(|r| r.try_recv().ok()) {
            self.front.heard(h);
        }
        while let Some(b) = self.inbox.locks.as_ref().and_then(|r| r.try_recv().ok()) {
            self.front.locks(b);
        }
        while let Some(h) = self.inbox.mouse.as_ref().and_then(|r| r.try_recv().ok()) {
            self.front.mouse_heard(h);
        }
        while let Ok(id) = self.inbox.jobs.try_recv() {
            self.front.macro_job(&id);
        }
        while let Some(n) = self.inbox.news.as_ref().and_then(|r| r.try_recv().ok()) {
            self.front.screen_news(n);
        }
        while let Ok(f) = self.inbox.front.try_recv() {
            self.front.front_app(f);
        }
        while let Ok(s) = self.inbox.starts.try_recv() {
            self.front.game_starting(s);
        }
        let mut shell = Vec::new();
        while let Ok(a) = self.inbox.tray.try_recv() {
            match a {
                tray::TrayAsk::Scene(id) => {
                    self.front.set_scene(id, true);
                    self.front.refresh();
                    self.full = true;
                }
                tray::TrayAsk::Quit => self.quit(),
                a => shell.push(a),
            }
        }
        shell
    }

    /// A key pressed or let go in the main window: Ctrl+Z and Ctrl+Shift+Z
    /// (or Ctrl+Y) undo a change to a profile or do it again, a key the
    /// page is recording or naming taken by it, Escape letting the page's
    /// chosen key or button go, Delete or Backspace taking the pad chosen on
    /// Home's desk off it, Ctrl and plus, minus or 0 zooming Home's desk in
    /// edit mode; a box being typed in takes them itself, and anything else
    /// goes to the page.
    pub(crate) fn key(&mut self, p: shell::Press) {
        let down = p.down;
        if down && p.ctrl && (p.is('z') || p.is('y')) && !self.front.typing() {
            self.front.asks.borrow_mut().push(front::Ask::Undo(p.shift || p.is('y')));
            return;
        }
        if self.front.typed(p.code, down) {
            self.front.refresh();
            return;
        }
        if down && p.is_key(Key::Escape) && !self.front.typing() && self.front.escape() {
            return;
        }
        if down && (p.is_key(Key::Delete) || p.is_key(Key::Backspace)) && !self.front.typing() && self.front.desk_delete_key() {
            return;
        }
        if down && p.ctrl && !self.front.typing() {
            let how = if p.is('=') || p.is('+') {
                Some(1)
            } else if p.is('-') {
                Some(-1)
            } else if p.is('0') {
                Some(2)
            } else {
                None
            };
            if how.is_some_and(|how| self.front.desk_zoom_key(how)) {
                return;
            }
        }
        if let Some(text) = p.text {
            self.slint.dispatch_event(if down { WindowEvent::KeyPressed { text } } else { WindowEvent::KeyReleased { text } });
        }
    }

    /// Quit as the tray's Quit does: each device its own again, and
    /// Ectomatter gone once what changed is saved (`step`).
    pub(crate) fn quit(&mut self) {
        self.front.scene_quit();
        self.front.hand_back();
        self.front.release_sliders();
        self.quitting = Some(Instant::now());
    }

    /// A turn of the shell's loop, before it draws: KWin's script looked at
    /// every five seconds (in case something took it out), a stop asked for
    /// (a log out, or kill) quit as the tray's Quit does, what was asked of
    /// the devices tried again, and Ectomatter stopped once what changed is
    /// saved (up to four seconds), or started again for a change to what it
    /// may use, back on that place.
    pub(crate) fn step(&mut self) -> Step {
        if self.kwin_looked.elapsed() > Duration::from_secs(5) && self.quitting.is_none() {
            self.kwin_looked = Instant::now();
            if let Some(c) = &self.front_bus {
                front_app::keep(c);
            }
        }
        if TERM.load(std::sync::atomic::Ordering::Relaxed) && self.quitting.is_none() {
            self.quit();
        }
        if let Some(t) = self.quitting {
            if self.front.flush() || t.elapsed() > Duration::from_secs(4) {
                // (KWin no longer told to tell a closed Ectomatter)
                if let Some(c) = &self.front_bus {
                    front_app::stop(c);
                }
                return Step::Stop;
            }
        }
        self.front.retry();
        if let Some(place) = self.front.restart.take() {
            // (each device handed back before the new Ectomatter turns the scene on again)
            self.front.scene_quit();
            if let Ok(me) = std::env::current_exe() {
                let _ = std::process::Command::new(me).args(["--place", &place]).spawn();
            }
            return Step::Stop;
        }
        Step::On
    }

    /// Ectomatter gone: each device its own again (the new Ectomatter, if it
    /// started again, turns the scene on), the screen handed back to the
    /// keyboard's own screens, a setting still being changed written, a
    /// recording letting the keyboard go, and the keyboard's thread given
    /// time to send the preview's stop.
    pub(crate) fn finish(&mut self) {
        self.front.scene_quit();
        self.front.quit_screen();
        self.front.settings.flush(true);
        if let Some(m) = &mut self.front.macros {
            m.cancel_recording(&self.front.page);
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}
