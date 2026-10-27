//! The scene form: a new scene, or one of the person's own changed, shown on the devices as it's set.

use super::*;

impl Front {
    /// The scene form opened over the page: New scene, or one of the person's
    /// own scenes to change. The devices show the scene as it's set.
    pub fn open_scene_form(&mut self, editing: Option<String>) {
        use crate::scenes::{Effect, Lights, Look, Part};
        self.close_menu();
        self.close_picker();
        let start = Part::Look(Look::new(Effect::Static));
        let keys = [crate::cores::keyboard_key(), crate::mouse::key()];
        let mut form = SceneForm { editing: None, name: String::new(), desk: true, look: Look::new(Effect::Wave), parts: keys.iter().map(|k| (k.to_string(), start.clone())).collect(), plugins: Vec::new() };
        if let Some(sc) = editing.and_then(|id| self.scenes().into_iter().find(|s| s.id == id && !s.built_in)) {
            form.editing = Some(sc.id.clone());
            form.name = sc.name.clone();
            match &sc.lights {
                Lights::Desk(l) => form.look = l.clone(),
                Lights::Each(_) => {
                    form.desk = false;
                    form.parts = keys.iter().map(|k| (k.to_string(), sc.lights.part(k))).collect();
                }
            }
            form.plugins = sc.plugins.clone();
        }
        self.scene_form = Some(form);
        self.ui.set_scene_form(true);
        self.show_scene_form();
        self.scene_play();
    }

    /// The scene form closed: Cancel (the scene on plays again), or Create
    /// scene (kept, and on) or Save (kept; on again if it was).
    pub(super) fn close_scene_form(&mut self, keep: bool) {
        let Some(form) = self.scene_form.take() else { return };
        self.close_menu();
        self.close_picker();
        self.ui.set_scene_form(false);
        // (the Name box's typing let go of: it goes without being left)
        slint::ComponentHandle::global::<crate::Typing>(&self.ui).set_on(false);
        if keep {
            let all = self.scenes();
            let mut sc = form.scene();
            let before = form.editing.as_ref().and_then(|id| all.iter().find(|s| &s.id == id)).map(|s| s.name.clone());
            sc.name = match (form.name.trim(), before) {
                ("", Some(was)) => was,
                ("", None) => (1..).map(|n| format!("Scene {n}")).find(|n| !all.iter().any(|s| &s.name == n)).unwrap_or_default(),
                (n, _) => n.to_string(),
            };
            let mut kept: Vec<serde_json::Value> = self.settings.get("scenes").and_then(|v| v.as_array().cloned()).unwrap_or_default();
            match &form.editing {
                Some(id) => {
                    if let Some(v) = kept.iter_mut().find(|v| v["id"].as_str() == Some(id)) {
                        *v = crate::scenes::to_json(&sc);
                    }
                }
                None => {
                    // (an id never used before, not even by a scene since deleted)
                    let made = self.settings.get("scenes_made").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    sc.id = crate::scenes::new_id(&all, made);
                    let _ = self.settings.set("scenes_made", serde_json::json!(made.max(crate::scenes::id_number(&sc.id)) as u64));
                    kept.push(crate::scenes::to_json(&sc));
                }
            }
            if let Err(err) = self.settings.set("scenes", serde_json::Value::Array(kept)) {
                self.status(format!("The scene was not saved: {err}"), 2);
            }
            if form.editing.is_none() {
                self.set_scene(Some(sc.id), true);
            } else {
                self.scene_play();
            }
        } else {
            self.scene_play();
        }
        self.refresh();
    }

    /// The scene being changed deleted: the games and apps that turned it on
    /// go back to Every game's choice (or none), and it's off if it was on.
    pub(super) fn delete_scene(&mut self) {
        let Some(id) = self.scene_form.as_ref().and_then(|f| f.editing.clone()) else { return };
        self.scene_form = None;
        self.close_menu();
        self.close_picker();
        self.ui.set_scene_form(false);
        slint::ComponentHandle::global::<crate::Typing>(&self.ui).set_on(false);
        let mut kept: Vec<serde_json::Value> = self.settings.get("scenes").and_then(|v| v.as_array().cloned()).unwrap_or_default();
        kept.retain(|v| v["id"].as_str() != Some(id.as_str()));
        let _ = self.settings.set("scenes", serde_json::Value::Array(kept));
        self.games.library.forget_scene(&id);
        if self.scene_back.as_ref().is_some_and(|b| b.as_deref() == Some(id.as_str())) {
            self.scene_back = Some(None);
        }
        if self.scene.as_deref() == Some(id.as_str()) {
            match self.scene_back.take() {
                // (a game or app had it on: the scene before it, as when that app leaves the front)
                Some(back) => self.set_scene(back, false),
                None => self.set_scene(None, true),
            }
        } else {
            self.scene_play();
        }
        self.refresh();
    }

    /// What the form says under its title: what a scene is for, or, changing
    /// one that games or apps turn on, which (so deleting it surprises none).
    pub(super) fn scene_form_sub(&self) -> String {
        let plain = "Light the whole desk with one effect, or give each device its own.".to_string();
        let Some(id) = self.scene_form.as_ref().and_then(|f| f.editing.clone()) else { return plain };
        let tied = self.games.library.tied("scene", &id);
        let names: Vec<String> = tied.iter().map(|a| if a == crate::games::EVERY { "Every game".to_string() } else { self.games.name(a, &self.front_apps) }).collect();
        match names.as_slice() {
            [] => plain,
            [one] if tied[0] == crate::games::EVERY => format!("{one} turns this scene on while a game is in front."),
            [one] if self.games.library.is_game(&tied[0]) => format!("{one} turns this scene on while the game is in front."),
            [one] => format!("{one} turns this scene on while the app is in front."),
            [rest @ .., last] => format!("{} and {last} turn this scene on while they’re in front.", rest.join(", ")),
        }
    }

    /// The devices the form gives a part each, as Home's desk has them, left
    /// to right: each one's key, name and glyph's style.
    pub(super) fn scene_form_devices(&self) -> Vec<(String, String, i32)> {
        let named: HashMap<String, String> = self.devices_in_order().into_iter().map(|(place, name, ..)| (place, name)).collect();
        self.scene_spans()
            .into_iter()
            .map(|sp| {
                let (place, style) = if sp.key == crate::cores::keyboard_key() { ("keyboard", 0) } else { ("mouse", 1) };
                let name = named.get(place).cloned().unwrap_or_else(|| if place == "keyboard" { "Keyboard".to_string() } else { "Mouse".to_string() });
                (sp.key, name, style)
            })
            .collect()
    }

    /// The plugins' devices a scene can give a profile (OpenRGB's): each one's
    /// id, name, glyph's style and profiles (by id and name).
    pub(super) fn scene_form_plugins(&self) -> Vec<(String, String, i32, Vec<(String, String)>)> {
        self.hosted_devices().into_iter().filter(|d| !d.profiles.is_empty()).map(|d| (d.key, d.name, d.glyph.map_or(0, |g| 2 + g as i32), d.profiles)).collect()
    }

    /// The form drawn as it is: its title and words, its pieces (the cells
    /// under the preview), the effect tiles and the preview itself.
    pub(super) fn show_scene_form(&mut self) {
        use crate::lighting::{cell, id, kind, style, text_cell};
        use crate::scenes::{Effect, Part};
        use crate::tokens::{line, part, size, space};
        let Some(form) = &self.scene_form else { return };
        // (the dialog's room inside its padding; a stacked form: each label over what it
        // labels, Space.label between, Space.pad between one and the next; two columns of
        // it Space.margin apart)
        const W: f32 = size::DIALOG - 2.0 * space::MARGIN;
        const CTRL: f32 = size::CONTROL;
        const HALF: f32 = (W - space::MARGIN) / 2.0;
        // (a device's row: its effect's dropdown, its colors after it)
        const PART_DROP: f32 = 190.0;
        let mut cells: Vec<Cell> = Vec::new();
        let mut y = 0.0;
        // the preview's words: each device under its part of the strip
        let spans = self.scene_spans();
        let devices = self.scene_form_devices();
        let room = W - part::SPAN_GAP * spans.len().saturating_sub(1) as f32;
        let mut x = 0.0;
        for (share, (_, name, _)) in crate::scenes::shares(&spans).into_iter().zip(&devices) {
            let w = share * room;
            cells.push(text_cell(style::NOTE, x, y, w - 2.0 * part::SPAN_GAP, line::NOTE, name));
            x += w + part::SPAN_GAP;
        }
        y += line::NOTE + space::PAD;
        // Name
        cells.push(text_cell(style::LABEL, 0.0, y, W, line::TEXT, "Name"));
        y += line::TEXT + space::LABEL;
        cells.push(Cell { text: form.name.as_str().into(), fill: "Name this scene".into(), style: 9, id: id(Ctl::SceneName), ..cell(kind::FIELD, 0.0, y, W, CTRL) });
        y += CTRL + space::PAD;
        // Lights: the whole desk, or each device
        cells.push(text_cell(style::LABEL, 0.0, y, W, line::TEXT, "Lights"));
        y += line::TEXT + space::LABEL;
        let seg = |cells: &mut Vec<Cell>, x0: f32, y: f32, c: Ctl, words: &[&str], on: usize| {
            let opts: Vec<(&str, &str)> = words.iter().map(|w| (*w, "")).collect();
            crate::form::switch(cells, x0, y, None, &opts, Some(on), |j| (id(c), j as i32, 0));
        };
        seg(&mut cells, 0.0, y, Ctl::SceneLights, &["Whole desk", "Each device"], if form.desk { 0 } else { 1 });
        y += CTRL + space::TIGHT;
        let order: Vec<&str> = devices.iter().map(|(_, _, glyph)| if *glyph == 0 { "the keyboard" } else { "the mouse" }).collect();
        let note = if form.desk {
            match order.as_slice() {
                [a, b] => format!("One effect crosses {a}, then {b}, in the order they sit on Home."),
                _ => "One effect crosses the whole desk, in the order the devices sit on Home.".to_string(),
            }
        } else {
            "Each device plays an effect of its own. Leave a device as it is to keep its profile’s lighting.".to_string()
        };
        y = crate::form::note_lines(&mut cells, 0.0, y, W, style::NOTE, &note) + space::PAD;
        // the colors as dots, Space.inline apart, each one's code Space.tight after it and
        // Space.row before the next, as a box's colors are
        let dots = |cells: &mut Vec<Cell>, x0: f32, y: f32, colors: &[[u8; 3]], base: usize, codes: bool| -> f32 {
            let mut x = x0;
            for (k, c) in colors.iter().enumerate() {
                cells.push(Cell { color: crate::tokens::color(*c), id: id(Ctl::SceneColor), index: (base + k) as i32, ..cell(kind::COLOR_DOT, x, y + (CTRL - size::SWATCH) / 2.0, size::SWATCH, size::SWATCH) });
                if codes {
                    let hex = lighting::hex_of(*c);
                    let w = crate::text::width(crate::tokens::font::MEDIUM, crate::tokens::font::CAPTION, &hex).ceil() + 2.0;
                    cells.push(text_cell(style::CODE, x + size::SWATCH + space::TIGHT, y, w, CTRL, &hex));
                    x += size::SWATCH + space::TIGHT + w + space::ROW;
                } else {
                    x += size::SWATCH + space::INLINE;
                }
            }
            x
        };
        let mut tiles_y = 0.0;
        if form.desk {
            let l = &form.look;
            cells.push(text_cell(style::LABEL, 0.0, y, W, line::TEXT, "Effect"));
            y += line::TEXT + space::LABEL;
            tiles_y = y;
            y += size::TILE + space::PAD;
            if l.effect.colors() > 0 || l.effect == Effect::Wave {
                if l.effect.colors() > 0 {
                    cells.push(text_cell(style::LABEL, 0.0, y, HALF, line::TEXT, "Colors"));
                }
                if l.effect == Effect::Wave {
                    cells.push(text_cell(style::LABEL, HALF + space::MARGIN, y, HALF, line::TEXT, "Direction"));
                }
                y += line::TEXT + space::LABEL;
                dots(&mut cells, 0.0, y, &l.colors, 0, true);
                if l.effect == Effect::Wave {
                    seg(&mut cells, HALF + space::MARGIN, y, Ctl::SceneWay, &["Left to right", "Right to left"], l.reverse as usize);
                }
                y += CTRL + space::PAD;
            }
            if l.effect != Effect::Static {
                cells.push(text_cell(style::LABEL, 0.0, y, W, line::TEXT, "Speed"));
                let words = crate::scenes::speed_words(l.speed);
                let ww = crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, words).ceil() + 2.0;
                cells.push(text_cell(style::NOTE, W - ww, y, ww, line::TEXT, words));
                y += line::TEXT + space::LABEL;
                cells.push(Cell { value: l.speed, color: crate::lighting::run(), id: id(Ctl::SceneSpeed), ..cell(kind::FILLED, 0.0, y, W, size::SLIDER) });
                y += size::SLIDER + space::PAD;
            }
            for (k, (dev, name, _, profiles)) in self.scene_form_plugins().iter().enumerate() {
                cells.push(text_cell(style::LABEL, 0.0, y, W, line::TEXT, name));
                y += line::TEXT + space::LABEL;
                let shown = form.plugins.iter().find(|(d, _)| d == dev).and_then(|(_, p)| profiles.iter().find(|(r, _)| r == p).map(|(_, n)| n.clone())).unwrap_or_else(|| "Leave as it is".into());
                cells.push(Cell { text: shown.as_str().into(), on: self.scene_form_menu == Some(FormMenu::Plugin(dev.clone())), id: id(Ctl::ScenePlugin), index: k as i32, ..cell(kind::DROP, 0.0, y, HALF, CTRL) });
                y += CTRL + space::TIGHT;
                y = crate::form::note_lines(&mut cells, 0.0, y, W, style::NOTE, &format!("{name} lights its devices from its own profiles. Choose one for this scene.")) + space::PAD;
            }
            y -= space::PAD;
        } else {
            // a row for each device: its glyph, its name, its effect and its colors
            let mut first = true;
            for (k, (key, name, glyph)) in devices.iter().enumerate() {
                if !first {
                    cells.push(crate::form::rule(0.0, y, W));
                }
                first = false;
                cells.push(Cell { style: *glyph, ..cell(kind::DEVICE, 0.0, y + space::ROW, CTRL, CTRL) });
                cells.push(text_cell(style::LABEL, CTRL + space::ROW, y + space::ROW, size::LABEL - CTRL - space::ROW, CTRL, name));
                let part = form.parts.iter().find(|(d, _)| d == key).map_or(Part::AsIs, |(_, p)| p.clone());
                let word = match &part {
                    Part::AsIs => "Leave as it is",
                    Part::Look(l) => l.effect.label(),
                };
                let dx = size::LABEL + space::PAD;
                cells.push(Cell { text: word.into(), on: self.scene_form_menu == Some(FormMenu::Part(key.clone())), id: id(Ctl::ScenePart), index: k as i32, ..cell(kind::DROP, dx, y + space::ROW, PART_DROP, CTRL) });
                if let Part::Look(l) = &part {
                    dots(&mut cells, dx + PART_DROP + space::ROW, y + space::ROW, &l.colors, 100 * (k + 1), l.colors.len() == 1);
                }
                y += space::ROW + CTRL + space::ROW;
            }
            for (k, (dev, name, glyph, profiles)) in self.scene_form_plugins().iter().enumerate() {
                cells.push(crate::form::rule(0.0, y, W));
                cells.push(Cell { style: *glyph, ..cell(kind::DEVICE, 0.0, y + space::ROW, CTRL, CTRL) });
                cells.push(text_cell(style::LABEL, CTRL + space::ROW, y + space::ROW, size::LABEL - CTRL - space::ROW, CTRL, name));
                let shown = form.plugins.iter().find(|(d, _)| d == dev).and_then(|(_, p)| profiles.iter().find(|(r, _)| r == p).map(|(_, n)| n.clone())).unwrap_or_else(|| "Leave as it is".into());
                cells.push(Cell { text: shown.as_str().into(), on: self.scene_form_menu == Some(FormMenu::Plugin(dev.clone())), id: id(Ctl::ScenePlugin), index: k as i32, ..cell(kind::DROP, size::LABEL + space::PAD, y + space::ROW, PART_DROP, CTRL) });
                y += space::ROW + CTRL + space::ROW;
            }
        }
        let editing = form.editing.is_some();
        let moves = form.scene().lights.moves();
        let tiles = form.desk.then(|| self.scene_tiles());
        self.scene_moves = moves;
        self.ui.set_scene_form_title(if editing { "Edit scene" } else { "New scene" }.into());
        self.ui.set_scene_form_sub(self.scene_form_sub().into());
        self.ui.set_scene_form_edit(editing);
        self.ui.set_scene_form_h(y);
        self.ui.set_scene_tiles_y(tiles_y);
        self.ui.set_scene_tiles_on(tiles.is_some());
        show_rows(self.ui.get_scene_tiles(), |m| self.ui.set_scene_tiles(m), tiles.unwrap_or_default());
        sync_cells(&self.scene_cells, cells);
        self.show_scene_strip(self.scene_t());
    }

    /// The scene form's effect tiles: each effect in the form's colors, the one chosen ticked.
    pub(super) fn scene_tiles(&self) -> Vec<EffectItem> {
        let Some(form) = &self.scene_form else { return Vec::new() };
        crate::scenes::EFFECTS
            .iter()
            .map(|(e, _, label)| {
                let l = form.look.as_effect(*e);
                let pic = crate::tokens::size::SCENE_TILE - 2.0 * crate::tokens::part::TILE_INSET;
                let (w, h) = ((pic * self.scale).round() as u32, (crate::tokens::part::TILE_PIC * self.scale).round() as u32);
                let across = if *e == crate::scenes::Effect::Wave { 2 } else { 1 };
                let rgba = crate::scenes::tile(&l, w * across, h);
                let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w * across, h);
                buf.make_mut_bytes().copy_from_slice(&rgba);
                let motion = match e {
                    crate::scenes::Effect::Breathing => 1,
                    crate::scenes::Effect::Wave => 3,
                    _ => 0,
                };
                EffectItem { name: (*label).into(), icon: slint::Image::from_rgba8(buf), on: *e == form.look.effect, motion }
            })
            .collect()
    }

    /// The form's preview: the scene across the desk now.
    pub(super) fn show_scene_strip(&mut self, t: f32) {
        let Some(form) = &self.scene_form else { return };
        let room = crate::tokens::size::DIALOG - 2.0 * crate::tokens::space::MARGIN;
        let (w, h, gap) = ((room * self.scale).round() as u32, (crate::tokens::size::PREVIEW * self.scale).round() as u32, (crate::tokens::part::SPAN_GAP * self.scale).round() as u32);
        let rgba = crate::scenes::strip(&form.scene().lights, &self.scene_spans(), w, h, gap, t);
        let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
        buf.make_mut_bytes().copy_from_slice(&rgba);
        self.ui.set_scene_strip(slint::Image::from_rgba8(buf));
    }

    /// One of the form's dropdowns opened, under it: a device's effects (or
    /// Leave as it is), or a plugin device's profiles.
    pub(super) fn open_scene_form_menu(&mut self, m: FormMenu, anchor: (f32, f32, f32), under: f32) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let Some(form) = &self.scene_form else { return };
        let row = |label: &str, tick: bool| MenuRow { kind: 0, label: label.into(), tick, ..Default::default() };
        let rows = match &m {
            FormMenu::Part(key) => {
                let now = form.parts.iter().find(|(d, _)| d == key).map(|(_, p)| p.clone());
                let effect = match &now {
                    Some(crate::scenes::Part::Look(l)) => Some(l.effect),
                    _ => None,
                };
                std::iter::once(row("Leave as it is", effect.is_none())).chain(std::iter::once(MenuRow { kind: 2, ..Default::default() })).chain(crate::scenes::EFFECTS.iter().map(|(e, _, w)| row(w, effect == Some(*e)))).collect()
            }
            FormMenu::Plugin(dev) => {
                let now = form.plugins.iter().find(|(d, _)| d == dev).map(|(_, p)| p.clone());
                let profiles = self.scene_form_plugins().into_iter().find(|(d, ..)| d == dev).map(|(.., p)| p).unwrap_or_default();
                std::iter::once(row("Leave as it is", now.is_none())).chain(std::iter::once(MenuRow { kind: 2, ..Default::default() })).chain(profiles.iter().map(|(r, n)| row(n, now.as_deref() == Some(r.as_str())))).collect()
            }
        };
        self.ui.set_menu_marks(true);
        self.hang_menu(rows, under, anchor);
        self.scene_form_menu = Some(m);
        self.show_scene_form();
    }

    /// A row of the form's open dropdown chosen.
    pub(super) fn scene_form_chose(&mut self, m: &FormMenu, i: usize) {
        use crate::scenes::{Look, Part, EFFECTS};
        let profiles = match m {
            FormMenu::Plugin(dev) => self.scene_form_plugins().into_iter().find(|(d, ..)| d == dev).map(|(.., p)| p).unwrap_or_default(),
            FormMenu::Part(_) => Vec::new(),
        };
        let Some(form) = &mut self.scene_form else { return };
        match m {
            FormMenu::Part(key) => {
                let new = match i {
                    0 => Some(Part::AsIs),
                    i if i >= 2 => EFFECTS.get(i - 2).map(|(e, ..)| {
                        let was = form.parts.iter().find(|(d, _)| d == key).and_then(|(_, p)| if let Part::Look(l) = p { Some(l.clone()) } else { None });
                        Part::Look(was.map_or_else(|| Look::new(*e), |l| l.as_effect(*e)))
                    }),
                    _ => None,
                };
                if let Some(p) = new {
                    match form.parts.iter_mut().find(|(d, _)| d == key) {
                        Some(x) => x.1 = p,
                        None => form.parts.push((key.clone(), p)),
                    }
                }
            }
            FormMenu::Plugin(dev) => {
                form.plugins.retain(|(d, _)| d != dev);
                if i >= 2 {
                    if let Some((r, _)) = profiles.get(i - 2) {
                        form.plugins.push((dev.clone(), r.clone()));
                    }
                }
            }
        }
        self.show_scene_form();
        self.scene_play();
    }

    /// A color of the form's, by its place (the whole desk's 0 and 1; a
    /// device's, 100 for each row down, and its own place).
    pub(super) fn scene_form_color(&self, index: usize) -> Option<[u8; 3]> {
        let f = self.scene_form.as_ref()?;
        if index < 100 {
            return f.look.colors.get(index).copied();
        }
        let (row, k) = (index / 100 - 1, index % 100);
        let key = self.scene_form_devices().get(row)?.0.clone();
        match f.parts.iter().find(|(d, _)| *d == key)? {
            (_, crate::scenes::Part::Look(l)) => l.colors.get(k).copied(),
            _ => None,
        }
    }

    pub(super) fn set_scene_form_color(&mut self, index: usize, rgb: [u8; 3]) {
        let key = (index >= 100).then(|| self.scene_form_devices().get(index / 100 - 1).map(|d| d.0.clone())).flatten();
        if index >= 100 && key.is_none() {
            return;
        }
        let Some(f) = &mut self.scene_form else { return };
        let colors = match &key {
            None => Some(&mut f.look.colors),
            Some(k) => f.parts.iter_mut().find(|(d, _)| d == k).and_then(|(_, p)| if let crate::scenes::Part::Look(l) = p { Some(&mut l.colors) } else { None }),
        };
        if let Some(c) = colors.and_then(|c| c.get_mut(index % 100)) {
            *c = rgb;
        }
        self.show_scene_form();
    }

    /// Whether each device's own Lighting page shows now, the keyboard's and
    /// the mouse's: there the device plays its own lighting, the one being
    /// chosen, on itself and on its picture, out of the desk's light.
    pub(super) fn lighting_open(&self) -> (bool, bool) {
        let place = self.ui.get_place();
        (place == "keyboard" && self.page.view == View::Lighting, place == "mouse" && self.mouse.tab == 1)
    }

    /// Where each device's lights are on Home's desk, in millimeters: the
    /// keyboard's keys with LEDs, the mouse's logo and wheel.
    pub(super) fn desk_places(&self) -> crate::scenes::Places {
        let mut out = crate::scenes::Places::default();
        let item = |id: &str| self.desk.items.iter().find(|it| it.id == id).map(|it| [it.x, it.y, it.w, it.h]);
        if let (Some(r), true) = (item(crate::cores::keyboard_key()), self.page.dev.is_some()) {
            let vb = self.page.view_box;
            out.keys = self.page.layout.iter().filter(|k| k.led).map(|k| {
                let (x, y) = crate::scenes::on_desk(vb, r, k.cx, k.cy);
                (k.pos, x, y)
            }).collect();
        }
        if let Some(r) = item(crate::mouse::key()) {
            let v = crate::mouse_art::view();
            let view = (v.0 as f64, v.1 as f64, v.2 as f64, v.3 as f64);
            let middles = match &self.mouse_layers {
                Some((scale, imgs)) => crate::mouse_art::light_boxes(&imgs.layers, (240.0 * scale).round() as u32).into_iter().map(|b| ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0)).collect(),
                None => crate::mouse_art::light_middles(),
            };
            out.mouse = crate::mouse::shape().lights.iter().zip(middles).map(|(id, (x, y))| {
                let (dx, dy) = crate::scenes::on_desk(view, r, x as f64, y as f64);
                (id.as_str(), dx, dy)
            }).collect();
        }
        out
    }

    /// The scene's next frame: each light's color where it sits, sent to its
    /// device (the keyboard every frame, the mouse as often as it takes two
    /// commands, when its colors change), and the pictures with it. A device
    /// the scene leaves as it is plays its own lighting, as does one whose own
    /// Lighting page shows (see lighting_open): as the page opens, the keyboard
    /// plays the page's preview again and the mouse is handed its own lights
    /// back; as it closes, each is in again.
    pub(super) fn scene_frame(&mut self) {
        use crate::scenes::Part;
        if self.scene_quit_done {
            return;
        }
        let Some(scene) = self.playing() else { return };
        let t = self.scene_t();
        let places = self.desk_places();
        let (kb, mouse) = (scene.lights.part(crate::cores::keyboard_key()), scene.lights.part(crate::mouse::key()));
        let keys: HashMap<u32, [u8; 3]> = match &kb {
            Part::Look(l) => places.keys.iter().map(|(pos, x, _)| (*pos, crate::scenes::color(l, *x, t))).collect(),
            Part::AsIs => HashMap::new(),
        };
        let lights = [0, 1].map(|i| match &mouse {
            Part::Look(l) => places.mouse.iter().find(|(id, ..)| crate::mouse::shape().lights.get(i).is_some_and(|l| l == id)).map(|(_, x, _)| crate::scenes::color(l, *x, t)),
            Part::AsIs => None,
        });
        self.scene_colors = (keys, lights);
        let aside = self.lighting_open();
        if aside.0 && !self.desk_aside.0 {
            let had = self.scene_claim.0;
            self.scene_claim.0 = false;
            if had {
                self.start_preview();
            }
        }
        if aside.1 != self.desk_aside.1 {
            if aside.1 && self.scene_claim.1 {
                self.mouse.hand_back();
            }
            self.scene_claim.1 = false;
            self.scene_mouse = (None, [[0; 3]; 2]);
        }
        self.desk_aside = aside;
        if self.page.dev.is_some() && !aside.0 {
            if matches!(kb, Part::Look(_)) {
                let colors: Vec<[u8; 3]> = (0..crate::cores::places() as u32).map(|p| self.scene_colors.0.get(&p).copied().unwrap_or([0; 3])).collect();
                self.ask_kb(device::Ask::Preview(serde_json::json!({ "colors": colors, "tab": "native", "claim": true })));
                self.scene_claim.0 = true;
            } else if self.scene_claim.0 {
                self.ask_kb(device::Ask::Preview(serde_json::json!({ "stop": true, "tab": "native", "claim": true })));
                self.scene_claim.0 = false;
                self.last_lights = None;
            }
        }
        if self.mouse.present() && !aside.1 {
            if let (Some(logo), Some(wheel)) = (lights[0], lights[1]) {
                let due = self.scene_mouse.0.is_none_or(|at| at.elapsed() >= Duration::from_millis(125));
                // (sent again each second, unchanged: a profile put on the mouse, or the mouse
                // plugged in again, shows its own lights until it is)
                let stale = self.scene_mouse.0.is_none_or(|at| at.elapsed() >= Duration::from_secs(1));
                if due && (self.scene_mouse.1 != [logo, wheel] || stale) {
                    self.mouse.direct(serde_json::json!({ "logo": lighting::hex_of(logo), "wheel": lighting::hex_of(wheel) }));
                    self.scene_mouse = (Some(Instant::now()), [logo, wheel]);
                    self.scene_claim.1 = true;
                }
            } else if self.scene_claim.1 {
                self.mouse.hand_back();
                self.scene_claim.1 = false;
                self.scene_mouse = (None, [[0; 3]; 2]);
            }
        }
        if self.scene_form.is_some() {
            self.show_scene_strip(t);
        }
        if self.mouse_pictured() {
            self.show_mouse_lights();
        }
        // (a plugin's device met since the scene came on, OpenRGB's after its program started, gets its profile)
        if self.scene_plugins_at.elapsed() >= Duration::from_secs(1) {
            self.scene_plugins_at = Instant::now();
            self.plugins_sync();
        }
    }

    /// The scene's frame now, as its timer would give it (a snapshot has no timers).
    pub fn scene_now(&mut self) {
        self.scene_frame();
    }

    /// A mouse light's color for its picture now: the scene's, while it lights
    /// the mouse (but on the mouse's own Lighting page); else the mouse's own,
    /// none while that light is off.
    pub(super) fn mouse_color(&self, i: usize, now: Instant) -> Option<[u8; 3]> {
        if !self.lighting_open().1 {
            if let Some(c) = self.scene_colors.1[i] {
                return Some(c);
            }
        }
        // (a mouse with fewer lights, or none, as with no mouse's core: none to color)
        let light = crate::mouse::shape().lights.get(i)?;
        self.mouse.lit(light).map(|_| self.mouse.color_at(i, now))
    }
}
