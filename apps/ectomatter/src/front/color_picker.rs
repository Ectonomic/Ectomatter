//! The color picker: opened on a swatch, its square, hue and fields, and the colors it keeps.

use super::*;

impl Front {
    pub(super) fn open_picker(&mut self, c: Ctl, piece: Option<crate::described::Piece>, index: usize, anchor: (f32, f32, f32)) {
        // the second click on the swatch whose picker a press just closed
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let described = match piece {
            Some(p) if self.on_keyboard() => self.page.described(p).map(|(id, d, e)| {
                let v = crate::described::kept(&e, &id, &d);
                (id, d, v)
            }),
            Some(p) => self.mouse.described(p),
            None => None,
        };
        let rgb = match described {
            Some((_, d, v)) => crate::described::color_at(&d, &v, index).unwrap_or([0xff, 0, 0]),
            None if c == Ctl::DeskPad => self.desk.pad_chosen.and_then(|i| self.desk.pads.get(i)).map_or(crate::desk::PAD_COLOR, |p| p.color),
            None if c == Ctl::SceneColor => self.scene_form_color(index).unwrap_or([0xff, 0x60, 0]),
            None => lighting::color_of(&self.page, c, self.paint),
        };
        self.pick = Some(Pick { ctl: c, piece, index, start: rgb, hsv: rgb2hsv(rgb), rgb, anchor });
        let recent = recent_colors();
        self.ui.set_pk_recent(ModelRc::new(VecModel::from(recent.iter().map(|c| color(*c)).collect::<Vec<_>>())));
        // under the swatch when it fits there, or when there is a fair amount of room there anyway; over it when below is cramped
        let (ax, ay, ah) = anchor;
        let (h, win) = (self.ui.get_picker_height(), self.ui.get_window_height());
        let (top, _) = crate::menu_size::hang(win, (ay, ah), h);
        self.ui.set_picker_x(ax);
        self.ui.set_picker_top(top);
        self.ui.set_pk_hue_img(hue_bar(self.scale));
        self.show_pick(None);
        self.ui.set_picker_open(true);
    }

    pub(super) fn close_picker(&mut self) {
        if let Some(p) = self.pick.take() {
            self.ui.set_picker_open(false);
            self.ui.invoke_blur_picker();
            // (a pad's color, changed as the picker moved, kept as it closes)
            if p.ctl == Ctl::DeskPad && p.rgb != p.start {
                self.desk.save_pads();
            }
            // the color the picker is closed on joins the recent ones
            if p.rgb != p.start {
                keep_recent(p.rgb);
            }
        }
    }

    /// Show the color everywhere but the field being typed in.
    pub(super) fn show_pick(&mut self, skip: Option<i32>) {
        let Some(p) = &self.pick else { return };
        let [h, sv, v] = p.hsv;
        let rgb = p.rgb;
        self.ui.set_pk_hue(h);
        self.ui.set_pk_sat(sv);
        self.ui.set_pk_val(v);
        self.ui.set_pk_now(color(rgb));
        self.ui.set_pk_pure(color(hsv2rgb([h, 1.0, 1.0])));
        self.ui.set_pk_sv_img(sv_square(h, self.scale));
        let fields = [
            format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]),
            rgb[0].to_string(),
            rgb[1].to_string(),
            rgb[2].to_string(),
            (h.round() as i32).to_string(),
            ((sv * 100.0).round() as i32).to_string(),
            ((v * 100.0).round() as i32).to_string(),
        ];
        let setters: [fn(&AppWindow, SharedString); 7] = [
            AppWindow::set_pk_hex,
            AppWindow::set_pk_r,
            AppWindow::set_pk_g,
            AppWindow::set_pk_b,
            AppWindow::set_pk_h,
            AppWindow::set_pk_s,
            AppWindow::set_pk_v,
        ];
        for (i, (set, t)) in setters.iter().zip(fields).enumerate() {
            if skip != Some(i as i32) {
                set(&self.ui, t.into());
            }
        }
    }

    /// A new color in the picker: shown, and set where the picker was opened.
    pub(super) fn pick_set(&mut self, hsv: [f32; 3], rgb: Option<[u8; 3]>, skip: Option<i32>) {
        let Some(p) = &mut self.pick else { return };
        p.hsv = hsv;
        p.rgb = rgb.unwrap_or_else(|| hsv2rgb(hsv));
        let (c, piece, index, rgb) = (p.ctl, p.piece, p.index, p.rgb);
        if c == Ctl::Paint {
            self.paint = rgb;
        } else if c == Ctl::SceneColor {
            self.set_scene_form_color(index, rgb);
        } else if c == Ctl::DeskPad {
            self.desk.set_pad_color(rgb);
            self.show_desk();
        } else if let Some(piece) = piece.filter(|_| self.on_keyboard()) {
            if let Some((id, d, _)) = self.page.described(piece) {
                self.page.change_settings(|e| crate::described::keep_color(e, &id, &d, index, rgb));
            }
        } else if let Some(piece) = piece {
            if let Some((_, d, v)) = self.mouse.described(piece) {
                self.mouse.set_described(piece, crate::described::with_color(&d, &v, index, rgb));
            }
            self.refresh();
        } else {
            lighting::set_color(&mut self.page, c, rgb);
        }
        self.show_pick(skip);
        self.push_preview();
    }

    /// A field typed in: a hex code, or a number of its own.
    pub(super) fn pick_field(&mut self, i: i32, t: &str) {
        let Some(p) = &self.pick else { return };
        let (rgb, hsv) = (p.rgb, p.hsv);
        match i {
            0 => {
                let t = t.trim().trim_start_matches('#');
                let full: String = if t.len() == 3 { t.chars().flat_map(|c| [c, c]).collect() } else { t.to_string() };
                if full.len() == 6 {
                    if let Ok(v) = u32::from_str_radix(&full, 16) {
                        let c = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
                        self.pick_set(rgb2hsv(c), Some(c), Some(0));
                    }
                }
            }
            1..=3 => {
                if let Ok(v) = t.trim().parse::<f32>() {
                    let mut c = rgb;
                    c[(i - 1) as usize] = v.round().clamp(0.0, 255.0) as u8;
                    self.pick_set(rgb2hsv(c), Some(c), Some(i));
                }
            }
            4..=6 => {
                if let Ok(v) = t.trim().parse::<f32>() {
                    let mut n = hsv;
                    match i {
                        4 => n[0] = v.clamp(0.0, 359.0),
                        5 => n[1] = v.clamp(0.0, 100.0) / 100.0,
                        _ => n[2] = v.clamp(0.0, 100.0) / 100.0,
                    }
                    self.pick_set(n, None, Some(i));
                }
            }
            _ => {}
        }
    }

    pub(super) fn choose_profile(&mut self, n: u8) {
        if self.page.dev.is_none() {
            return;
        }
        self.close_menu();
        self.page.choose(n);
        if n != self.page.active() {
            self.ask_kb(device::Ask::Activate(n));
        }
    }

    /// Notice what changed since last looked: each change marks its profile
    /// to be saved, and what it was before can be undone. Changes close
    /// together (a slider dragged, a color picked) undo as one.
    pub(super) fn note_changes(&mut self) {
        // the screen's pages, undone as one entry (0, which no profile is)
        if self.screens != self.screens_noted {
            let burst = self.undo_mark.is_some_and(|(m, t)| m == 0 && t.elapsed() < Duration::from_millis(1200));
            if !burst {
                self.undo.push((0, self.screens_noted.to_json()));
                if self.undo.len() > 100 {
                    self.undo.remove(0);
                }
            }
            self.undo_mark = Some((0, Instant::now()));
            self.redo.clear();
            self.screens_noted = self.screens.clone();
        }
        if self.page.dev.is_none() {
            return;
        }
        let now = Instant::now();
        for (n, d) in &self.page.drafts {
            match self.last_drafts.get(n) {
                None => {
                    self.last_drafts.insert(*n, d.clone());
                }
                Some(before) if before != d => {
                    let burst = self.undo_mark.is_some_and(|(m, t)| m == *n && t.elapsed() < Duration::from_millis(1200));
                    if !burst {
                        self.undo.push((*n, before.clone()));
                        if self.undo.len() > 100 {
                            self.undo.remove(0);
                        }
                    }
                    self.undo_mark = Some((*n, now));
                    self.redo.clear();
                    self.changed_at.insert(*n, now);
                    self.last_drafts.insert(*n, d.clone());
                }
                _ => {}
            }
        }
    }

    /// Save a profile that has changed, once it has been still a moment.
    pub(super) fn auto_save(&mut self) {
        self.note_changes();
        if self.pages_changed.is_some_and(|t| t.elapsed() >= Duration::from_millis(1200)) {
            self.save_pages();
        }
        if self.page.dev.is_none() || self.page.busy || self.kb.is_none() {
            return;
        }
        let quiet = |t: Option<&Instant>| t.is_some_and(|t| t.elapsed() >= Duration::from_millis(1200));
        let due = crate::cores::profiles().into_iter().find(|n| self.page.is_dirty(*n) && quiet(self.changed_at.get(n)));
        if let Some(n) = due {
            self.save_now(n);
        }
    }

    pub(super) fn save_now(&mut self, n: u8) {
        let Some(d) = self.page.drafts.get(&n).cloned() else { return };
        self.page.busy = true;
        self.saving = Some((n, d));
        self.ask_kb(device::Ask::Save(n, self.page.save_body_of(n)));
    }

    /// Put back how a profile was before its last change (or, `redo`, as it
    /// was before the last undo); it saves as any change does.
    pub(super) fn undo(&mut self, redo: bool) {
        self.note_changes();
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let Some((n, d)) = from.pop() else { return };
        if n == 0 {
            to.push((0, self.screens.to_json()));
            self.screens = crate::lcd_page::Screens::from_json(&d);
            self.screens_noted = self.screens.clone();
            self.undo_mark = None;
            if let Some(ed) = &mut self.editor {
                ed.page = ed.page.min(self.screens.library.len().saturating_sub(1));
                ed.chosen = ed.chosen.filter(|i| self.screens.library.get(ed.page).is_some_and(|p| *i < p.pieces.len()));
            }
            self.pages_edited();
            self.status(if redo { "Did again the last change on the Screen tab" } else { "Undid the last change on the Screen tab" }, 0);
            self.refresh();
            return;
        }
        if let Some(now) = self.page.drafts.insert(n, d.clone()) {
            to.push((n, now));
        }
        self.last_drafts.insert(n, d);
        self.changed_at.insert(n, Instant::now());
        self.undo_mark = None;
        let name = self.page.profile_name(n);
        self.status(if redo { format!("Did again the change to {name}") } else { format!("Undid the last change to {name}") }, 0);
        self.push_preview();
        self.refresh();
    }

    /// Ectomatter closing: whatever has changed is saved now, without waiting.
    /// Whether everything is saved, so it may go.
    pub fn flush(&mut self) -> bool {
        self.note_changes();
        self.save_pages();
        if self.released.is_some_and(|t| t.elapsed() < Duration::from_millis(300)) {
            return false;
        }
        if self.page.dev.is_none() || self.kb.is_none() {
            return true;
        }
        if !self.page.busy {
            if let Some(n) = crate::cores::profiles().into_iter().find(|n| self.page.is_dirty(*n) && self.changed_at.contains_key(n)) {
                self.save_now(n);
            }
        }
        !self.page.busy && !crate::cores::profiles().into_iter().any(|n| self.page.is_dirty(n) && self.changed_at.contains_key(&n))
    }

    /// What the page's typing goes to: a box being typed in takes Ctrl+Z itself.
    pub fn typing(&self) -> bool {
        slint::ComponentHandle::global::<crate::Typing>(&self.ui).get_on()
    }

    /// A key clicked on the board: chosen for editing on the Keys page; on
    /// the Lighting page it lights as pressing it would.
    pub(super) fn select_key(&mut self, pos: u32) {
        self.slider_sel = 0;
        if self.page.dev.is_none() {
            return;
        }
        let now = self.t0.elapsed().as_secs_f64() * 1000.0;
        if self.page.view == View::Lighting {
            // painting key by key: a click paints the key
            if self.page.effect_desc(&self.page.effect_id())["paint"] == true {
                let Some(k) = self.page.key(pos).cloned() else { return };
                if !k.led {
                    self.status(format!("{} has no light behind it.", self.page.key_label(&k)), 0);
                    return;
                }
                if let Some(c) = self.page.custom.get_mut(pos as usize) {
                    *c = self.paint;
                }
                lighting::save_custom(self.page.editing, &self.page.custom);
                self.push_preview();
                return;
            }
            // on Some areas, a key in an area turns that area on or off, as its chip does
            if self.some_areas() {
                if let Some(i) = self.area_of(pos) {
                    self.kb_zone(i);
                    return;
                }
            }
            // otherwise it lights as pressing it would, for Reactive
            self.page.flash(pos, now);
            return;
        }
        if self.page.view != View::Keys {
            self.page.view = View::Keys;
        } else if self.page.sel == Some(pos) {
            // pressing it again lets it go
            self.page.sel = None;
            self.close_menu();
            return;
        }
        self.close_menu();
        self.listening = None;
        self.page.sel = Some(pos);
    }


    /// A key typed into Ectomatter's window, as a Linux key code, which "Press
    /// a key" takes. True if it was taken. (The board shows the keyboard's
    /// keys going down and coming up from the keyboard itself: see [watched].)
    pub fn typed(&mut self, code: u32, down: bool) -> bool {
        let Some((mods, hid)) = keyboard::evdev_to_hid(code) else { return self.listening.is_some() };
        if let Some((layer, held)) = self.listening {
            if down {
                if hid == 0 {
                    self.listening = Some((layer, held | mods));
                } else {
                    self.assign(layer, keyboard::pressed_action(held, hid));
                }
            } else if hid == 0 {
                // a lone modifier pressed and let go assigns that modifier
                if held == mods {
                    self.assign(layer, keyboard::pressed_action(held, 0));
                } else {
                    self.listening = Some((layer, held & !mods));
                }
            }
            return true;
        }
        false
    }

    /// What the keyboard did, from its own devices, whichever window has it:
    /// a key on the board goes down and comes up with the real one (lighting
    /// its Reactive effect as it goes down), the lock lights follow the
    /// keyboard's, and a keyboard gone lets every key go.
    /// A key pressed or let go on the keyboard, as its core tells it: one on
    /// a host slot does its job; each shows on the keyboard's picture, at the
    /// key that sends what it sent.
    pub fn keyboard_input(&mut self, p: &serde_json::Value, down: bool) {
        if let Some(n) = p["host"].as_u64() {
            self.screen_key_event(keyboard::NEXT_LAYOUT + n as u32, down);
        }
        let Some(sent) = Action::read(&p["sends"]) else { return };
        let Some(pos) = self.page.pos_for(&sent).filter(|p| self.page.key(*p).is_some()) else { return };
        if down {
            if self.page.pressed.insert(pos) {
                let now = self.t0.elapsed().as_secs_f64() * 1000.0;
                self.page.flash(pos, now);
                self.ripple_from_key(pos, Duration::ZERO);
                self.refresh();
            }
        } else if self.page.pressed.remove(&pos) {
            self.refresh();
        }
    }

    /// A key pressed on the keyboard Home shows: a ripple runs out across the
    /// desk from the key, `age` into its run.
    pub fn ripple_from_key(&mut self, pos: u32, age: Duration) {
        let Some(k) = self.page.key(pos) else { return };
        let Some(at) = self.on_desk_keyboard(k.cx, k.cy) else { return };
        self.ripple_at(at, age);
    }

    /// A mouse button pressed (by the core's name for it, see mouse::BUTTONS):
    /// a ripple from the button, where the mouse sits on Home's desk. Not the
    /// pointer's click (the button whose job is the mouse's own button 1): its
    /// ripple comes from where the pointer clicks the desk, and from nowhere
    /// when the click is elsewhere.
    pub fn ripple_from_button(&mut self, control: &str, age: Duration) {
        let Some(b) = crate::mouse::shape().buttons.iter().position(|b| b.id == control) else { return };
        if self.mouse.job(b)["mouse"].as_u64() == Some(1) {
            return;
        }
        let Some((dw, layers)) = self.desk_layers() else { return };
        let Some(&(x, y)) = crate::mouse_art::button_middles(layers, dw).get(b) else { return };
        let Some(at) = self.on_desk_mouse(x, y) else { return };
        self.ripple_at(at, age);
    }

    /// A ripple from a point of Home's desk (its logical pixels), `age` into
    /// its run, in the light nearest it as the devices show it now (see
    /// desk::ripple_light): none while Home doesn't show, or while the window
    /// is closed into the tray, where the desk isn't drawn.
    pub(super) fn ripple_at(&mut self, at: (f32, f32), age: Duration) {
        if self.hidden || self.ui.get_place() != "home" {
            return;
        }
        let color = crate::desk::ripple_light(at, &self.lights_on_desk());
        self.desk.ripple(at, color, age);
        self.desk_frames_run();
    }

    /// Each light on Home's desk as it shows now: where it sits (the desk's
    /// logical pixels) and its color, none where it's dark. The keys as the
    /// keyboard was last lit (in the desk's light, while that plays), a key's
    /// Reactive flash from the moment its press starts it; the mouse's logo
    /// and wheel.
    pub(super) fn lights_on_desk(&self) -> Vec<((f32, f32), Option<[u8; 3]>)> {
        let mut out = Vec::new();
        let drawn: HashMap<u32, &Light> = self.last_lights.iter().flatten().map(|l| (l.pos, l)).collect();
        let scene = !self.scene_colors.0.is_empty();
        for k in self.page.layout.iter().filter(|k| k.led) {
            let Some(at) = self.on_desk_keyboard(k.cx, k.cy) else { break };
            let color = if scene {
                self.scene_colors.0.get(&k.pos).copied()
            } else {
                self.page.flash_look(k.pos).or_else(|| drawn.get(&k.pos).filter(|l| l.strength > 0.0).map(|l| l.rgb))
            };
            out.push((at, color));
        }
        if let Some((dw, layers)) = self.desk_layers() {
            let now = Instant::now();
            for (i, b) in crate::mouse_art::light_boxes(layers, dw).iter().enumerate() {
                if let Some(at) = self.on_desk_mouse((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0) {
                    out.push((at, self.mouse_color(i, now)));
                }
            }
        }
        out
    }

    /// A point of the keyboard's drawing (the board's own units, as the
    /// page's view_box has them) where the keyboard sits on Home's desk, in
    /// the desk's logical pixels; none off Home.
    pub(super) fn on_desk_keyboard(&self, x: f64, y: f64) -> Option<(f32, f32)> {
        let [kx, ky, kw, kh] = self.desk_keyboard()?;
        let (vx, vy, vw, vh) = self.page.view_box;
        Some((kx + ((x - vx) / vw) as f32 * kw - self.ui.get_desk_x(), ky + ((y - vy) / vh) as f32 * kh - self.ui.get_desk_y()))
    }

    /// The same for a point of the mouse's drawing (see mouse_art::view).
    pub(super) fn on_desk_mouse(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let i = self.desk.items.iter().position(|it| it.id == crate::mouse::key())?;
        let [mx, my, mw, mh] = self.desk.screen(i);
        let v = crate::mouse_art::view();
        Some((mx + (x - v.0) / v.2 * mw, my + (y - v.1) / v.3 * mh))
    }

    /// The computer's lock lights now (bit 0 Num Lock, 1 Caps Lock, 2 Scroll
    /// Lock), as the keyboard shows them.
    pub fn locks(&mut self, bits: u8) {
        if self.page.locks != bits {
            self.page.locks = bits;
            // the keyboard is lit again with them
            self.last_lights = None;
        }
    }

    pub(super) fn assign(&mut self, layer: usize, a: Action) {
        self.listening = None;
        if let Some(pos) = self.page.sel {
            self.page.set_def(LAYERS[layer].0, pos, &a);
        }
        self.refresh();
    }
}
