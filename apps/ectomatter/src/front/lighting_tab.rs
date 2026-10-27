//! The keyboard's Lighting page: its draft shown on the keyboard while it's open, its effects, colors and zones.

use super::*;

impl Front {
    /// The keyboard shows the draft while the Lighting page is open. Nothing is
    /// stored, so leaving without saving gives the profile back untouched.
    pub(super) fn start_preview(&mut self) {
        self.previewing = true;
        self.push_preview();
    }

    /// The window closing or Ectomatter quitting: the keyboard goes back to what its
    /// profile stores, its lighting and its screen, whatever is left unsaved.
    pub fn hand_back(&mut self) {
        self.stop_preview();
        if let Some(d) = &self.page.dev {
            let saved = screen_of(&d["profiles"][self.page.active().to_string()]["lighting"]);
            if self.screen_sent != Some(saved) {
                self.screen_sent = Some(saved);
                self.ask_kb(device::Ask::Screen(saved.0, saved.1));
            }
        }
    }

    pub fn stop_preview(&mut self) {
        if self.previewing {
            self.previewing = false;
            self.ui.set_live(false);
            self.ask_kb(device::Ask::Preview(serde_json::json!({ "stop": true, "tab": "native", "claim": true })));
        }
    }

    pub(super) fn push_preview(&mut self) {
        // (while a scene sends the keyboard its colors, a preview left running waits: it
        // goes on again as the scene lets the keyboard go)
        if !self.previewing || self.page.dev.is_none() || self.kb.is_none() || self.scene_claim.0 {
            return;
        }
        // one asked for at a time; a change made meanwhile goes once it is answered
        if self.preview_asked {
            self.preview_again = true;
            return;
        }
        let mut body = self.page.preview_body();
        // the screen's clock, so the keys show the moment of the effect it draws
        body["t"] = serde_json::json!(self.t0.elapsed().as_millis() as u64);
        body["tab"] = serde_json::json!("native");
        body["claim"] = serde_json::json!(true);
        self.preview_asked = true;
        self.ask_kb(device::Ask::Preview(body));
    }

    /// A piece of a setting a core described used: a color opens the color
    /// picker, a choice or a switch takes its new value.
    pub(super) fn described_pick(&mut self, piece: crate::described::Piece, index: usize, anchor: (f32, f32, f32)) {
        let Some((_, d, v)) = self.mouse.described(piece) else { return };
        if crate::described::color_at(&d, &v, index).is_some() {
            self.open_picker(Ctl::Described, Some(piece), index, anchor);
            return;
        }
        if let Some(nv) = crate::described::picked(&d, &v, index) {
            self.mouse.set_described(piece, nv);
        }
        self.refresh();
    }

    /// Whether the keyboard's page is the one showing, whose Lighting boxes
    /// are drawn from its description.
    pub(super) fn on_keyboard(&self) -> bool {
        self.ui.get_place() == "keyboard"
    }

    /// A piece of the keyboard's Lighting boxes, drawn from its description,
    /// used: a color opens the color picker, a menu choice its menu, the
    /// zones their switch or a zone, and a choice or a checkbox takes its
    /// new value.
    pub(super) fn kb_described_pick(&mut self, piece: crate::described::Piece, index: usize, anchor: (f32, f32, f32), under: f32) {
        let Some((id, d, e)) = self.page.described(piece) else { return };
        match d["type"].as_str().unwrap_or("") {
            "color" | "colors" => {
                self.open_picker(Ctl::Described, Some(piece), index, anchor);
                return;
            }
            "choice" if d["menu"] == true => {
                self.open_light_menu(piece, anchor, under);
                return;
            }
            "zones" => {
                let light = self.page.keys_light();
                let areas = self.areas.entry((self.page.editing, self.page.effect_id())).or_insert(crate::described::all_zones(&light));
                let z = crate::described::zones_picked(&light, crate::described::kept(&e, &id, &d).as_u64().unwrap_or(0), index, areas);
                self.page.change_settings(|e| crate::described::keep(e, &id, &d, &serde_json::json!(z)));
            }
            _ => {
                if let Some(v) = crate::described::picked(&d, &crate::described::kept(&e, &id, &d), index) {
                    self.page.change_settings(|e| crate::described::keep(e, &id, &d, &v));
                }
            }
        }
        self.push_preview();
    }

    /// On Some zones, a zone turned on or off from the keyboard's picture, as its chip does.
    pub(super) fn kb_zone(&mut self, zone: usize) {
        let effect = self.page.effect_desc(&self.page.effect_id());
        if let Some(k) = crate::described::params(&effect).iter().position(|(_, d)| d["type"] == "zones") {
            self.kb_described_pick(crate::described::Piece::Effect(k), zone, (0.0, 0.0, 0.0), 0.0);
        }
    }

    /// A piece of the tile used: a swatch opens the color picker, the rest change the draft.
    pub(super) fn cell_pick(&mut self, c: Ctl, index: usize, _anchor: (f32, f32, f32), under: f32) {
        match c {
            Ctl::SceneName if index == 2 => {
                self.scene_form_escape();
                return;
            }
            Ctl::SceneLights => {
                if let Some(f) = &mut self.scene_form {
                    f.desk = index == 0;
                }
                self.show_scene_form();
                self.scene_play();
                return;
            }
            Ctl::SceneWay => {
                if let Some(f) = &mut self.scene_form {
                    f.look.reverse = index == 1;
                }
                self.show_scene_form();
                return;
            }
            Ctl::SceneColor => {
                self.open_picker(Ctl::SceneColor, None, index, _anchor);
                return;
            }
            Ctl::ScenePart => {
                if let Some((key, ..)) = self.scene_form_devices().get(index).cloned() {
                    self.open_scene_form_menu(FormMenu::Part(key), _anchor, under);
                }
                return;
            }
            Ctl::ScenePlugin => {
                if let Some((dev, ..)) = self.scene_form_plugins().get(index).cloned() {
                    self.open_scene_form_menu(FormMenu::Plugin(dev), _anchor, under);
                }
                return;
            }
            Ctl::LibChoice => {
                self.open_games_menu(crate::games_page::Menu::Device(index), _anchor, under);
                self.refresh();
                return;
            }
            Ctl::LibSetting => {
                self.open_games_menu(crate::games_page::Menu::Setting(index), _anchor, under);
                self.refresh();
                return;
            }
            Ctl::LibScript => {
                self.close_menu();
                self.games.edit_script(index, &self.ui);
                self.refresh();
                return;
            }
            Ctl::LibScriptRun => {
                if let Some(s) = self.games.run_now(index) {
                    self.worker.try_now(s);
                }
                self.refresh();
                return;
            }
            Ctl::LibScriptAdd => {
                self.close_menu();
                self.games.add_script(&self.ui);
                self.refresh();
                return;
            }
            Ctl::LibSpot => {
                self.close_menu();
                if let Some((plugin, action)) = self.games.spot_picked(index, &self.ui) {
                    self.section_action(&plugin, &action);
                }
                self.refresh();
                return;
            }
            Ctl::LibCheck => {
                self.close_menu();
                self.games.check_picked(index);
                self.refresh();
                return;
            }
            Ctl::LibReset => {
                self.close_menu();
                self.games.reset_picked(index);
                self.refresh();
                return;
            }
            Ctl::PackageOn => {
                self.package_switch(index / 2, index % 2 == 0);
                self.refresh();
                return;
            }
            Ctl::PackageAdd => {
                self.close_menu();
                self.add_package_form(if index == 0 { crate::packages::Kind::Core } else { crate::packages::Kind::Plugin });
                self.refresh();
                return;
            }
            // (a newer version waiting: the form a package is added from, on its release)
            Ctl::PackageReview => {
                let Some(p) = self.packages.get(index).cloned() else { return };
                let Some(repo) = p.repository() else { return };
                self.add_package_form(p.kind());
                self.ui.set_gl_plugin_address(format!("github.com/{repo}").into());
                if let Some(f) = &mut self.package_form {
                    f.typed_at = Some(Instant::now() - Duration::from_secs(1));
                }
                self.package_form_tick();
                self.refresh();
                return;
            }
            Ctl::PackageRemove => {
                // first the question, saying what's left after
                let Some(p) = self.packages.get(index).cloned() else { return };
                let built_in = crate::packages::all_in(&crate::packages::builtin_dir(p.kind()), std::path::Path::new("")).iter().any(|b| b.id() == p.id());
                let after = if built_in {
                    format!("Ectomatter goes back to the {} that comes with it.", p.name())
                } else {
                    format!("Its sections and devices go from Ectomatter, and what it changed for a game running is put back. The choices made in its sections are kept, for if you add {} again.", p.name())
                };
                // (a core: its device's page goes with it, and Ectomatter starts again to let go of it)
                let after = match p.kind() {
                    crate::packages::Kind::Core if built_in => format!("Ectomatter restarts and goes back to the {} that comes with it.", p.name()),
                    crate::packages::Kind::Core => format!("Ectomatter restarts without it, and its device’s page is gone until you add {} again.", p.name()),
                    crate::packages::Kind::Plugin => after,
                };
                self.ui.set_confirm_title(format!("Remove {} {}?", p.name(), p.version()).into());
                self.ui.set_confirm_body(after.into());
                self.ui.set_confirm_keep("Keep".into());
                self.ui.set_confirm_do("Remove".into());
                self.removing = Some(p.id().to_string());
                self.ui.set_confirm_open(true);
                return;
            }

            Ctl::MouseSameWheel => {
                self.mouse.same_wheel = index % 2 == 1;
                crate::uistate::set("mouse_same_wheel", serde_json::json!(self.mouse.same_wheel));
                if self.mouse.same_wheel {
                    self.mouse.copy_logo_to_wheel();
                }
                self.refresh();
                return;
            }
            Ctl::Paint | Ctl::LcdColor => {
                self.open_picker(c, None, index, _anchor);
                return;
            }
            Ctl::Reset => self.page.reset_effect(),
            Ctl::Preset => {
                if let Some(c) = lighting::PRESETS.get(index) {
                    self.paint = *c;
                }
            }
            // a checkbox: which box, and the state it is turned to
            Ctl::Demo => self.page.demo = index % 2 == 1,
            Ctl::LcdApp | Ctl::GameKey => crate::display::act(&mut self.page, c, index / 2, (index % 2) as f32, None),
            Ctl::LcdPreset => crate::display::act(&mut self.page, c, index, 0.0, None),
            Ctl::SliderBind => {
                self.open_bind_menu(index, _anchor, under);
                return;
            }
            Ctl::SensorTab => {
                if let Some(p) = self.sensor_map.tabs.get(index) {
                    self.sensor_tab = *p;
                    self.renaming = None;
                }
                return;
            }
            Ctl::Sensor => {
                if let Some(id) = self.sensor_map.chips.get(index).cloned() {
                    self.toggle_sensor(&id);
                }
                return;
            }
            Ctl::SensorPencil => {
                if let Some(id) = self.sensor_map.chips.get(index).cloned() {
                    let now = crate::sensors::setup().names.get(&id).cloned().or_else(|| self.sensors_seen.as_ref().and_then(|(_, cat, _)| cat.sensor(&id)).map(|(_, s)| s.name.clone()));
                    self.renaming = Some((id, now.unwrap_or_default()));
                }
                return;
            }
            Ctl::SensorRun => {
                if let Some(r) = self.sensor_map.runs.get(index).cloned() {
                    if !self.sensor_open.remove(&r) {
                        self.sensor_open.insert(r);
                    }
                }
                return;
            }
            Ctl::MakeMain => {
                if let (Some(key), Some((_, cat, _))) = (self.sensor_map.mains.get(index).cloned(), self.sensors_seen.clone()) {
                    let mut s = crate::sensors::setup();
                    s.make_main(&cat, &key);
                    self.keep_setup(s);
                }
                return;
            }
            // a new name given (Enter, or the line left), or not (Escape)
            Ctl::SensorName => {
                if let (Some((id, typed)), Some((_, cat, _))) = (self.renaming.take(), self.sensors_seen.clone()) {
                    if index == 1 {
                        let mut s = crate::sensors::setup();
                        s.rename(&cat, &id, &typed);
                        self.keep_setup(s);
                    }
                }
                return;
            }
            Ctl::PluginChoice => {
                if let Some((plugin, setting, v)) = self.plugin_map.choices.get(index).cloned() {
                    if self.settings.get(&format!("plugins.{plugin}.{setting}")).as_ref() != Some(&v) {
                        self.set_plugin_value(&plugin, &setting, v);
                    }
                }
                return;
            }
            Ctl::Control | Ctl::HdrMode => {
                let Some(v) = crate::ecto_settings::choice(c, index) else { return };
                // (the keyboard's Control is its core's switch)
                let path = match c {
                    Ctl::Control => match crate::cores::keyboard() {
                        Some(k) => crate::packages::switch_of(crate::packages::Kind::Core, &k.id),
                        None => return,
                    },
                    _ => "hdr.lighting".to_string(),
                };
                let path = path.as_str();
                if self.settings.get(path).as_ref() == Some(&v) || (c == Ctl::Control && v == serde_json::json!(self.controlled)) {
                    return;
                }
                if c == Ctl::Control && v == serde_json::json!(false) {
                    // give the keyboard back what Ectomatter borrowed before letting it go
                    self.give_sliders_back();
                    self.stop_preview();
                }
                if let Err(e) = self.settings.set(path, v) {
                    self.status(format!("The setting was not saved: {e}"), 2);
                    return;
                }
                // a change to what Ectomatter may use starts it again, back on Settings
                if c == Ctl::Control {
                    self.restart = Some("settings".into());
                }
            }
            Ctl::Fill | Ctl::Scatter | Ctl::Clear => {
                // Fill paints an area (the whole keyboard, or one of the others); the rest every key
                let area = if c == Ctl::Fill { crate::described::zone_bit(&self.page.keys_light(), index).unwrap_or(0x80) as u8 } else { 0x80 };
                let within = self.page.zone_set(area);
                let leds: Vec<u32> = self.page.layout.iter().filter(|k| k.led && within.as_ref().is_none_or(|w| w.contains(&k.pos))).map(|k| k.pos).collect();
                let mut seed = self.t0.elapsed().as_nanos() as u64 | 1;
                for pos in 0..self.page.custom.len() as u32 {
                    let led = leds.contains(&pos);
                    self.page.custom[pos as usize] = match c {
                        Ctl::Fill if led => self.paint,
                        Ctl::Scatter if led => [0, 1, 2].map(|_| {
                            seed ^= seed << 13;
                            seed ^= seed >> 7;
                            seed ^= seed << 17;
                            (seed >> 56) as u8
                        }),
                        Ctl::Clear => [0; 3],
                        _ => self.page.custom[pos as usize],
                    };
                }
                lighting::save_custom(self.page.editing, &self.page.custom);
            }
            _ => {}
        }
        self.push_preview();
    }
}
