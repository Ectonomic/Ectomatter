//! Home's desk: the devices where they sit, edit mode, and the pads under them.

use super::*;

impl Front {
    /// The devices Ectomatter knows, on the desk: those it has read or met, and
    /// those it has kept a place for; each at its size, and whether it is
    /// plugged in.
    pub(super) fn desk_meet(&mut self) {
        if let Some(core) = crate::cores::keyboard().filter(|_| self.met(crate::cores::keyboard_key())) {
            let size = crate::desk::size_of(&core.art, core.device["light"]["scale"].as_f64().unwrap_or(1.38) as f32);
            // (not yet known to be plugged in, it is drawn as plugged in: only a keyboard gone shows dimmed)
            let kind = core.device["kind"].as_str().unwrap_or("keyboard").to_string();
            self.desk.meet(crate::cores::keyboard_key(), core.name(), "keyboard", &kind, size, self.ui.get_conn_state() != 2, true);
        }
        if self.met(crate::mouse::key()) {
            let dev = &self.mouse.state["device"];
            let v = crate::mouse_art::view();
            let per = dev["scale"].as_f64().unwrap_or(520.0 / 125.1) as f32;
            let size = (v.2 / per * crate::desk::PER_MM, v.3 / per * crate::desk::PER_MM);
            let name = dev["name"].as_str().or(crate::mouse::shape().described["name"].as_str()).unwrap_or_default().to_string();
            let kind = dev["kind"].as_str().unwrap_or("mouse").to_string();
            self.desk.meet(crate::mouse::key(), &name, "mouse", &kind, size, self.mouse.present() || self.mouse.state.is_null(), false);
        }
    }

    /// The desk as it is now: its devices where the view puts them, edit
    /// mode's parts, and the mouse's picture.
    pub(super) fn show_desk(&mut self) {
        let ui = &self.ui;
        self.desk.resize(ui.get_desk_w(), ui.get_desk_h());
        let names = self.desk_names();
        let d = &self.desk;
        let items: Vec<DeskItem> = d
            .items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let [x, y, w, h] = d.screen(i);
                let backed = d.name_backed(names[i]);
                DeskItem { kind: if it.kind == "keyboard" { 0 } else { 1 }, name: it.name.as_str().into(), sub: if it.connected { "".into() } else { "Not connected".into() }, x, y, w, h, off: !it.connected, hover: d.hover == Some(i), backed }
            })
            .collect();
        show_rows(ui.get_desk_items(), |r| ui.set_desk_items(r), items);
        let r = crate::desk::PAD_RADIUS * d.view.z;
        let pads: Vec<DeskPad> = d.pads_shown().into_iter().map(|([x, y, w, h], chosen, hover, drawing)| DeskPad { x, y, w, h, r, chosen, hover, drawing }).collect();
        show_rows(ui.get_desk_pads(), |r| ui.set_desk_pads(r), pads);
        // (what the desk said of a picture is said while its pad is chosen)
        if d.pad_chosen.is_none() {
            self.pad_note.clear();
        }
        let d = &self.desk;
        let chosen = d.pad_chosen.and_then(|i| d.pads.get(i));
        ui.set_desk_pad_chosen(d.pad_chosen.filter(|_| chosen.is_some()).map_or(-1, |i| i as i32));
        if let Some(p) = chosen {
            ui.set_desk_pad_rgb(color(p.color));
        }
        ui.set_desk_pad_pictured(chosen.is_some_and(|p| p.picture.is_some()));
        ui.set_desk_pad_size(chosen.map_or(String::new(), |p| crate::desk::size_words(p.w / crate::desk::PER_MM, p.h / crate::desk::PER_MM)).into());
        ui.set_desk_drawing(d.editing && d.draw_tool);
        ui.set_desk_pad_note(self.pad_note.as_str().into());
        ui.set_desk_editing(d.editing);
        self.show_rail();
        ui.set_desk_panning(d.panning());
        ui.set_desk_carrying(d.carrying());
        ui.set_desk_zoom(d.zoom_text().into());
        ui.set_desk_empty(if d.items.is_empty() { crate::desk::works_with().into() } else { "".into() });
        self.show_desk_mouse();
        // (the mouse's lights, and their effects playing, on the desk too)
        self.show_mouse_lights();
        self.mouse_frames_run();
    }

    /// The mouse on the desk: its picture as drawn for its size there, or its
    /// page's shown at that size while that one is drawn.
    pub(super) fn show_desk_mouse(&mut self) {
        let Some(i) = self.desk.items.iter().position(|it| it.id == crate::mouse::key()) else { return };
        let shown = self.desk.screen(i)[2];
        let scale = self.scale;
        let imgs = match (&self.desk_mouse, &self.mouse_layers) {
            (Some((w, imgs)), _) => Some((*w as f32 / scale, imgs)),
            (None, Some((_, imgs))) => Some((240.0, imgs)),
            _ => None,
        };
        if let Some((drawn, imgs)) = imgs {
            let ui = &self.ui;
            ui.set_desk_mouse_picture(imgs.base.clone());
            ui.set_desk_mouse_m(imgs.layers.margin as f32 / scale);
            ui.set_desk_mouse_k(shown / drawn);
            ui.set_desk_mouse_logo_under(imgs.lights[0].clone());
            ui.set_desk_mouse_wheel_under(imgs.lights[1].clone());
            ui.set_desk_mouse_logo(imgs.lights[2].clone());
            ui.set_desk_mouse_wheel(imgs.lights[3].clone());
        }
    }

    /// Once the desk has settled, the mouse's picture drawn for its size on
    /// it (to the nearest four pixels), unless it is drawn or being drawn.
    pub(super) fn desk_mouse_want(&mut self) {
        let Some(i) = self.desk.items.iter().position(|it| it.id == crate::mouse::key()) else { return };
        if !self.desk_settled() || self.mouse_layers.is_none() {
            return;
        }
        let w = ((self.desk.screen(i)[2] * self.scale / 4.0).round() * 4.0).clamp(32.0, 1600.0) as u32;
        let page = (240.0 * self.scale).round() as u32;
        if w == page {
            if self.desk_mouse.take().is_some() {
                self.show_desk_mouse();
            }
            return;
        }
        if self.desk_mouse.as_ref().is_some_and(|(dw, _)| *dw == w) || self.desk_mouse_drawing == Some(w) {
            return;
        }
        self.desk_mouse_drawing = Some(w);
        self.mouse_draw_at(w);
    }

    /// The desk's easing, fading and ripples, a frame each sixtieth of a
    /// second while they go on.
    pub(super) fn desk_frames_run(&mut self) {
        let want = self.desk.animating() || self.desk.rippling();
        if want == self.desk_frames.is_some() {
            return;
        }
        self.desk_frames = want.then(|| {
            let t = slint::Timer::default();
            let asks = self.asks.clone();
            t.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || asks.borrow_mut().push(Ask::DeskTick));
            t
        });
    }

    /// Whether the desk has come to rest: nothing easing or carried, and the
    /// view the same for a moment, so pictures drawn for its sizes are worth drawing.
    pub fn desk_settled(&self) -> bool {
        !self.desk.animating() && !self.desk.panning() && !self.desk.carrying() && self.desk.changed.elapsed() >= Duration::from_millis(150)
    }

    /// Where the keyboard is on Home's desk, in the window's logical pixels, while Home shows it.
    pub fn desk_keyboard(&self) -> Option<[f32; 4]> {
        if self.ui.get_place() != "home" {
            return None;
        }
        let i = self.desk.items.iter().position(|it| it.id == crate::cores::keyboard_key())?;
        let [x, y, w, h] = self.desk.screen(i);
        Some([self.ui.get_desk_x() + x, self.ui.get_desk_y() + y, w, h])
    }

    /// The keyboard's size on Home's desk as the view has it, in logical
    /// pixels, whatever shows: for its picture to be drawn ahead of Home's opening.
    pub fn desk_keyboard_size(&self) -> Option<(f32, f32)> {
        let i = self.desk.items.iter().position(|it| it.id == crate::cores::keyboard_key())?;
        let [_, _, w, h] = self.desk.screen(i);
        (self.desk.size.0 > 0.0).then_some((w, h))
    }

    /// Whether the keyboard on the desk is plugged in.
    pub fn desk_keyboard_on(&self) -> bool {
        self.desk.items.iter().find(|it| it.id == crate::cores::keyboard_key()).is_none_or(|it| it.connected)
    }

    /// What the desk lays on the window's ground, in the buffer's pixels, while Home shows.
    pub fn desk_decor(&self, scale: f32) -> crate::picture::Decor {
        if self.ui.get_place() != "home" {
            return crate::picture::Decor::default();
        }
        let d = &self.desk;
        // each device's name, under it, for the dots to leave clear
        let names = self.desk_names();
        d.decor((self.ui.get_desk_x(), self.ui.get_desk_y()), scale, &names, &|path| self.pad_photos.get(path).cloned())
    }

    /// Where each device's name is on the desk's room, under it (its words
    /// and, for one not plugged in, the line under them), in logical pixels.
    pub(super) fn desk_names(&self) -> Vec<[f32; 4]> {
        let d = &self.desk;
        d.items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let [x, y, w, h] = d.screen(i);
                let nw = crate::text::width(crate::tokens::font::MEDIUM, crate::tokens::font::TEXT, &it.name) + 15.0;
                let sw = if it.connected { 0.0 } else { crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::CAPTION, "Not connected") };
                let half = nw.max(sw) / 2.0;
                let top = y + h + crate::desk::NAME_GAP;
                [x + w / 2.0 - half, top, x + w / 2.0 + half, top + if it.connected { 18.0 } else { 38.0 }]
            })
            .collect()
    }

    /// The pads' pictures: each one not read yet, read away from the window's
    /// thread (at once, offscreen), and kept once read for the desk to show;
    /// and the picture asked for a pad, once the desktop's file picker
    /// answers and the file chosen is read, put on the pad.
    pub fn poll_pads(&mut self) {
        let offscreen = crate::desk::offscreen();
        // (a picture no pad shows any more let go of)
        let pads = &self.desk.pads;
        self.pad_photos.retain(|k, _| pads.iter().any(|p| p.picture.as_deref() == Some(k.as_str())));
        let mut wanted: Vec<String> = Vec::new();
        for p in self.desk.pads.iter().filter_map(|p| p.picture.clone()) {
            if !self.pad_photos.contains_key(&p) && !self.pad_unread.contains(&p) && !self.pad_reading.iter().any(|(r, _)| *r == p) && !wanted.contains(&p) {
                wanted.push(p);
            }
        }
        for path in wanted {
            let (tx, rx) = mpsc::channel();
            let p = path.clone();
            let read = move || {
                let _ = tx.send(crate::desk::photo_of(std::path::Path::new(&p)));
            };
            if offscreen {
                read();
            } else {
                std::thread::spawn(read);
            }
            self.pad_reading.push((path, rx));
        }
        let (reading, photos, unread) = (&mut self.pad_reading, &mut self.pad_photos, &mut self.pad_unread);
        reading.retain(|(path, rx)| match rx.try_recv() {
            Ok(Ok(photo)) => {
                photos.insert(path.clone(), std::sync::Arc::new(photo));
                false
            }
            Ok(Err(e)) => {
                eprintln!("ectomatter: a pad's picture, {path}: {e}");
                unread.insert(path.clone());
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        let Some((i, was, ask)) = self.pad_asked.take() else { return };
        match ask {
            PadAsk::Picking(rx) => match rx.try_recv() {
                Ok(Some(path)) => {
                    let (tx, rx) = mpsc::channel();
                    let read = move || {
                        let _ = tx.send(crate::desk::import_picture(&path));
                    };
                    if offscreen {
                        read();
                    } else {
                        std::thread::spawn(read);
                    }
                    self.pad_asked = Some((i, was, PadAsk::Reading(rx)));
                    // (read now, offscreen)
                    if offscreen {
                        self.poll_pads();
                    }
                }
                Ok(None) | Err(mpsc::TryRecvError::Disconnected) => {}
                Err(mpsc::TryRecvError::Empty) => self.pad_asked = Some((i, was, PadAsk::Picking(rx))),
            },
            PadAsk::Reading(rx) => match rx.try_recv() {
                Ok(Ok((path, photo))) => {
                    let key = path.to_string_lossy().to_string();
                    self.pad_unread.remove(&key);
                    self.pad_photos.insert(key.clone(), std::sync::Arc::new(photo));
                    // (a pad deleted or moved while its picture was chosen takes
                    // none, and the copy kept for it goes)
                    if self.desk.set_pad_picture(i, was, Some(key.clone())) {
                        self.pad_note.clear();
                    } else {
                        self.desk.forget_picture(Some(key));
                    }
                    self.show_desk();
                }
                Ok(Err(why)) => {
                    self.pad_note = why;
                    self.show_desk();
                }
                Err(mpsc::TryRecvError::Empty) => self.pad_asked = Some((i, was, PadAsk::Reading(rx))),
                Err(mpsc::TryRecvError::Disconnected) => {}
            },
        }
    }

    /// A picture asked for the chosen pad: the desktop's own file picker
    /// (offscreen, ECTOMATTER_PAD_PICTURE's file, as if chosen there).
    pub(super) fn ask_pad_picture(&mut self) {
        let Some((i, b)) = self.desk.chosen_box() else { return };
        if matches!(self.pad_asked, Some((_, _, PadAsk::Picking(_)))) {
            return;
        }
        self.close_picker();
        self.pad_note.clear();
        let rx = match std::env::var_os("ECTOMATTER_PAD_PICTURE").filter(|_| crate::desk::offscreen()) {
            Some(f) => {
                let (tx, rx) = mpsc::channel();
                let _ = tx.send(Some(std::path::PathBuf::from(f)));
                rx
            }
            None => crate::picker::open_file("Choose a picture for the pad", "Pictures", &["*.png", "*.jpg", "*.jpeg", "*.webp", "*.gif", "*.svg", "*.PNG", "*.JPG", "*.JPEG", "*.WEBP", "*.GIF", "*.SVG"]),
        };
        self.pad_asked = Some((i, b, PadAsk::Picking(rx)));
        // (offscreen, the answer taken at once: a snapshot has no later turn to take it on)
        if crate::desk::offscreen() {
            self.poll_pads();
        }
        self.show_desk();
    }

    /// Delete or Backspace on Home in edit mode: the chosen pad taken off the
    /// desk; false where no pad is chosen (or the color picker is open on it).
    pub fn desk_delete_key(&mut self) -> bool {
        if self.ui.get_place() != "home" || !self.desk.editing || self.desk.pad_chosen.is_none() || self.pick.is_some() {
            return false;
        }
        self.desk.delete_pad();
        self.pad_note.clear();
        self.show_desk();
        true
    }

    /// Ctrl with plus, minus or 0 on Home in edit mode: zoom in, out, or to the
    /// fit (`how` as the zoom controls have it); false anywhere else.
    pub fn desk_zoom_key(&mut self, how: i32) -> bool {
        if self.ui.get_place() != "home" || !self.desk.editing {
            return false;
        }
        self.desk.zoom_step(how);
        self.desk_frames_run();
        true
    }

    /// The mouse's page, as it is now: its tab, its picture, its list and its boxes.
    pub(super) fn refresh_mouse(&mut self) {
        use crate::mouse::KINDS;
        // (other devices' jobs named as their pages have them now)
        let names = self.job_names();
        if self.mouse.names != names {
            self.mouse.names = names;
        }
        let mouse_scene = (self.mouse.tab == 1).then(|| self.scene_lighting(crate::mouse::key())).flatten();
        if mouse_scene.is_some() {
            self.ui.set_mouse_hint_chip(self.scene_chip(self.scene_on().as_ref()));
        }
        let ui = &self.ui;
        let m = &self.mouse;
        ui.set_mouse_tab(m.tab as i32);
        ui.set_mouse_hint_chip_on(mouse_scene.is_some());
        match &mouse_scene {
            Some(name) => ui.set_mouse_hint(format!("The {name} scene is lighting the desk. Choosing an effect here turns the scene off.").into()),
            None => ui.set_mouse_hint(m.hint().into()),
        }
        ui.set_mouse_status(m.failed.clone().unwrap_or_default().into());
        // the profiles, and what was last done to them, with Undo for a moment
        // (a profile tied to apps says so under its name)
        let mut items = m.profile_items();
        if let Some(store) = &m.profiles {
            for (it, p) in items.iter_mut().zip(&store.list) {
                if let Some(w) = self.with_apps(&p.apps) {
                    it.sub = w.into();
                }
            }
        }
        show_rows(ui.get_mouse_profiles(), |r| ui.set_mouse_profiles(r), items);
        ui.set_mouse_profiles_shown(m.profiles.is_some());
        ui.set_mouse_said(m.said_now().into());
        ui.set_mouse_undo_shown(m.undo_shown());
        // (each list kept, its rows changed in place: a list made anew would
        // drop a press the pointer is in the middle of)
        show_rows(ui.get_mouse_calls(), |r| ui.set_mouse_calls(r), m.calls());
        let (lines, dots, on_line, on_dot) = m.lines();
        ui.set_mouse_lines(lines.into());
        ui.set_mouse_dots(dots.into());
        ui.set_mouse_line_on(on_line.into());
        ui.set_mouse_dot_on(on_dot.into());
        show_rows(ui.get_mouse_rows(), |r| ui.set_mouse_rows(r), m.rows());
        // Buttons: the Key library, as the Keys tab has it: the layer the page
        // edits as its tab, then the buttons in their groups
        let layers: Vec<LayerTab> = m.state["device"]["capabilities"]["keys"]["layers"]
            .as_array()
            .and_then(|ls| ls.first())
            .map(|l| vec![LayerTab { name: l["label"].as_str().unwrap_or("When pressed").into(), cap: "".into() }])
            .unwrap_or_else(|| vec![LayerTab { name: "When pressed".into(), cap: "".into() }]);
        show_rows(ui.get_mouse_layers(), |r| ui.set_mouse_layers(r), layers);
        let q = self.mouse_lib_query.trim().to_lowercase();
        let all = m.lib_list("");
        let list = m.lib_list(&q);
        // (its groups start open for a mouse's few buttons, scrolling in their
        // box, and shut where the whole list runs past 600 high, as the
        // keyboard's 116 keys do: a heading 30 high and a row 34, under the tab
        // and the find box)
        let need = 50.0 + 60.0 + 10.0 + all.iter().map(|(_, n, _)| 30.0 + 34.0 * *n as f32).sum::<f32>();
        if self.mouse_lib_shut.is_none() && !all.is_empty() {
            self.mouse_lib_shut = Some(if need > 600.0 { all.iter().map(|(g, ..)| g.clone()).collect() } else { Default::default() });
        }
        // a button newly chosen (on the picture, say): its group opens to show it, once
        let newly = m.button.is_some() && m.button != self.mouse_lib_revealed;
        self.mouse_lib_revealed = m.button;
        if newly {
            if let (Some(shut), Some((g, ..))) = (self.mouse_lib_shut.as_mut(), all.iter().find(|(_, _, rows)| rows.iter().any(|r| Some(r.0) == m.button))) {
                shut.remove(g);
            }
        }
        let shut = self.mouse_lib_shut.clone().unwrap_or_default();
        let mut groups: Vec<LibGroup> = Vec::new();
        for (g, total, rows) in list {
            if rows.is_empty() && !q.is_empty() {
                continue;
            }
            let open = !q.is_empty() || !shut.contains(&g);
            let rows: Vec<LibRow> = rows
                .into_iter()
                .map(|(i, cap, name, job, rebound)| LibRow { id: i as i32, cap: cap.into(), name: name.into(), job: job.into(), rebound, chosen: m.button == Some(i) })
                .collect();
            let rebound = rows.iter().filter(|r| r.rebound).count() as i32;
            let model = self.mouse_lib_rows.entry(g.clone()).or_insert_with(|| Rc::new(VecModel::default())).clone();
            set_rows(&model, if open { rows } else { Vec::new() });
            groups.push(LibGroup { name: g.into(), count: total as i32, rebound, open, rows: ModelRc::from(model) });
        }
        // (the list scrolled to it, where it is longer than its box)
        if newly {
            let mut y = 0.0;
            for g in &groups {
                y += crate::tokens::size::LIST_HEAD;
                let rows: Vec<LibRow> = (0..g.rows.row_count()).filter_map(|i| g.rows.row_data(i)).collect();
                if let Some(i) = rows.iter().position(|r| r.chosen) {
                    ui.set_mouse_lib_reveal_y(y + i as f32 * crate::tokens::size::CONTROL);
                    ui.set_mouse_lib_reveal_n(ui.get_mouse_lib_reveal_n() + 1);
                    break;
                }
                y += rows.len() as f32 * crate::tokens::size::CONTROL;
            }
        }
        show_rows(ui.get_mouse_lib_groups(), |r| ui.set_mouse_lib_groups(r), groups);
        ui.set_mouse_chosen(m.button.is_some());
        if let Some(b) = m.button {
            let (cap, title, factory, state, rebound, kind, note) = m.detail(b);
            ui.set_mouse_cap(cap.into());
            ui.set_mouse_title(title.into());
            ui.set_mouse_factory(factory.into());
            ui.set_mouse_state(state.into());
            ui.set_mouse_rebound(rebound);
            ui.set_mouse_kind(kind as i32);
            show_rows(ui.get_mouse_kinds(), |r| ui.set_mouse_kinds(r), KINDS.iter().map(|(n, _)| SharedString::from(*n)).collect());
            show_rows(ui.get_mouse_kind_icons(), |r| ui.set_mouse_kind_icons(r), KINDS.iter().map(|(_, i)| SharedString::from(*i)).collect());
            ui.set_mouse_job(crate::mouse::job_label_with(&m.job(b), &m.names).into());
            ui.set_mouse_note(note.into());
        } else {
            let (note, any) = m.all_buttons();
            ui.set_mouse_all_note(note.into());
            ui.set_mouse_any_rebound(any);
        }
        // the effects' tiles, their pictures the app's own (described::tile), each drawn once
        let wide = pic_wide(self.ui.get_mouse_fx_w());
        if self.mouse_icons.as_ref().map(|(s, w, _)| (*s, *w)) != Some((self.scale, wide)) {
            self.mouse_icons = Some((self.scale, wide, Vec::new()));
        }
        let fx = self.mouse.effect();
        let known = self.mouse.light_known(self.mouse.light_name());
        let effects: Vec<EffectItem> = self
            .mouse
            .effects()
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let (tile, motion) = crate::mouse::tile_of(e);
                let icons = &mut self.mouse_icons.as_mut().unwrap().2;
                let icon = match icons.iter().find(|(t, _)| *t == tile) {
                    Some((_, icon)) => icon.clone(),
                    None => {
                        let icon = lighting::icon(tile, self.scale, wide);
                        icons.push((tile, icon.clone()));
                        icon
                    }
                };
                EffectItem { name: e["label"].as_str().unwrap_or("").into(), icon, on: i == fx && known, motion }
            })
            .collect();
        self.ui.set_mouse_effect_moves(effects.iter().any(|e| e.on && e.motion != 0));
        show_rows(self.ui.get_mouse_effects(), |r| self.ui.set_mouse_effects(r), effects);
        // (the color the picker is open on, ringed)
        let open = self.pick.as_ref().and_then(|p| match p.piece {
            Some(crate::described::Piece::Effect(k)) => Some((k, p.index)),
            _ => None,
        });
        // the tab's boxes, in the page's row of boxes, each laid out to the width the row made it;
        // on Settings, the form across the page
        let boxes = self.mouse.boxes(|i, least| self.mouse_boxes.width(i, least), open);
        self.mouse_boxes.show(boxes);
        if self.mouse.tab == 3 {
            let (cells, h) = self.mouse.cells(self.ui.get_mouse_w());
            self.ui.set_mouse_cells_h(h);
            self.show_cells(cells);
        }
        // the picture: its layers, the buttons it picks out, its lights now, and their effects playing
        self.mouse_draw();
        self.show_mouse_picked();
        self.show_mouse_lights();
        self.mouse_frames_run();
    }

    /// The menu on the mouse's profile in use: Duplicate, and Delete, which
    /// the only profile can't be.
    pub(super) fn open_profile_menu(&mut self, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let only = self.mouse.profiles.as_ref().is_none_or(|p| p.list.len() < 2);
        let row = |kind: i32, label: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() };
        let mut rows = vec![row(0, "Duplicate"), row(0, "Use with an app…"), row(2, ""), row(if only { 5 } else { 4 }, "Delete")];
        if only {
            rows.push(row(3, "Your only profile can’t be deleted."));
        }
        self.hang_menu(rows, 0.0, anchor);
        self.profile_menu = true;
    }

    /// Whether a press off the open menu shut it a moment ago, on what opens this one (so
    /// the press on its button doesn't open it again).
    pub(super) fn just_shut(&mut self, anchor: (f32, f32, f32)) -> bool {
        self.shut.take().is_some_and(|(at, when)| (at.0 - anchor.0).abs() < 2.0 && (at.1 - anchor.1).abs() < 2.0 && when.elapsed() < Duration::from_millis(800))
    }

    /// A menu of `rows` open from `anchor` (what opened it: its left, its top and its
    /// height; for one opened at the pointer, the point), as every menu opens
    /// (src/menu_size.rs): as wide as its rows need and never narrower than `under`, the
    /// field it drops from (none for a button's); under what opened it where it fits, over
    /// it where there's more room there.
    pub(super) fn hang_menu(&mut self, rows: Vec<MenuRow>, under: f32, anchor: (f32, f32, f32)) {
        self.hang_menu_most(rows, under, anchor, f32::MAX);
    }

    /// hang_menu, showing at most `most` of its rows' height before they scroll.
    pub(super) fn hang_menu_most(&mut self, mut rows: Vec<MenuRow>, under: f32, anchor: (f32, f32, f32), most: f32) {
        let w = crate::menu_size::width(&rows, self.ui.get_menu_marks(), under);
        crate::menu_size::fit_notes(&mut rows, w);
        let h = crate::menu_size::height(&rows, false, false, most);
        let (ax, ay, ah) = anchor;
        let win = self.ui.get_window_height();
        let (top, most) = crate::menu_size::hang(win, (ay, ah), h);
        // (turned to what's chosen, where that's out of view)
        let reveal = crate::menu_size::reveal(&rows, h.min(most) - 2.0 * crate::tokens::part::MENU_PAD);
        self.ui.set_menu_search(false);
        self.ui.set_menu_back(false);
        self.ui.set_menu_natural(h);
        self.ui.set_menu_w(w);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        self.ui.set_menu_ax(ax);
        self.ui.set_menu_ay(ay);
        self.ui.set_menu_ah(ah);
        self.ui.set_menu_top(top);
        self.ui.set_menu_max(most);
        self.ui.set_menu_open(true);
        self.ui.invoke_scroll_menu(reveal);
    }

    /// The menu on the keyboard's profile being edited: Use with an app.
    pub(super) fn open_kb_profile_menu(&mut self, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let row = |kind: i32, label: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() };
        self.hang_menu(vec![row(0, "Use with an app…")], 0.0, anchor);
        self.kb_profile_menu = true;
    }

    /// The apps a profile can be tied to (the keyboard's, `device` 0, by its
    /// number; the mouse's, 1, by its place): those seen in front, newest
    /// first, and any it's tied to that aren't, each ticked if it is.
    pub(super) fn open_apps_menu(&mut self, device: usize, profile: usize, anchor: (f32, f32, f32)) {
        self.close_menu();
        let tied = self.tied_apps(device, profile);
        let mut apps: Vec<(String, String)> = self.front_apps.list.iter().map(|a| (a.id.clone(), a.name.clone())).collect();
        for t in &tied {
            if !apps.iter().any(|(id, _)| id == t) {
                apps.push((t.clone(), self.front_apps.name(t)));
            }
        }
        let mut rows = vec![MenuRow { kind: 1, label: "Use it while one of these is in front".into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() }];
        rows.extend(apps.iter().map(|(id, name)| MenuRow { kind: 0, label: name.as_str().into(), tag: "".into(), tick: tied.contains(id), sub: "".into(), ..Default::default() }));
        if apps.is_empty() {
            rows.push(MenuRow { kind: 3, label: "Apps show here once they’ve been in front while Ectomatter is running.".into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() });
        }
        self.hang_menu(rows, 0.0, anchor);
        self.apps_menu = Some((device, profile, anchor));
    }

    /// A device's key in the Library (`device` 0 the keyboard, 1 the mouse),
    /// and a profile's id there: the keyboard's by its number, the mouse's by
    /// the id Ectomatter gave it (from its place in the list).
    pub(super) fn tie_key(&self, device: usize, profile: usize) -> Option<(&'static str, String)> {
        if device == 1 {
            return self.mouse.profiles.as_ref().and_then(|s| s.list.get(profile)).map(|p| (crate::mouse::key(), p.id.clone()));
        }
        Some((crate::cores::keyboard_key(), profile.to_string()))
    }

    /// The games and apps a profile is chosen for, as the Library keeps them
    /// (the keyboard's, `device` 0, by its number; the mouse's, 1, by its place).
    pub(super) fn tied_apps(&self, device: usize, profile: usize) -> Vec<String> {
        self.tie_key(device, profile).map(|(d, p)| self.games.library.tied(d, &p)).unwrap_or_default()
    }

    /// A profile tied to an app, or no longer: one profile of a device at a
    /// time for each app, the last it was tied to, kept in the Library with
    /// what each game switches.
    pub(super) fn tie_app(&mut self, device: usize, profile: usize, app: &str, on: bool) {
        let Some((d, p)) = self.tie_key(device, profile) else { return };
        let mine = self.games.library.choice(app, d) == Some(crate::games::Choice::Profile(p.clone()));
        if on {
            self.games.library.set_choice(app, d, Some(crate::games::Choice::Profile(p)));
        } else if mine {
            self.games.library.set_choice(app, d, None);
        }
    }

    /// The words under a profile's name for the apps it's tied to: "With
    /// Steam", or with how many more.
    pub(super) fn with_apps(&self, apps: &[String]) -> Option<String> {
        let first = self.games.name(apps.first()?, &self.front_apps);
        Some(match apps.len() {
            1 => format!("With {first}"),
            n => format!("With {first} and {} more", n - 1),
        })
    }

    /// Ties made before the Library kept them, taken in once and taken out of
    /// where they were: the keyboard's from Ectomatter's settings, the mouse's
    /// from its profiles' file (once the mouse's profiles have been read).
    pub(super) fn take_old_ties(&mut self) {
        // (moved only to a device whose id is known: its core described it)
        if !self.ties_taken[0] && !crate::cores::keyboard_key().is_empty() {
            self.ties_taken[0] = true;
            if let Some(all) = self.settings.get(&crate::cores::keyboard_setting("profile_apps")).and_then(|v| v.as_object().cloned()) {
                let ties: Vec<(String, Vec<String>)> = all.iter().map(|(n, a)| (n.clone(), a.as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect())).collect();
                self.games.library.take_ties(crate::cores::keyboard_key(), &ties);
                let _ = self.settings.set(&crate::cores::keyboard_setting("profile_apps"), serde_json::Value::Null);
            }
        }
        if !self.ties_taken[1] && !crate::mouse::key().is_empty() {
            if let Some(store) = &mut self.mouse.profiles {
                self.ties_taken[1] = true;
                let ties: Vec<(String, Vec<String>)> = store.list.iter().filter(|p| !p.apps.is_empty()).map(|p| (p.id.clone(), p.apps.clone())).collect();
                if !ties.is_empty() {
                    self.games.library.take_ties(crate::mouse::key(), &ties);
                    for p in &mut store.list {
                        p.apps.clear();
                    }
                    store.save();
                }
            }
        }
    }

    /// The devices a Library page lists: each one met, by its key, its name
    /// and its profiles (the keyboard's by their numbers, named as the
    /// keyboard names them; the mouse's as Ectomatter keeps them).
    pub(super) fn lib_devices(&self) -> Vec<crate::games_page::Device> {
        let mut out = Vec::new();
        for (place, name, kind, _) in self.devices_in_order() {
            match place.as_str() {
                "keyboard" => {
                    let known = self.page.dev.is_some();
                    let profiles = crate::cores::profiles().iter().map(|n| (n.to_string(), if known { self.page.profile_name(*n) } else { format!("Profile {n}") })).collect();
                    out.push(crate::games_page::Device { key: crate::cores::keyboard_key().into(), name, kind, profiles, glyph: None, sub: String::new(), chips: Vec::new() });
                }
                "mouse" => {
                    if let Some(s) = &self.mouse.profiles {
                        out.push(crate::games_page::Device { key: crate::mouse::key().into(), name, kind, profiles: s.list.iter().map(|p| (p.id.clone(), p.name.clone())).collect(), glyph: None, sub: String::new(), chips: Vec::new() });
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// The devices the packages Ectomatter runs itself brought in (the OpenRGB
    /// plugin's), as a Library page lists them: each by its own id, its
    /// profiles its package's, with its package's glyph.
    pub(super) fn hosted_devices(&self) -> Vec<crate::games_page::Device> {
        self.hosted
            .devices()
            .into_iter()
            .map(|(core, d)| {
                let profiles = d["capabilities"]["profiles"]["list"].as_array().into_iter().flatten().filter_map(|p| Some((p["id"].as_str()?.to_string(), p["name"].as_str().unwrap_or("").to_string()))).collect();
                let glyph = self.packages.iter().position(|p| p.id() == core);
                let sub = match d["sub"].as_str().unwrap_or("") {
                    "" => format!("Through {}", self.hosted.get(&core).map_or(core.clone(), |p| p.package.name().to_string())),
                    s => s.to_string(),
                };
                crate::games_page::Device { key: d["device"].as_str().unwrap_or("").to_string(), name: d["name"].as_str().unwrap_or("").to_string(), kind: d["kind"].as_str().unwrap_or("").to_string(), profiles, glyph, sub, chips: Vec::new() }
            })
            .collect()
    }

    /// The Library's page as it is now, with each plugin's section for the page open.
    pub(super) fn show_library(&mut self) {
        // (offscreen, what's known read at once: a snapshot has no later turn to take it on)
        if crate::desk::offscreen() && self.known.is_none() && self.hosted.launcher().is_some() {
            self.ask_known();
        }
        // the scene first, a row like a device's: No scene and every scene, each with its chip
        let chips = self.scene_chips();
        self.ui.set_scene_chips(ModelRc::new(VecModel::from(chips.clone())));
        let profiles = std::iter::once(("none".to_string(), "No scene".to_string())).chain(self.scenes().into_iter().map(|s| (s.id, s.name))).collect();
        let mut devices = vec![crate::games_page::Device { key: "scene".into(), name: "Scene".into(), kind: "scene".into(), profiles, glyph: None, sub: String::new(), chips }];
        devices.extend(self.lib_devices());
        devices.extend(self.hosted_devices());
        let sections = match self.games.open.clone() {
            Some(id) => {
                let kind = if id == crate::games::EVERY { "every" } else if self.games.library.is_game(&id) { "game" } else { "app" };
                let name = self.games.name(&id, &self.front_apps);
                // each plugin given the page's values; the one that says how games start, what's
                // known too (its section waits for that, the moment it takes to read)
                let launcher = self.hosted.launcher().map(String::from);
                let facts = self.launch_facts(&id);
                let lib = &self.games.library;
                let more = |p: &crate::packages::Package| {
                    let mut m = serde_json::json!({ "values": lib.wanted_values(&id, p.id()), "own": lib.plugin_values(&id, p.id()), "every": lib.plugin_values(crate::games::EVERY, p.id()) });
                    if launcher.as_deref() == Some(p.id()) {
                        m["facts"] = facts.clone();
                    }
                    m
                };
                let described = self.hosted.sections(kind, &name, &more);
                described
                    .into_iter()
                    .filter(|(plugin, _)| launcher.as_deref() != Some(plugin.as_str()) || !facts.is_null())
                    .map(|(plugin, desc)| crate::games_page::Section { name: self.hosted.get(&plugin).map(|p| p.package.name().to_string()).unwrap_or_default(), plugin, desc })
                    .collect()
            }
            None => Vec::new(),
        };
        self.games.show(&self.ui, &devices, sections, self.scale, &self.front_apps);
    }

    /// One of the Library's menus opened: the store's, the order's, or a
    /// device's on a page, under what opened it.
    pub(super) fn open_games_menu(&mut self, m: crate::games_page::Menu, anchor: (f32, f32, f32), under: f32) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let counts = self.games.counts(&self.front_apps);
        let rows = self.games.menu_rows(&m, counts);
        self.hang_menu(rows, under, anchor);
        self.games.menu = Some(m);
    }

    /// The packages, read again, and each one's glyph given to the window, at
    /// a device's row's size in the Library and a tile's in Settings.
    pub(super) fn give_packages(&mut self) {
        self.packages = crate::packages::all();
        let at = |px: u32| -> Vec<slint::Image> { self.packages.iter().map(|p| p.glyph().map(|g| crate::package_glyph(&g, px)).unwrap_or_default()).collect() };
        let (small, tile) = (at(22), at(25));
        self.ui.set_package_glyphs(ModelRc::new(VecModel::from(small)));
        self.ui.set_package_tiles(ModelRc::new(VecModel::from(tile)));
    }

    /// Settings' About: a Read opened, or what went wrong with it said under its row.
    pub(super) fn about_tick(&mut self) -> bool {
        let Some((which, rx)) = &self.about_read else { return false };
        let said = match rx.try_recv() {
            Ok(r) => r.err(),
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => Some("It didn’t open.".to_string()),
        };
        if let Some(e) = &said {
            eprintln!("ectomatter: About's Read: {e}");
        }
        self.about_said = said.map(|e| (*which, e));
        self.about_read = None;
        if self.ui.get_place() == "settings" {
            self.refresh();
        }
        true
    }

    /// Settings' Cores or Plugins: each package of that kind's row, by its place among all.
    pub(super) fn package_rows(&self, kind: crate::packages::Kind) -> Vec<crate::ecto_settings::PackageRow> {
        self.packages
            .iter()
            .enumerate()
            .filter(|(_, p)| p.kind() == kind)
            .map(|(i, p)| {
                let on = self.settings.on(&p.switch());
                let running = self.hosted.get(p.id());
                // (one that names no repository says nothing more, but that it was added from a file)
                let status = if p.repository().is_none() { if p.added { "Added from a file".to_string() } else { String::new() } } else { crate::updates::status_of(p) };
                crate::ecto_settings::PackageRow {
                    name: p.name().to_string(),
                    description: p.description(),
                    can: p.can(),
                    version: p.version().to_string(),
                    status,
                    on,
                    warning: running.and_then(|r| r.warning.clone()).or_else(|| self.hosted.refused(p.id()).map(String::from)).filter(|_| on),
                    glyph: i,
                    removable: p.added,
                    review: crate::updates::waiting(p).is_some(),
                    systems: p.systems().iter().map(|s| s.to_string()).collect(),
                }
            })
            .collect()
    }

    /// Settings' Devices: each core with its device as the system shows it
    /// (looked at every two seconds at most), what it's called in words about
    /// it, and what another program having it open means for it.
    pub(super) fn device_rows(&mut self) -> Vec<crate::ecto_settings::DeviceRow> {
        let ids: Vec<(String, Option<(u16, u16)>, serde_json::Value)> = self.packages.iter().filter(|p| p.kind() == crate::packages::Kind::Core).map(|p| (p.id().to_string(), p.manifest.usb_ids(), p.manifest.json())).collect();
        let rows = self.package_rows(crate::packages::Kind::Core);
        rows.into_iter()
            .zip(ids)
            .map(|(core, (id, usb, manifest))| {
                let st = match usb {
                    Some((vendor, product)) => {
                        if self.device_status.get(&id).is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(2)) {
                            let mut st = crate::devices::usb(vendor, product);
                            // (and whether this computer has a rule that lets it be opened)
                            st["ruled"] = serde_json::json!(crate::permission::missing(&manifest).is_empty());
                            self.device_status.insert(id.clone(), (Instant::now(), st));
                        }
                        self.device_status[&id].1.clone()
                    }
                    None => serde_json::Value::Null,
                };
                let kind = self.device_kind(&id);
                let holders_note = if kind == "keyboard" {
                    "Two programs sending lighting at once make the keys flicker. Close the other program, or turn Live lighting off here.".to_string()
                } else {
                    format!("Programs that both set the {kind} undo each other’s changes: whichever sets its lights or buttons last wins. Ectomatter changes the {kind} only when you do, here.")
                };
                crate::ecto_settings::DeviceRow {
                    core,
                    present: st["present"].as_bool().unwrap_or(false),
                    reachable: st["reachable"].as_bool(),
                    // (one Ectomatter opens already needs no rule, whatever its rule is called)
                    allow: st["ruled"] == serde_json::json!(false) && st["reachable"] != serde_json::json!(true) && self.granting.is_none(),
                    // (each program once, in the same order every time)
                    holders: st["holders"].as_array().map(|a| a.iter().filter_map(|h| h["name"].as_str().map(String::from)).collect::<std::collections::BTreeSet<_>>().into_iter().collect()).unwrap_or_default(),
                    holders_note,
                    kind,
                }
            })
            .collect()
    }

    /// What a core's device is called in words about it ("keyboard"), as its
    /// own description says: as it describes it before it's met, or as it
    /// told it once it was.
    pub(super) fn device_kind(&self, id: &str) -> String {
        let told = self.hosted.get(id).and_then(|r| r.describes["kind"].as_str().map(String::from)).or_else(|| self.hosted.devices().into_iter().find(|(p, _)| p == id).and_then(|(_, d)| d["kind"].as_str().map(String::from)));
        told.unwrap_or_else(|| "device".into())
    }

    /// Each running plugin with a Settings section of its own handed all the
    /// values Ectomatter keeps for it (`only` the one named, where one is), as
    /// it starts and as one changes; handed, not waited on.
    pub(super) fn hand_values(&self, only: Option<&str>) {
        for r in &self.hosted.plugins {
            let id = r.package.id();
            let has = r.package.manifest.spec.adds(ectomatter_protocol::manifest::Adds::Settings);
            if !has || only.is_some_and(|o| o != id) {
                continue;
            }
            let values = self.settings.get(&format!("plugins.{id}")).filter(serde_json::Value::is_object).unwrap_or_else(|| serde_json::json!({}));
            r.core.tell("settings/set", &serde_json::json!({ "values": values }));
        }
    }

    /// A package started, and if it's a plugin with a Settings section of its
    /// own, handed its values.
    pub(super) fn start_package(&mut self, p: crate::packages::Package) {
        let id = p.id().to_string();
        self.hosted.start(p);
        self.hand_values(Some(&id));
    }

    /// One of a plugin's own settings set, kept, and handed to the plugin.
    pub(super) fn set_plugin_value(&mut self, plugin: &str, setting: &str, v: serde_json::Value) {
        if let Err(e) = self.settings.set(&format!("plugins.{plugin}.{setting}"), v) {
            self.settings_say(format!("The setting was not saved: {e}"), true);
            return;
        }
        self.hand_values(Some(plugin));
        self.refresh();
    }

    /// The plugins' own Settings sections, each with the values Ectomatter
    /// keeps for it and what's being typed into one of its boxes.
    pub(super) fn plugin_sections(&self) -> Vec<crate::ecto_settings::PluginSection> {
        self.hosted
            .settings_sections()
            .into_iter()
            .map(|(plugin, section)| {
                let values = self.settings.get(&format!("plugins.{plugin}")).unwrap_or(serde_json::Value::Null);
                let typed = self.typing.as_ref().filter(|(p, _, _)| *p == plugin).map(|(_, s, t)| (s.clone(), t.clone()));
                crate::ecto_settings::PluginSection { plugin, section, values, typed }
            })
            .collect()
    }

    /// The weather outside, as the Weather Feed last told it: in °C, and
    /// the sky in words.
    pub(super) fn outside(&self) -> Option<(f64, String)> {
        let r = self.hosted.readings();
        // (whichever plugin tells readings of these kinds)
        let of = |kind: &str| r.of_kind(kind).and_then(|(p, id)| r.value(&p, &id));
        Some((of(crate::lcd_page::OUTSIDE_TEMPERATURE)?.as_f64()?, of(crate::lcd_page::SKY)?.as_str()?.to_string()))
    }

    /// Add a core or Add a plugin, from Settings: its form.
    pub(super) fn add_package_form(&mut self, kind: crate::packages::Kind) {
        self.package_form = Some(PackageForm { kind, repo: None, file: None, found: None, why: None, typed_at: None, looked_for: None, looking: None, picking: None, adding: None });
        self.ui.set_gl_plugin_address("".into());
        self.show_package_form();
    }

    /// The form as it stands: what was found, and whether it can be added.
    pub(super) fn show_package_form(&mut self) {
        let Some(f) = &self.package_form else { return };
        self.ui.set_gl_modal(4);
        let word = f.kind.name();
        self.ui.set_gl_plugin_title(format!("Add a {word}").into());
        self.ui.set_gl_plugin_sub(format!("From the {word}’s GitHub releases, or from its file. Nobody at Ectonomic has reviewed a {word} added this way; Ectomatter says what the {word} can reach before you add it.").into());
        // (the file by its name: the folder it's in says less than its name)
        self.ui.set_gl_plugin_file(f.file.as_ref().map(|p| p.file_name().map_or_else(|| p.to_string_lossy().to_string(), |n| n.to_string_lossy().to_string())).unwrap_or_default().into());
        let (mut can_add, mut state, mut bad) = (false, String::new(), false);
        match &f.found {
            Some(m) => {
                let name = m["name"].as_str().unwrap_or("This package");
                let version = m["version"].as_str().unwrap_or("");
                // (a core found from Add a plugin, or the other way round, goes where its kind goes)
                let its = crate::packages::Kind::of(m);
                self.ui.set_gl_plugin_found(format!("{name} {version}").trim().into());
                self.ui.set_gl_plugin_found_sub(m["description"].as_str().unwrap_or("").into());
                self.ui.set_gl_plugin_can(crate::packages::can_words(m).join("\n").into());
                let id = m["id"].as_str().unwrap_or("");
                let have = self.packages.iter().find(|p| p.id() == id);
                if let Err(why) = crate::updates::runs_here(m) {
                    (state, bad) = (format!("This Ectomatter can’t use it: {why}."), true);
                } else if let Some(h) = have.filter(|h| crate::packages::version_key(h.version()) >= crate::packages::version_key(version)) {
                    state = format!("Ectomatter already has {} {}.", h.name(), h.version());
                } else if f.adding.is_some() {
                    state = "Adding…".into();
                } else {
                    can_add = true;
                    if its != f.kind {
                        state = match its {
                            crate::packages::Kind::Core => format!("{name} is a core, for a device, so it goes under Devices."),
                            crate::packages::Kind::Plugin => format!("{name} is a plugin, so it goes under Plugins."),
                        };
                    }
                }
            }
            None => {
                self.ui.set_gl_plugin_found("".into());
                self.ui.set_gl_plugin_found_sub("".into());
                self.ui.set_gl_plugin_can("".into());
                if let Some(why) = &f.why {
                    (state, bad) = (why.clone(), true);
                } else if f.looking.is_some() {
                    state = format!("Looking for {} on GitHub…", f.looked_for.clone().unwrap_or_default());
                }
            }
        }
        self.ui.set_gl_plugin_state(state.into());
        self.ui.set_gl_plugin_bad(bad);
        self.ui.set_gl_can_add(can_add);
    }

    /// What came of the form's looking: an address looked up once its typing
    /// rests, a file read once it's chosen, a plugin put in place.
    pub(super) fn package_form_tick(&mut self) {
        let typed = self.ui.get_gl_plugin_address().to_string();
        let Some(f) = &mut self.package_form else { return };
        let mut changed = false;
        if f.typed_at.is_some_and(|t| t.elapsed() >= Duration::from_millis(500)) {
            f.typed_at = None;
            let repo = crate::packages::repository_of(&typed);
            if repo != f.looked_for {
                f.found = None;
                f.why = None;
                f.looked_for = repo.clone();
                f.repo = repo.clone();
                f.looking = repo.map(|r| {
                    let (tx, rx) = mpsc::channel();
                    let _ = std::thread::Builder::new().name("plugin look".into()).spawn(move || {
                        let _ = tx.send(crate::updates::release_manifest(&r));
                    });
                    rx
                });
                if f.looking.is_none() && !typed.trim().is_empty() {
                    f.why = Some("That isn’t a GitHub address. It looks like github.com/owner/name.".into());
                }
                changed = true;
            }
        }
        if let Some(rx) = &f.looking {
            if let Ok(r) = rx.try_recv() {
                f.looking = None;
                match r {
                    Ok(m) => f.found = Some(m),
                    Err(e) => f.why = Some(format!("No plugin there: {e}.")),
                }
                changed = true;
            }
        }
        if let Some(rx) = &f.picking {
            if let Ok(chosen) = rx.try_recv() {
                f.picking = None;
                if let Some(p) = chosen {
                    f.repo = None;
                    f.looked_for = None;
                    match crate::packages::manifest_in_file(&p) {
                        Ok(m) => {
                            f.found = Some(m);
                            f.why = None;
                        }
                        Err(e) => {
                            f.found = None;
                            f.why = Some(e);
                        }
                    }
                    f.file = Some(p);
                    self.ui.set_gl_plugin_address("".into());
                }
                changed = true;
            }
        }
        if let Some(rx) = &f.adding {
            if let Ok(r) = rx.try_recv() {
                f.adding = None;
                match r {
                    Ok(m) => {
                        self.package_form = None;
                        self.ui.set_gl_modal(0);
                        self.package_added(&m.id, &m.name);
                        return;
                    }
                    Err(e) => f.why = Some(format!("It couldn’t be added: {e}.")),
                }
                changed = true;
            }
        }
        if changed {
            self.show_package_form();
        }
    }

    /// Add, in Add a plugin: the plugin put in place (from GitHub on a thread
    /// of its own; from a file at once), then started.
    pub(super) fn package_add_done(&mut self) {
        let Some(f) = &mut self.package_form else { return };
        let Some(m) = f.found.clone() else { return };
        let id = m["id"].as_str().unwrap_or("").to_string();
        if let Some(repo) = f.repo.clone() {
            let (tx, rx) = mpsc::channel();
            let _ = std::thread::Builder::new().name("plugin add".into()).spawn(move || {
                let _ = tx.send(crate::updates::install_latest(&repo, &id, &m, &crate::packages::added_dir(crate::packages::Kind::of(&m))));
            });
            f.adding = Some(rx);
            self.show_package_form();
            return;
        }
        if let Some(file) = f.file.clone() {
            match crate::packages::install(&file) {
                Ok(m) => {
                    self.package_form = None;
                    self.ui.set_gl_modal(0);
                    self.package_added(&m.id, &m.name);
                }
                Err(e) => {
                    f.why = Some(format!("It couldn’t be added: {e}"));
                    self.show_package_form();
                }
            }
        }
    }

    /// A plugin added: in Settings' list, and started.
    pub(super) fn package_added(&mut self, id: &str, name: &str) {
        self.give_packages();
        if let Some(p) = self.packages.iter().find(|p| p.id() == id).cloned() {
            // (a core runs the new version from Ectomatter's next start, with its device's page)
            if p.kind() == crate::packages::Kind::Core && self.hosted.get(id).is_some() {
                self.restart = Some("settings".into());
            } else if self.settings.on(&p.switch()) {
                self.hosted.stop(id);
                self.start_package(p);
            }
        }
        self.settings_say(format!("Added {name}."), false);
        // (a core's devices this computer has no rule for: the system's password prompt, once)
        if let Some(m) = self.packages.iter().find(|p| p.id() == id && p.kind() == crate::packages::Kind::Core).map(|p| p.manifest.json()) {
            self.allow_devices(&m);
        }
        self.refresh();
    }

    /// An added plugin taken out: what it changed put back, stopped, its folder
    /// gone; one that came with Ectomatter too is used again in its place.
    pub(super) fn remove_package(&mut self, id: &str) {
        let name = self.packages.iter().find(|p| p.id() == id).map(|p| p.name().to_string()).unwrap_or_else(|| id.to_string());
        let kind = self.packages.iter().find(|p| p.id() == id).map_or(crate::packages::Kind::Plugin, |p| p.kind());
        // (a core's device page keeps driving it until Ectomatter starts again, below)
        if kind == crate::packages::Kind::Plugin {
            self.worker.drop_plugin(id);
            self.hosted.stop(id);
            self.following.retain(|k, _| !k.starts_with(&format!("{id}/")));
            self.plugin_applied.retain(|k, _| !k.starts_with(&format!("{id}/")));
        }
        let root = crate::packages::added_dir(kind);
        let dir = root.join(id);
        if dir.starts_with(&root) && dir != root {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                self.settings_say(format!("{name} wasn’t removed: {e}"), true);
                return;
            }
        }
        self.give_packages();
        // (a core: Ectomatter starts again, its device's page with the one that comes with it, if one does)
        if kind == crate::packages::Kind::Core {
            self.restart = Some("settings".into());
        } else if let Some(p) = self.packages.iter().find(|p| p.id() == id).cloned() {
            if self.settings.on(&p.switch()) {
                self.start_package(p);
            }
        }
        self.hosted.forget_sections();
        self.settings_say(format!("Removed {name}."), false);
        self.refresh();
    }

    /// A package turned on or off in Settings: the keyboard's core is the
    /// keyboard's Control, and every core starts again with Ectomatter, its
    /// device's page with it; a plugin starts or stops at once, what it
    /// changed for a game running put back first.
    pub(super) fn package_switch(&mut self, row: usize, on: bool) {
        let Some(p) = self.packages.get(row).cloned() else { return };
        if crate::cores::keyboard().is_some_and(|k| k.id == p.id()) {
            self.cell_pick(Ctl::Control, if on { 0 } else { 1 }, (0.0, 0.0, 0.0), 0.0);
            return;
        }
        if self.settings.on(&p.switch()) == on {
            return;
        }
        if let Err(e) = self.settings.set(&p.switch(), serde_json::json!(on)) {
            self.settings_say(format!("The setting was not saved: {e}"), true);
            return;
        }
        if p.kind() == crate::packages::Kind::Core {
            self.restart = Some("settings".into());
            return;
        }
        if on {
            self.start_package(p);
        } else {
            self.worker.drop_plugin(p.id());
            self.hosted.stop(p.id());
            // (its devices let go of what they followed)
            self.following.retain(|k, _| !k.starts_with(&format!("{}/", p.id())));
            self.plugin_applied.retain(|k, _| !k.starts_with(&format!("{}/", p.id())));
        }
        self.hosted.forget_sections();
    }

    /// An app that doesn't run through Steam, as its windows open and close:
    /// its plugins' changes made as its first window opens (where its page
    /// has any), and put back as its last closes. Games, and Steam's apps,
    /// go by the Library's watch instead.
    pub(super) fn app_windows(&mut self, f: &crate::front_app::Front, app: &str) {
        if f.window.is_empty() || f.is_ectomatter() {
            return;
        }
        if f.closed {
            if self.windows.remove(&f.window).is_some() && !self.windows.values().any(|a| a == app) && self.app_sessions.remove(app) {
                eprintln!("ectomatter: {} closed", self.games.name(app, &self.front_apps));
                self.worker.quit_game(app);
            }
            return;
        }
        self.windows.insert(f.window.clone(), app.to_string());
        let watched = self.games.running.iter().any(|r| r.id == app) || crate::games::steam_number(app).is_some() || self.games.library.is_game(app);
        if watched || self.app_sessions.contains(app) {
            return;
        }
        let name = self.games.name(app, &self.front_apps);
        let r = crate::games::Running { id: app.to_string(), pid: f.pid, since: crate::games::now() };
        let plan = self.game_plan(&r, &name);
        if !plan.changes.is_empty() {
            eprintln!("ectomatter: {name} opened");
            self.app_sessions.insert(app.to_string());
            self.worker.start_game(plan);
        }
    }

    /// What the plugins change as a game or app starts, for the put-back
    /// thread to make: each plugin's changes for it from its choices (its own
    /// over Every game's) and the person's commands for it.
    pub(super) fn game_plan(&self, r: &crate::games::Running, name: &str) -> crate::putback::Plan {
        let lib = &self.games.library;
        let kind = if lib.is_game(&r.id) { "game" } else { "app" };
        let scripts = lib.scripts_for(&r.id);
        let game = crate::putback::Game { id: r.id.clone(), name: name.to_string(), pid: r.pid, since: r.since };
        let values = |plugin: &str| lib.wanted_values(&r.id, plugin);
        let mut changes: Vec<(String, serde_json::Value, String, String, Option<crate::commands::Script>)> = self.hosted.changes(&game, kind, &values).into_iter().map(|(plugin, what, label, words)| (plugin, what, label, words, None)).collect();
        // then the person's own commands, Ectomatter's Custom Scripts: Every game's first, so
        // their undos come last as the game quits
        for (s, _) in scripts {
            let label: String = if s.name.trim().is_empty() { "SCRIPT".to_string() } else { s.name.to_uppercase() }.chars().take(20).collect();
            changes.push((CUSTOM_SCRIPTS.to_string(), serde_json::json!({ "command": s.id }), label, s.name.clone(), Some(s)));
        }
        crate::putback::Plan { game, changes, seq: PLANS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) }
    }
}
