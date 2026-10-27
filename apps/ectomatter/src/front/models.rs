//! The page's models: the window drawn from the page's state, each list changed in place.

use super::*;

impl Front {
    /// Draw the page from the page's state.
    pub fn refresh(&mut self) {
        // (a device's page with no core for the device, or for a device
        // Ectomatter hasn't met, isn't shown: Home is, which says to plug one in)
        if matches!(self.ui.get_place().as_str(), "keyboard" | "mouse") && !self.devices_in_order().iter().any(|d| d.0 == self.ui.get_place().as_str()) {
            self.ui.set_place("home".into());
        }
        // (the page's sensors read only while Settings or the screen editor shows them)
        if self.ui.get_place() != "settings" && self.layouts.is_none() {
            if let Some(r) = &mut self.reader {
                r.rest();
            }
        }
        self.note_changes();
        self.sync_playlist_keys();
        self.follow_profile();
        self.sync_aside();
        // (a scene that lights the keyboard says so in place of the Lighting page's hint)
        let kb_scene = self.scene_lighting(crate::cores::keyboard_key());
        let lighting_hint = match &kb_scene {
            Some(name) => format!("The {name} scene is lighting the desk. Choosing an effect here turns the scene off."),
            None => "Choose a lighting effect for this profile, then set the colors, speed and zones below.".to_string(),
        };
        if kb_scene.is_some() {
            self.ui.set_hint_chip(self.scene_chip(self.scene_on().as_ref()));
        }
        let ui = &self.ui;
        let p = &self.page;
        ui.set_tab(match p.view {
            View::Keys => 0,
            View::Lighting => 1,
            View::Display => 2,
            View::Screen => 3,
        });
        let loaded = p.dev.is_some();
        if loaded {
            let profiles: Vec<ProfileItem> = crate::cores::profiles()
                .iter()
                .map(|&n| {
                    let (colors, [c1, c2, c3, c4]) = swatch(&p.profile_swatch(n));
                    ProfileItem {
                        n: n as i32,
                        name: p.profile_name(n).into(),
                        // (a profile tied to apps says so under its name)
                        sub: self.with_apps(&self.tied_apps(0, n as usize)).unwrap_or_else(|| p.profile_effect(n)).into(),
                        active: n == p.active(),
                        current: n == p.editing,
                        dirty: p.is_dirty(n),
                        unsaved: 0,
                        colors,
                        c1,
                        c2,
                        c3,
                        c4,
                    }
                })
                .collect();
            show_rows(ui.get_profiles(), |m| ui.set_profiles(m), profiles);
        }
        ui.set_layer(self.layer as i32);
        ui.set_hint_chip_on(p.view == View::Lighting && kb_scene.is_some());
        ui.set_hint(
            match p.view {
                View::Keys if p.sel.is_some() => "Escape clears the choice.",
                View::Keys => "Select a key on the keyboard or in the Key library to change what a key does.",
                View::Lighting => lighting_hint.as_str(),
                View::Display => "Check the keyboard’s connection and which features the installed firmware supports.",
                View::Screen => "Choose what the keyboard’s screen shows while Ectomatter is running, when a game starts or quits, and when Ectomatter isn’t running.",
            }
            .into(),
        );
        // on Screen, what the screen is doing now, beside Pause
        ui.set_screen_status(self.screen_status().into());

        // the keys over the keyboard; on Lighting, the area the pointer is over outlined
        let (x0, y0, _, _) = p.view_box;
        let zoned = self.area_keys();
        let some = self.some_areas();
        // (painting key by key: a click on a lit key paints it)
        let painting = p.drafts.contains_key(&p.editing) && p.effect_desc(&p.effect_id())["paint"] == true;
        let names = p.board_macro_names();
        let keys: Vec<KeyItem> = p
            .layout
            .iter()
            .map(|k| KeyItem {
                pos: k.pos as i32,
                x: (k.x - x0 + keyboard::GAP / 2.0) as f32,
                y: (k.y - y0 + keyboard::GAP / 2.0) as f32,
                w: (k.w - keyboard::GAP) as f32,
                h: (k.h - keyboard::GAP) as f32,
                molded: k.molded,
                remap: p.remap(k.pos, &names).filter(|_| self.rebindings).map(|s| s.to_uppercase()).unwrap_or_default().into(),
                pale: self.pale.contains(&k.pos),
                selected: p.view == View::Keys && p.sel == Some(k.pos),
                pressed: p.pressed.contains(&k.pos),
                zoned: zoned.as_ref().is_some_and(|z| z.contains(&k.pos)),
                // the hand only where a click does something: choosing a key, painting
                // one, or on Some areas, turning its area on or off
                clickable: match p.view {
                    View::Keys => true,
                    View::Lighting if painting => k.led,
                    View::Lighting => some && self.area_of(k.pos).is_some(),
                    _ => false,
                },
            })
            .collect();
        show_rows(ui.get_keys(), |m| ui.set_keys(m), keys);
        ui.set_dimming(zoned.is_some());
        ui.set_show_rebindings_on(self.rebindings);

        // the chosen key's editor
        let chosen = if p.view == View::Keys { p.sel.and_then(|pos| p.key(pos).cloned()) } else { None };
        ui.set_key_open(chosen.is_some());
        if let Some(k) = chosen {
            let fac = p.factory("base", k.pos);
            let name = p.key_label(&k);
            ui.set_key_name(name.clone().into());
            ui.set_key_factory(format!("Factory: {}", if keyboard::is_nothing(&fac) { "does nothing".to_string() } else { p.describe(&fac) }).into());
            ui.set_reset_label(format!("Reset {name} to factory").into());
            let layers: Vec<LayerItem> = LAYERS
                .iter()
                .enumerate()
                .map(|(i, (layer, title))| {
                    let d = p.def(layer, k.pos);
                    let mac = if let Action::Macro { n, .. } = d { Some(n) } else { None };
                    let label = if let Some(n) = mac { p.macro_label(n) } else if keyboard::is_nothing(&d) { "Nothing".into() } else { p.describe(&d) };
                    let held: Vec<SharedString> = mac.map(|n| p.macro_keys_held(n).into_iter().map(Into::into).collect()).unwrap_or_default();
                    let is_macro = mac.is_some();
                    LayerItem {
                        title: (*title).into(),
                        kind: keyboard::kind_of(&d).into(),
                        label: label.into(),
                        nothing: keyboard::is_nothing(&d),
                        held: ModelRc::new(VecModel::from(held)),
                        r#macro: is_macro,
                        listening: self.listening.is_some_and(|(l, _)| l == i),
                    }
                })
                .collect();
            show_rows(ui.get_layers(), |m| ui.set_layers(m), layers);
            // the one editor, on the layer shown
            let layer = LAYERS[self.layer].0;
            let d = p.def(layer, k.pos);
            let rebound_here = d != p.factory(layer, k.pos);
            let rebound = rebound_here || p.def(LAYERS[1 - self.layer].0, k.pos) != p.factory(LAYERS[1 - self.layer].0, k.pos);
            let fn_unset = self.layer == 1 && d == p.def("base", k.pos);
            let job = if fn_unset {
                "Same as on Base".to_string()
            } else if let Action::Macro { n, .. } = d {
                p.macro_label(n)
            } else if keyboard::is_nothing(&d) {
                "Nothing".into()
            } else {
                p.describe(&d)
            };
            ui.set_key_cap(k.short.as_str().into());
            ui.set_key_state((if rebound { format!("Rebound on {}.", p.profile_name(p.editing)) } else { "As it came from the factory.".to_string() }).into());
            ui.set_key_rebound(rebound);
            ui.set_layer_rebound(rebound_here);
            ui.set_key_kind(match self.key_tag(&d) {
                "Key" => 0,
                "Media" => 1,
                "Mouse" => 2,
                "Macro" => 3,
                "Profile" => 4,
                "Screen" => 5,
                "Keyboard" => 6,
                _ => 7,
            });
            ui.set_key_job(job.into());
            ui.set_key_listening(self.listening.is_some_and(|(l, _)| l == self.layer));
            let note = if fn_unset {
                "On the Function layer this key does what it does on Base, until you choose a job here.".to_string()
            } else {
                keyboard::screen_job(&d).map_or(String::new(), |c| self.screen_note(c))
            };
            ui.set_key_note(note.into());
        }
        // the Key library: each group, open or closed, its keys as they are on the layer shown
        if loaded && p.view == View::Keys {
            // something newly chosen: its group opens, once
            let chosen = match self.slider_sel {
                1 => Some(-1),
                2 => Some(-2),
                _ => p.sel.map(|pos| pos as i32),
            };
            let newly = chosen != self.lib_revealed;
            if newly {
                self.lib_revealed = chosen;
                let group = match chosen {
                    Some(-1) | Some(-2) => Some("Sliders"),
                    Some(pos) => p.key(pos as u32).map(|k| k.group.as_str()),
                    None => None,
                };
                if let Some(g) = group {
                    self.lib_shut.remove(g);
                }
            }
            let layer = LAYERS[self.layer].0;
            let q = self.lib_query.trim().to_lowercase();
            let mut groups: Vec<LibGroup> = Vec::new();
            // the sliders first, as keys are listed
            let bind_label = |i: usize| self.slider_choices(i).into_iter().find(|(v, _)| Some(v) == self.bind.get(i)).map_or_else(String::new, |c| c.1);
            let mut slider_rows: Vec<LibRow> = crate::display::sliders()
                .iter()
                .enumerate()
                .map(|(i, s)| LibRow { id: -(i as i32) - 1, cap: s.glyph.as_str().into(), name: s.label.as_str().into(), job: bind_label(i).into(), rebound: false, chosen: self.slider_sel == i + 1 })
                .collect();
            slider_rows.retain(|r| q.is_empty() || r.name.to_lowercase().contains(&q) || r.job.to_lowercase().contains(&q));
            // each group's rows keep one list, changed in place, so a click on a
            // heading or a row is never lost to a refresh between press and release
            let lib_rows = &mut self.lib_rows;
            let mut push = |name: &str, rows: Vec<LibRow>, total: usize| {
                if rows.is_empty() && !q.is_empty() {
                    return;
                }
                let open = !q.is_empty() || !self.lib_shut.contains(name);
                let rebound = rows.iter().filter(|r| r.rebound).count() as i32;
                let model = lib_rows.entry(name.to_string()).or_insert_with(|| Rc::new(VecModel::default())).clone();
                set_rows(&model, if open { rows } else { Vec::new() });
                groups.push(LibGroup { name: name.into(), count: total as i32, rebound, open, rows: ModelRc::from(model) });
            };
            // (on Base only: Fn gives the sliders no second job)
            if self.layer == 0 {
                push("Sliders", slider_rows, 2);
            }
            for (g, keys) in lib_groups(p) {
                let total = keys.len();
                let rows: Vec<LibRow> = keys
                    .iter()
                    .map(|k| {
                        let name = p.key_label(k);
                        let d = p.def(layer, k.pos);
                        let fac = p.factory(layer, k.pos);
                        let re = d != fac;
                        let job = if re {
                            if let Action::Macro { n, .. } = d { p.macro_label(n) } else if keyboard::is_nothing(&d) { "Nothing".into() } else { p.describe(&d) }
                        // (a button, with nothing printed on it, is known by what it does)
                        } else if !k.printed && !keyboard::is_nothing(&fac) && p.describe(&fac) != name {
                            p.describe(&fac)
                        } else {
                            String::new()
                        };
                        LibRow { id: k.pos as i32, cap: k.short.as_str().into(), name: name.clone().into(), job: job.into(), rebound: re, chosen: p.sel == Some(k.pos) }
                    })
                    .filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q) || r.job.to_lowercase().contains(&q))
                    .collect();
                push(&g, rows, total);
            }
            // something newly chosen: the list scrolls to its row (a group's heading, then its
            // rows, each a control's height)
            if newly && chosen.is_some() {
                let mut y = 0.0;
                for g in &groups {
                    y += crate::tokens::size::LIST_HEAD;
                    let rows: Vec<LibRow> = (0..g.rows.row_count()).filter_map(|i| g.rows.row_data(i)).collect();
                    if let Some(i) = rows.iter().position(|r| r.chosen) {
                        ui.set_lib_reveal_y(y + i as f32 * crate::tokens::size::CONTROL);
                        ui.set_lib_reveal_n(ui.get_lib_reveal_n() + 1);
                        break;
                    }
                    y += rows.len() as f32 * crate::tokens::size::CONTROL;
                }
            }
            show_rows(ui.get_lib_groups(), |m| ui.set_lib_groups(m), groups);
            // Game mode's keys, blocked or not
            let blocked = !p.draft()["lighting"]["game_keys"].as_u64().unwrap_or(0) & 0x7f;
            let items: Vec<CheckItem> = crate::display::game_keys().into_iter().map(|(bit, n)| CheckItem { name: n.into(), on: blocked & bit != 0 }).collect();
            show_rows(ui.get_game_items(), |m| ui.set_game_items(m), items);
            // a slider chosen: where it sits, what it controls
            ui.set_slider_chosen(self.slider_sel as i32);
            if self.slider_sel != 0 {
                let s = self.slider_sel - 1;
                let sliders = crate::display::sliders();
                let Some(me) = sliders.get(s) else { return };
                ui.set_slider_title(me.label.as_str().into());
                ui.set_slider_at(self.overlays.fracs.get(s).copied().flatten().unwrap_or(0.0));
                let bound = self.bind.get(s).cloned().unwrap_or_else(|| "own".into());
                let choices: Vec<CheckItem> = self.slider_choices(s).iter().map(|(v, l)| CheckItem { name: l.as_str().into(), on: bound == *v }).collect();
                show_rows(ui.get_slider_choices(), |m| ui.set_slider_choices(m), choices);
                // what the firmware can't do yet, said where it matters, in the keyboard's words
                let fw = self.firmware();
                let resting = self.idled.get(s).cloned().flatten().filter(|_| bound == "none").and_then(|job| self.slider_choices(s).into_iter().find(|(v, _)| *v == job)).map(|(_, l)| l);
                // (the slider that has its job now)
                let other = resting.as_ref().and_then(|_| {
                    let job = self.idled.get(s).cloned().flatten()?;
                    let want = crate::display::drives(me, &job);
                    sliders.iter().enumerate().find(|(j, o)| *j != s && self.bind.get(*j).is_some_and(|b| crate::display::drives(o, b) == want)).map(|(_, o)| o.called.clone())
                });
                let rest_note = resting.map(|l| {
                    let other = other.unwrap_or_else(|| "other slider".into());
                    format!("Set to Nothing while the {other} does {}. It goes back to {} when the {other} does something else.", l.to_lowercase(), l.to_lowercase())
                });
                let note = match &rest_note {
                    Some(n) => n.clone(),
                    None if bound != "own" => {
                        if me.frees.as_ref().is_some_and(|f| crate::display::has(&fw, f)) { me.freed.clone() } else { me.busy.clone() }
                    }
                    None => String::new(),
                };
                ui.set_slider_note(note.into());
            }
        }
        if self.menu.is_some() {
            self.build_menu();
        }
        if loaded && self.page.view == View::Lighting && self.ui.get_place() == "keyboard" {
            self.refresh_lighting();
        }
        self.desk_meet();
        self.desk.resize(self.ui.get_desk_w(), self.ui.get_desk_h());
        if self.ui.get_place() == "home" {
            self.show_desk();
        }
        self.take_old_ties();
        if self.ui.get_place() == "mouse" {
            self.refresh_mouse();
        } else if self.ui.get_place() == "library" {
            self.show_library();
        } else if self.ui.get_place() == "settings" {
            // what the kernel says of the keyboard, looked at every two seconds (it
            // scans every process), not at every step of a slider
            let sensors = self.sensors_view();
            self.hosted.poll();
            let st = crate::ecto_settings::State {
                sensors: Some(sensors),
                devices: self.device_rows(),
                plugins: self.package_rows(crate::packages::Kind::Plugin),
                sections: self.plugin_sections(),
                settings: self.settings.all(),
                top_peak: self.levels.1,
                reference: self.levels.0,
            };
            let (cells, h, map, pmap) = crate::ecto_settings::page(&st, self.ui.get_content_width());
            self.sensor_map = map;
            self.plugin_map = pmap;
            self.ui.set_settings_h(h);
            self.show_cells(cells);
        } else if loaded && self.page.view == View::Screen {
            // the default backlight at the right end of the tabs' row, with, while the layout
            // the screen shows has a color of its own, a line saying so: the playing layout on
            // the first tab, the onboard layout on the second; none while a page is edited
            let cells = if self.editor.is_some() {
                Vec::new()
            } else {
                let w = self.ui.get_under_w();
                let page = if self.ui.get_screen_closed() { if self.onboard_ready() { self.ob_page() } else { None } } else { self.shown() };
                let own = page.and_then(|p| p.light.map(|l| (p.name.clone(), l)));
                let picking = self.pick.as_ref().is_some_and(|p| p.ctl == Ctl::LcdColor);
                // the room the two tabs leave on their row: each tab its words and 18 either side, 4
                // between them, the open second tab's foot, then a gap (the names as app.slint has them)
                let names = ["Playlist", "Onboard layout", "Game launch"];
                let tabs: f32 = names.iter().map(|t| crate::text::width(crate::tokens::font::MEDIUM, crate::tokens::font::CONTROL, t) + 36.0).sum::<f32>() + 4.0 * (names.len() - 1) as f32;
                crate::display::screen_cells(&self.page, w, tabs + 10.0 + 24.0, own.as_ref().map(|(n, l)| (n.as_str(), *l)), picking)
            };
            self.show_cells(cells);
            self.refresh_onboard();
            self.show_game_rows();
            self.refresh_editor();
            self.draw_screen();
        } else if loaded && self.page.view == View::Display {
            let fw = self.firmware();
            let connected = match self.ui.get_conn_state() {
                1 => Some(true),
                3 => None,
                _ => Some(false),
            };
            let t = crate::display::tiles(&fw, connected, self.ui.get_page_w());
            self.ui.set_tile_height(t.height);
            self.show_cells(t.cells);
        }
        // the screen as the profile being edited has it, on the keyboard now (its own
        // status screen in the default backlight, whatever the playlist plays)
        if loaded && self.controlled {
            let mut want = screen_of(&self.page.draft()["lighting"]);
            if self.player.is_some() && !self.aside {
                let l = self.page_light([want.0[1], want.0[2], want.0[3]]);
                want.0[1..4].copy_from_slice(&l);
            } else if self.aside && self.onboard_ready() {
                // the keyboard by itself, as it lights its onboard layout: in the layout's own
                // color, or with the screen off, no light
                match self.screens.onboard_of(self.page.editing) {
                    Some(p) => {
                        if let Some(l) = p.light {
                            want.0[1..4].copy_from_slice(&l);
                        }
                    }
                    None => want.0[0] = 0,
                }
            }
            if self.screen_sent != Some(want) {
                self.screen_sent = Some(want);
                self.ask_kb(device::Ask::Screen(want.0, want.1));
            }
        }
        let back = self.page.lcd_color();
        if back != self.overlays.lcd.2 {
            self.overlays.lcd.2 = back;
            self.show_overlays();
        }        // the Macro Editor shows the same keyboard: its macros, and the keys they are on
        if self.macros.is_some() {
            self.macros_elsewhere();
        }
        if let Some(m) = &mut self.macros {
            m.refresh(&self.page);
        }
    }

    /// Where Ectomatter's macros run, for the Macro Editor's library of them:
    /// each mouse profile's buttons given one, and how each plays there.
    pub fn macros_elsewhere(&mut self) {
        // (the words the steps that do a job say it by: the devices' profiles, the macros)
        let jobs = self.job_names();
        let Some(m) = &mut self.macros else { return };
        m.mouse_holds = self.mouse.profiles.is_some();
        m.ed.jobs = jobs.clone();
        m.other.jobs = jobs;
        let (mut runs, mut names) = (Vec::new(), Vec::new());
        if let Some(store) = &self.mouse.profiles {
            for (pi, p) in store.list.iter().enumerate() {
                names.push(p.name.clone());
                for (bi, (id, label)) in crate::mouse::shape().buttons.iter().map(|b| (b.id.as_str(), b.label.as_str())).enumerate() {
                    if let Some(Action::Macro { n, plays }) = Action::read(&p.body["keys"]["base"][id]) {
                        runs.push(crate::macros::Elsewhere { profile: pi as u8, control: format!("The mouse’s {}", label.to_lowercase()), at: bi as u32, n, plays });
                    }
                }
            }
        }
        // the keyboard's keys given one, by the codes Ectomatter keeps for its jobs, on its
        // profiles (named after the mouse's)
        if self.page.dev.is_some() {
            let first = names.len();
            for (k, &p) in crate::cores::profiles().iter().enumerate() {
                names.push(format!("keyboard’s {}", self.page.profile_name(p)));
                let Some(d) = self.page.drafts.get(&p) else { continue };
                for (layer, words) in [("base", ""), ("fn", "Fn + ")] {
                    for pos in 0..crate::cores::places() as u32 {
                        let Some(code) = Action::read(&d["keys"][layer][self.page.id(pos)]).as_ref().and_then(keyboard::screen_job) else { continue };
                        let job = self.keyed.get(&code).and_then(|id| id.strip_prefix(crate::jobs::ON_KEY)).and_then(crate::jobs::Job::of);
                        if let Some(crate::jobs::Job::Macro { n, plays }) = job {
                            let key = self.page.key(pos).map(|k| self.page.key_label(k)).unwrap_or_else(|| format!("Key {pos}"));
                            runs.push(crate::macros::Elsewhere { profile: (first + k) as u8, control: format!("The keyboard’s {words}{key}"), at: 1000 + pos, n, plays });
                        }
                    }
                }
            }
        }
        for ed in [&mut m.ed, &mut m.other] {
            if ed.lib == crate::macros::Lib::Ectomatter {
                ed.elsewhere = runs.clone();
                ed.names = names.clone();
            }
        }
    }

    /// How one of Ectomatter's macros plays, set in the Macro Editor, on every
    /// mouse button that runs it, on every profile.
    pub fn macro_plays(&mut self, n: u32, how: ectomatter_protocol::Plays) {
        // (the keyboard's keys that run it, kept against their codes)
        let job = format!("{}{}", crate::jobs::ON_KEY, crate::jobs::Job::Macro { n, plays: how }.id());
        self.rekey(|id| (matches!(crate::jobs::Job::of(id.strip_prefix(crate::jobs::ON_KEY)?), Some(crate::jobs::Job::Macro { n: m, .. }) if m == n)).then(|| job.clone()));
        let Some(store) = &mut self.mouse.profiles else { return };
        let job = Action::Macro { n, plays: how }.write();
        // (the profile in use is on the mouse: a button that holds the macro
        // itself, or stops being able to, is told)
        for bi in crate::mouse::rewrite_macro(store, n, &job) {
            self.mouse.set_job(bi, job.clone());
        }
        self.macros_elsewhere();
        self.refresh();
    }

    /// One of Ectomatter's macros saved with new steps: each of the mouse's
    /// buttons that runs it told again, for the mouse to hold the macro as it
    /// is now (or to leave it to Ectomatter, if it can't hold it any more).
    pub fn macro_saved(&mut self, n: u32) {
        for bi in 0..crate::mouse::shape().buttons.len() {
            let job = self.mouse.job(bi);
            if matches!(Action::read(&job), Some(Action::Macro { n: m, .. }) if m == n) {
                self.mouse.set_job(bi, job);
            }
        }
        self.macros_elsewhere();
        self.refresh();
    }

    /// Ectomatter's jobs kept against the keyboard's codes, each one `f` gives
    /// another id changed to it, and kept so.
    pub(super) fn rekey(&mut self, f: impl Fn(&str) -> Option<String>) {
        let mut changed = false;
        for id in self.keyed.values_mut() {
            if let Some(new) = f(id) {
                changed |= *id != new;
                *id = new;
            }
        }
        if changed {
            let kept: serde_json::Map<String, serde_json::Value> = self.keyed.iter().map(|(c, id)| (c.to_string(), serde_json::json!(id))).collect();
            if let Err(e) = self.settings.set(&crate::cores::keyboard_setting("screen.keyed"), serde_json::Value::Object(kept)) {
                self.status(format!("The keys' jobs were not saved: {e}"), 2);
            }
            self.sync_playlist_keys();
        }
    }

    /// One of Ectomatter's macros deleted: every mouse button that ran it, on
    /// every profile, does nothing now, as the delete question said, so a new
    /// macro given its number later isn't run by them unasked.
    pub fn macro_gone(&mut self, n: u32) {
        // (a key that ran it does nothing now, as a button does)
        let gone = format!("{}deleted", crate::jobs::ON_KEY);
        self.rekey(|id| (matches!(crate::jobs::Job::of(id.strip_prefix(crate::jobs::ON_KEY)?), Some(crate::jobs::Job::Macro { n: m, .. }) if m == n)).then(|| gone.clone()));
        let Some(store) = &mut self.mouse.profiles else { return };
        let off = serde_json::json!({ "off": true });
        // (the profile in use is on the mouse: its buttons are told too)
        for bi in crate::mouse::rewrite_macro(store, n, &off) {
            self.mouse.set_job(bi, off.clone());
        }
        self.macros_elsewhere();
        self.refresh();
    }

    /// A control given a job Ectomatter does (`job`), pressed or let go: one of
    /// Ectomatter's macros played, as its job says it plays.
    pub(super) fn play_for(&mut self, control: &str, job: Option<serde_json::Value>, down: bool) {
        let Some(j) = job.as_ref().and_then(crate::jobs::Job::read) else { return };
        let crate::jobs::Job::Macro { n, plays } = j else {
            self.run_job(&j, down);
            return;
        };
        let jobs = self.macro_jobs.clone();
        let player = self.typist.get_or_insert_with(|| crate::typist::Player::start(jobs));
        if !down {
            player.up(control);
            return;
        }
        // (the macros it plays put in their places: the player plays keys, waits and jobs)
        if let Some(m) = crate::library::get(n) {
            let steps = crate::macros::flatten(m["steps"].as_array().map_or(&[][..], |v| v.as_slice()), &mut vec![n]);
            player.down(control, steps, plays);
        }
    }

    /// A job one of Ectomatter's macros does as a step, by its id, as its
    /// player reaches it: done as a control given the job would do it.
    pub fn macro_job(&mut self, id: &str) {
        if let Some(j) = crate::jobs::Job::of(id).filter(|j| !matches!(j, crate::jobs::Job::Macro { .. })) {
            self.run_job(&j, true);
        }
    }

    /// One of Ectomatter's own jobs, its control pressed (`down`) or let go:
    /// another device's profile switched, one of the keyboard's screen jobs
    /// done, or the scene switched, as the control goes down. (A macro plays
    /// in play_for.)
    pub(super) fn run_job(&mut self, j: &crate::jobs::Job, down: bool) {
        use crate::jobs::{Job, To};
        if !down {
            return;
        }
        eprintln!("ectomatter: a job: {}", self.job_names().of(j));
        match j {
            // (each device's profiles switched as its page switches them; a plugin's device
            // through its plugin)
            Job::Profile { device, to } if device == crate::jobs::keyboard() => {
                if self.page.dev.is_none() {
                    return;
                }
                let now = self.page.active() as usize;
                let n = match to {
                    To::Id(id) => match id.parse::<usize>() {
                        Ok(n) => n,
                        Err(_) => return,
                    },
                    To::Next => crate::cores::profiles()[crate::jobs::stepped(now.saturating_sub(1), crate::cores::profiles().len(), true)] as usize,
                    To::Previous => crate::cores::profiles()[crate::jobs::stepped(now.saturating_sub(1), crate::cores::profiles().len(), false)] as usize,
                };
                if crate::cores::profiles().contains(&(n as u8)) && n != now {
                    self.choose_profile(n as u8);
                    self.refresh();
                }
            }
            Job::Profile { device, to } if device == crate::jobs::mouse() => {
                let Some(store) = &self.mouse.profiles else { return };
                let i = match to {
                    To::Id(id) => store.list.iter().position(|p| &p.id == id),
                    To::Next => Some(crate::jobs::stepped(store.active, store.list.len(), true)),
                    To::Previous => Some(crate::jobs::stepped(store.active, store.list.len(), false)),
                };
                if let Some(i) = i {
                    self.mouse.choose_profile(i);
                    self.show_mouse_lights();
                    self.refresh();
                }
            }
            Job::Profile { device, to: To::Id(p) } => self.plugin_choose(device, p),
            // (a plugin's device steps through its profiles only as its plugin can: none does yet)
            Job::Profile { device, .. } => eprintln!("ectomatter: {device} can’t step through its profiles"),
            Job::Screen(code) => self.screen_key(*code),
            Job::Scene(to) => {
                // (chosen by hand, as on Home: kept, and on again when Ectomatter next starts;
                // none once Ectomatter is quitting, which the desk would never show)
                if self.scene_quit_done {
                    return;
                }
                let ids: Vec<String> = self.scenes().into_iter().map(|s| s.id).collect();
                let Some(want) = crate::jobs::scene_after(to, self.scene.as_deref(), &ids) else { return };
                self.set_scene(want, true);
                self.refresh();
            }
            Job::Macro { .. } => {}
        }
    }

    /// The names other devices' jobs are said by, as the pages have them now:
    /// every device with profiles a control can switch, as the Library lists
    /// them (the keyboard once it's read; a plugin's device that can be
    /// switched by hand), Ectomatter's macros, the scenes.
    pub fn job_names(&self) -> crate::jobs::Names {
        let mut devices: Vec<crate::jobs::Profiled> = self
            .lib_devices()
            .into_iter()
            .map(|d| crate::jobs::Profiled { steps: true, screen: d.key == crate::jobs::keyboard(), here: d.key != crate::jobs::keyboard() || self.page.dev.is_some(), key: d.key, name: d.name, kind: d.kind, profiles: d.profiles })
            .collect();
        for (_, d) in self.hosted.devices() {
            let cap = &d["capabilities"]["profiles"];
            let key = d["device"].as_str().unwrap_or("").to_string();
            // (a device id a job's id can carry, and no other device's)
            if cap["choose"] != true || key.is_empty() || key.contains(':') || devices.iter().any(|x| x.key == key) {
                continue;
            }
            let profiles = cap["list"].as_array().into_iter().flatten().filter_map(|p| Some((p["id"].as_str()?.to_string(), p["name"].as_str().unwrap_or("").to_string()))).collect();
            devices.push(crate::jobs::Profiled { key, name: d["name"].as_str().unwrap_or("").into(), kind: d["kind"].as_str().unwrap_or("").into(), profiles, steps: false, screen: false, here: true });
        }
        let all = crate::library::all();
        let mut macros: Vec<(u32, String)> = all.as_object().map(|m| m.iter().filter_map(|(n, v)| Some((n.parse().ok()?, v["name"].as_str().filter(|s| !s.is_empty()).map_or_else(|| format!("Macro {n}"), String::from)))).collect()).unwrap_or_default();
        macros.sort();
        let scenes = self.scenes().into_iter().map(|s| (s.id, s.name)).collect();
        crate::jobs::Names { devices, macros, scenes }
    }

    /// The devices Ectomatter shows, as they sit on Home's desk (left to right),
    /// then any not on it yet: each one's page, its name, what sort it is,
    /// and whether it is plugged in. Each is among them once Ectomatter has
    /// met it (`met`), as Home's desk has it; each by its key on the desk.
    pub fn devices_in_order(&self) -> Vec<(String, String, String, bool)> {
        let kb = |core: &crate::cores::Core| ("keyboard".to_string(), core.name().to_string(), "keyboard".to_string(), self.ui.get_conn_state() == 1);
        let mouse = || {
            let name = self.mouse.state["device"]["name"].as_str().or(crate::mouse::shape().described["name"].as_str()).unwrap_or_default().to_string();
            ("mouse".to_string(), name, "mouse".to_string(), self.mouse.present())
        };
        let mut out: Vec<(String, String, String, bool)> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for id in self.desk.order().iter().map(String::as_str).chain([crate::cores::keyboard_key(), crate::mouse::key()]) {
            // (each device once, by its key)
            if seen.iter().any(|s| s == id) {
                continue;
            }
            seen.push(id.to_string());
            match id {
                k if k == crate::cores::keyboard_key() && self.met(k) => out.extend(crate::cores::keyboard().map(kb)),
                m if m == crate::mouse::key() && self.met(m) => out.push(mouse()),
                _ => {}
            }
        }
        out
    }

    /// Whether Ectomatter has met the device by this key: read it or heard
    /// from it, this run or one before (Home's desk keeps its place).
    pub(super) fn met(&self, key: &str) -> bool {
        !key.is_empty()
            && (self.desk.keeps(key)
                || (key == crate::cores::keyboard_key() && (self.page.dev.is_some() || self.ui.get_conn_state() == 1))
                || (key == crate::mouse::key() && self.mouse.known()))
    }

    /// The rail's devices, as they sit on Home's desk.
    pub(super) fn show_rail(&self) {
        let ui = &self.ui;
        let rows: Vec<crate::RailDevice> = self.devices_in_order().into_iter().map(|(place, name, kind, _)| crate::RailDevice { place: place.into(), name: name.into(), kind: kind.into() }).collect();
        // (each device's page names it in its bar as the rail does)
        for r in &rows {
            match r.place.as_str() {
                "keyboard" if ui.get_kb_name() != r.name => ui.set_kb_name(r.name.clone()),
                "mouse" if ui.get_mouse_name() != r.name => ui.set_mouse_name(r.name.clone()),
                _ => {}
            }
        }
        show_rows(ui.get_rail_devices(), |r| ui.set_rail_devices(r), rows);
    }

    /// What the mouse's core says, from the mouse's thread.
    pub fn mouse_heard(&mut self, h: crate::mouse::Heard) {
        use crate::mouse::Heard;
        match h {
            Heard::State(v) => {
                self.mouse.heard_state(v);
                self.mouse.failed = None;
                // (a mouse gone lets every button go)
                if !self.mouse.present() && self.mouse_down != 0 {
                    self.mouse_down = 0;
                    self.show_mouse_held();
                }
            }
            Heard::Failed(e) => self.mouse.failed = Some(e),
            // the picture plays the light's effect from the moment the mouse took it
            Heard::Played(light, v, at) => {
                self.mouse.played(&light, &v, at);
                self.show_mouse_lights();
                self.mouse_frames_run();
                return;
            }
            Heard::Drawn(w, scale, l) => {
                self.mouse_drawn(w, scale, *l);
                return;
            }
            // a button pressed or let go: a press sends a ripple across Home's desk
            // from the button, as a key's does (but the pointer's click, whose ripple
            // comes from where it clicks the desk); one given one of Ectomatter's
            // macros plays it (let go ends one that plays while it's held)
            Heard::Input(control, down) => {
                // (held down on its pictures as it is on the mouse)
                if let Some(b) = crate::mouse::shape().buttons.iter().position(|b| b.id == control) {
                    let held = if down { self.mouse_down | 1 << b } else { self.mouse_down & !(1 << b) };
                    if held != self.mouse_down {
                        self.mouse_down = held;
                        self.show_mouse_held();
                    }
                }
                if down {
                    self.ripple_from_button(&control, Duration::ZERO);
                }
                self.play_for(&format!("mouse/{control}"), crate::mouse::shape().buttons.iter().position(|b| b.id == control).map(|i| self.mouse.job(i)), down);
                return;
            }
        }
        self.ui.set_mouse_known(self.mouse.known());
        self.show_rail();
        self.ui.set_mouse_conn(if !self.mouse.known() { 0 } else if self.mouse.present() { 1 } else { 2 });
        // the picture drawn ahead of the page's being opened
        if self.mouse.known() {
            self.mouse_draw();
        }
        self.refresh();
    }

    /// The mouse's picture in layers for its page at the window's scale,
    /// unless it is drawn or being drawn.
    pub(super) fn mouse_draw(&mut self) {
        let scale = self.scale;
        if self.mouse_layers.as_ref().is_some_and(|(s, _)| *s == scale) || self.mouse_drawing == Some(scale) {
            return;
        }
        self.mouse_drawing = Some(scale);
        self.mouse_draw_at((240.0 * scale).round() as u32);
    }

    /// The mouse's picture in layers, the mouse `w` pixels wide: on a thread of
    /// its own, or here when the page has no mouse's thread to hear it back
    /// from (a snapshot).
    pub(super) fn mouse_draw_at(&mut self, w: u32) {
        let scale = self.scale;
        let draw = move || {
            // (ECTOMATTER_MOUSE_ART: another copy of the drawing, to try a change to it offscreen)
            let path = std::env::var_os("ECTOMATTER_MOUSE_ART").map(std::path::PathBuf::from).unwrap_or_else(|| crate::mouse::shape().art.clone());
            let art = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            crate::mouse_art::layers(&art, w, &crate::stage_shadow(scale), crate::STAGE_SHADOW_OPACITY)
        };
        match self.mouse.tell.clone() {
            Some(tell) => {
                std::thread::spawn(move || match draw() {
                    Ok(l) => {
                        let _ = tell.send(crate::mouse::Heard::Drawn(w, scale, Box::new(l)));
                    }
                    Err(e) => eprintln!("ectomatter: the mouse's picture: {e}"),
                });
            }
            None => match draw() {
                Ok(l) => self.mouse_drawn(w, scale, l),
                Err(e) => eprintln!("ectomatter: the mouse's picture: {e}"),
            },
        }
    }

    /// The mouse's picture drawn, `w` pixels wide: its layers put on its page,
    /// or on the desk, whichever it was drawn for.
    pub(super) fn mouse_drawn(&mut self, w: u32, scale: f32, l: crate::mouse_art::Layers) {
        let page = (240.0 * scale).round() as u32;
        if w == page && self.mouse_drawing == Some(scale) {
            self.mouse_drawing = None;
        }
        if self.desk_mouse_drawing == Some(w) {
            self.desk_mouse_drawing = None;
        }
        // (drawn for a scale the window has left: drawn again for this one)
        if scale != self.scale {
            self.mouse_draw();
            return;
        }
        let for_desk = w != page;
        let imgs = mouse_images(l, scale);
        if for_desk {
            self.desk_mouse = Some((w, imgs));
            self.show_desk_mouse();
            self.light_desk_mouse();
            return;
        }
        self.ui.set_mouse_picture(imgs.base.clone());
        self.ui.set_mouse_pic_m(imgs.layers.margin as f32 / scale);
        // (its lights in HDR, as the core says they are made)
        let units_mm = self.mouse.state["device"]["scale"].as_f64().map_or(520.0 / 125.1, |v| v as f32);
        self.mouse_px_mm = crate::mouse_art::px_mm(w, units_mm);
        self.mouse_hdr = Some((scale, crate::mouse_art::Hdr::new(&imgs.layers, &self.mouse_light_model(), self.mouse_px_mm)));
        self.mouse_lit = None;
        // (in the order they.re laid: see mouse::Shape)
        self.ui.set_mouse_logo_under(imgs.lights[0].clone());
        self.ui.set_mouse_wheel_under(imgs.lights[1].clone());
        self.ui.set_mouse_logo(imgs.lights[2].clone());
        self.ui.set_mouse_wheel(imgs.lights[3].clone());
        self.mouse_layers = Some((scale, imgs));
        // (what the picture shows, shown again on the new layers)
        self.mouse_shows = (Some(0), Some(0));
        self.show_mouse_picked();
        self.show_mouse_lights();
        self.show_desk_mouse();
    }

    /// The chosen button picked out on the picture, and the one under the
    /// pointer a shade lighter, on Buttons.
    pub(super) fn show_mouse_picked(&mut self) {
        let chosen = self.mouse.button.filter(|_| self.mouse.tab == 0).map(|b| b as u8 + 1);
        let hover = self.mouse_hover.filter(|h| self.mouse.tab == 0 && Some(*h) != chosen);
        if self.mouse_layers.is_none() || self.mouse_shows == (chosen, hover) {
            return;
        }
        self.mouse_shows = (chosen, hover);
        // (the HDR picture with them picked out)
        self.mouse_light_up();
        let imgs = &self.mouse_layers.as_ref().unwrap().1;
        if let Some(p) = chosen.and_then(|n| imgs.chosen.get(n as usize - 1)) {
            self.ui.set_mouse_picked(p.clone());
        }
        self.ui.set_mouse_picked_shown(chosen.is_some());
        if let Some(p) = hover.and_then(|n| imgs.shapes.get(n as usize - 1)) {
            self.ui.set_mouse_over(p.clone());
        }
        self.ui.set_mouse_over_shown(hover.is_some());
    }

    /// Each light on the picture in its color of the moment, as the mouse shows it.
    pub(super) fn show_mouse_lights(&mut self) {
        self.mouse_light_up();
        if self.ui.get_place() == "home" {
            self.light_desk_mouse();
        }
        let now = Instant::now();
        for i in 0..2 {
            let (c, k) = crate::mouse_art::level(self.mouse.color_at(i, now));
            let tint = Color::from_argb_u8((k * 255.0).round() as u8, c[0], c[1], c[2]);
            if i == 0 {
                self.ui.set_mouse_logo_tint(tint);
            } else {
                self.ui.set_mouse_wheel_tint(tint);
            }
        }
    }

    /// The mouse's buttons held down, as they are now, on the picture of it
    /// that shows (drawn again now), and on the other when it next shows;
    /// neither drawn while the window is closed into the tray.
    pub(super) fn show_mouse_held(&mut self) {
        let shows = !self.hidden && self.mouse_pictured();
        let place = self.ui.get_place();
        if shows && place == "home" {
            self.light_desk_mouse();
        } else {
            self.desk_mouse_lit = None;
        }
        if shows && place == "mouse" {
            self.mouse_light_up();
        } else {
            self.mouse_lit = None;
        }
    }

    /// The mouse's picture drawn for the window's scale now, if it isn't (here,
    /// with no thread to draw it on: a snapshot).
    pub fn ready_mouse_picture(&mut self) {
        self.mouse_draw();
    }

    /// The mouse's picture drawn for its size on Home's desk now, settled or
    /// not, as the window draws it once the desk settles (a snapshot has no
    /// moment to wait for that).
    pub fn ready_desk_mouse(&mut self) {
        let Some(i) = self.desk.items.iter().position(|it| it.id == crate::mouse::key()) else { return };
        self.mouse_draw();
        let w = ((self.desk.screen(i)[2] * self.scale / 4.0).round() * 4.0).clamp(32.0, 1600.0) as u32;
        if w != (240.0 * self.scale).round() as u32 && self.desk_mouse.as_ref().is_none_or(|(dw, _)| *dw != w) {
            self.desk_mouse_drawing = Some(w);
            self.mouse_draw_at(w);
        }
    }

    /// How the mouse's lights are made: as the lab tunes them, or as its
    /// core's light file has them.
    pub fn mouse_light_model(&self) -> serde_json::Value {
        self.mouse_light.clone().unwrap_or_else(|| crate::mouse::light_model(&self.mouse.state["device"]))
    }

    /// The mouse lit as the lab tunes it: its lamps made again for that.
    pub fn set_mouse_light(&mut self, light: serde_json::Value) {
        self.mouse_light = Some(light);
        if let Some((scale, imgs)) = &self.mouse_layers {
            self.mouse_hdr = Some((*scale, crate::mouse_art::Hdr::new(&imgs.layers, &self.mouse_light_model(), self.mouse_px_mm)));
        }
        self.desk_mouse_hdr = None;
        self.mouse_light_up();
    }

    /// The layers Home's desk shows the mouse in: those drawn for its size
    /// there, or the page's while those aren't drawn; and their width.
    pub(super) fn desk_layers(&self) -> Option<(u32, &crate::mouse_art::Layers)> {
        match (&self.desk_mouse, &self.mouse_layers) {
            (Some((w, imgs)), _) => Some((*w, &imgs.layers)),
            (None, Some((scale, imgs))) => Some(((240.0 * scale).round() as u32, &imgs.layers)),
            _ => None,
        }
    }

    /// The mouse lit in HDR for Home's desk, as it shows now: dark and half
    /// shown while it isn't plugged in.
    pub(super) fn light_desk_mouse(&mut self) {
        let Some(item) = self.desk.items.iter().find(|it| it.id == crate::mouse::key()) else { return };
        let on = item.connected;
        let (white, ceiling) = (self.levels.0 as f32, self.light_gain(self.levels.0));
        let now = Instant::now();
        let colors = [0, 1].map(|i| self.mouse_color(i, now).filter(|_| on));
        let light = self.mouse_light_model();
        let units_mm = self.mouse.state["device"]["scale"].as_f64().map_or(520.0 / 125.1, |v| v as f32);
        let Some((w, layers)) = self.desk_layers() else { return };
        if self.desk_mouse_hdr.as_ref().is_none_or(|(hw, _)| *hw != w) {
            self.desk_mouse_hdr = Some((w, crate::mouse_art::Hdr::new(layers, &light, crate::mouse_art::px_mm(w, units_mm))));
        }
        // (the same layers, borrowed field by field beside the lamps)
        let layers = match (&self.desk_mouse, &self.mouse_layers) {
            (Some((_, imgs)), _) | (None, Some((_, imgs))) => &imgs.layers,
            _ => return,
        };
        let Some((_, hdr)) = self.desk_mouse_hdr.as_mut() else { return };
        let lit = hdr.on_ground(layers, colors, if on { 1.0 } else { 0.5 }, self.mouse_down, white, ceiling);
        self.desk_mouse_lit = Some(Rc::new(lit));
    }

    /// Each of the mouse's lights at its brightest now, in nits: its window
    /// over its LED in the light's color of the moment, no brighter than the
    /// screen shows (Peak nits); 0 for a light that's off.
    pub fn mouse_brightest(&self) -> [f64; 2] {
        let Some((_, hdr)) = &self.mouse_hdr else { return [0.0; 2] };
        let now = Instant::now();
        let most = self.light_gain(self.levels.0) as f64 * self.levels.0;
        std::array::from_fn(|i| {
            let Some(c) = self.mouse_color(i, now) else { return 0.0 };
            let lin = |v: u8| (v as f64 / 255.0).powf(2.2);
            let y = 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
            hdr.lamps.get(i).map_or(0.0, |l| (l.peak() as f64 * y).min(most))
        })
    }

    /// The mouse's lights as the lab chooses them, one at a time: each
    /// where its window is on the drawing, by its id and name. None before
    /// the mouse's picture is drawn.
    pub fn mouse_lab_parts(&self) -> Option<Vec<ectomatter_draw::hdr::CapShape>> {
        let (scale, imgs) = self.mouse_layers.as_ref()?;
        let boxes = crate::mouse_art::light_boxes(&imgs.layers, (240.0 * scale).round() as u32);
        // (in the core's order, as the lights are on the mouse)
        Some(
            self.mouse
                .light_order()
                .into_iter()
                .filter_map(|(i, id, label)| {
                    let b = *boxes.get(crate::mouse::shape().lights.iter().position(|l| *l == id)?)?;
                    let _ = i;
                    Some(ectomatter_draw::hdr::CapShape { id: id.to_string(), label, short: None, foot: b, lit: true, legend: false, button: false, face: b, print: None, bump: Default::default() })
                })
                .collect(),
        )
    }

    /// The mouse lit in HDR for its page's hole, as it shows now: its chosen
    /// and hovered buttons, its lights in their colors of the moment, as
    /// bright as their LEDs make them, up to Peak nits.
    pub(super) fn mouse_light_up(&mut self) {
        let (white, ceiling) = (self.levels.0 as f32, self.light_gain(self.levels.0));
        let now = Instant::now();
        let colors = [0, 1].map(|i| self.mouse_color(i, now));
        let Some((scale, imgs)) = &self.mouse_layers else { return };
        let Some((_, hdr)) = self.mouse_hdr.as_mut().filter(|(s, _)| s == scale) else { return };
        let part = mouse_part(&self.ui, *scale, &imgs.layers);
        let lit = hdr.picture(&imgs.layers, crate::ink_env::rgb("stage", crate::tokens::ink::STAGE), self.mouse_shows.0, self.mouse_shows.1, self.mouse_down, colors, white, ceiling, part);
        self.mouse_lit = Some(Rc::new(lit));
        self.mouse_part = part;
        self.ui.set_mouse_hdr_ready(true);
    }

    /// Where the mouse goes in the window, in its pixels (`s` to a logical
    /// pixel), and its picture there: on its page, in the stage's hole, while
    /// the page shows its picture; nothing otherwise. Drawn again first if
    /// the hole shows another part of it now (the window's width changed).
    pub fn mouse_shown(&mut self, s: f32) -> ((i32, i32, usize, usize), Option<Rc<crate::picture::Lit>>) {
        let none = ((0, 0, 0, 0), None);
        // on Home's desk: where the desk has it, with its shadow round it
        if self.ui.get_place() == "home" {
            let Some(i) = self.desk.items.iter().position(|it| it.id == crate::mouse::key()) else { return none };
            if self.desk_mouse_lit.is_none() {
                self.light_desk_mouse();
            }
            let Some((dw, layers)) = self.desk_layers() else { return none };
            let [x, y, w, h] = self.desk.screen(i);
            // (its margin at the size it shows: as drawn, scaled as the desk shows it)
            let k = w / (dw as f32 / self.scale);
            let m = layers.margin as f32 / self.scale * k;
            let (px, py) = (((self.ui.get_desk_x() + x - m) * s).round() as i32, ((self.ui.get_desk_y() + y - m) * s).round() as i32);
            let (pw, ph) = (((w + 2.0 * m) * s).round() as usize, ((h + 2.0 * m) * s).round() as usize);
            let lit = self.desk_mouse_lit.clone();
            // (drawn for its size there, as it is: no stretching a pixel or two)
            let (pw, ph) = match &lit {
                Some(l) if l.w.abs_diff(pw) <= 4 && l.h.abs_diff(ph) <= 4 && dw == ((w * self.scale / 4.0).round() * 4.0) as u32 => (l.w, l.h),
                _ => (pw, ph),
            };
            return ((px, py, pw, ph), lit);
        }
        if !self.ui.get_mouse_hole_shown() {
            return none;
        }
        let Some((scale, imgs)) = &self.mouse_layers else { return none };
        if (*scale - s).abs() > 1e-3 {
            // (drawn for another scale: the stage's shade fills the hole till this one's is)
            self.ui.set_mouse_hdr_ready(false);
            return none;
        }
        let part = mouse_part(&self.ui, *scale, &imgs.layers);
        if part != self.mouse_part || self.mouse_lit.is_none() {
            self.mouse_light_up();
        }
        let (px, py) = ((self.ui.get_mouse_pic_wx() * s).round() as i32, (self.ui.get_mouse_pic_wy() * s).round() as i32);
        ((px + part.0 as i32, py + part.1 as i32, part.2, part.3), self.mouse_lit.clone())
    }

    /// The lights' effects played on the picture, 30 frames a second, while
    /// the picture shows and an effect moves.
    pub(super) fn mouse_frames_run(&mut self) {
        let want = self.mouse_pictured() && self.mouse.moving();
        if want == self.mouse_frames.is_some() {
            return;
        }
        self.mouse_frames = want.then(|| {
            let t = slint::Timer::default();
            let asks = self.asks.clone();
            t.start(slint::TimerMode::Repeated, Duration::from_millis(33), move || asks.borrow_mut().push(Ask::MouseFrame));
            t
        });
    }

    /// What is on the mouse's picture at a point on it (in its logical
    /// pixels): the button there, on Buttons, or the light, on Lighting.
    pub(super) fn mouse_pic_at(&self, x: f32, y: f32) -> (Option<u8>, Option<usize>) {
        let Some((scale, imgs)) = &self.mouse_layers else { return (None, None) };
        let (px, py) = (x * scale, y * scale);
        match self.mouse.tab {
            0 => (imgs.layers.button_at(px, py), None),
            1 => (None, imgs.layers.light_at(px, py)),
            _ => (None, None),
        }
    }

    /// The same at a point in the window, inside the stage, which clips the picture.
    pub(super) fn mouse_pic_at_window(&self, x: f32, y: f32) -> (Option<u8>, Option<usize>) {
        let ui = &self.ui;
        let (x0, y0, w) = (ui.get_mouse_wx(), ui.get_mouse_wy(), ui.get_mouse_w());
        let (sy, sh) = (y0 + ui.get_mouse_stage_y(), ui.get_mouse_stage_height());
        if x < x0 || x >= x0 + w || y < sy || y >= sy + sh {
            return (None, None);
        }
        let m = ui.get_mouse_pic_m();
        self.mouse_pic_at(x - (x0 + w / 2.0 - 600.0 + 480.0 - m), y - (sy + 30.0 - m))
    }

    /// Whether the mouse's picture shows: on its page's Buttons or Lighting,
    /// or on Home's desk.
    pub(super) fn mouse_pictured(&self) -> bool {
        match self.ui.get_place().as_str() {
            "mouse" => self.mouse.tab < 2,
            "home" => self.desk.items.iter().any(|i| i.id == crate::mouse::key()),
            _ => false,
        }
    }
}
