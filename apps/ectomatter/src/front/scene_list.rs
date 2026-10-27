//! Scenes: the two that come with Ectomatter and the person's own, played across the desk.

use super::*;

impl Front {
    /// Every scene: the two that come with Ectomatter, then the person's own.
    pub fn scenes(&self) -> Vec<crate::scenes::Scene> {
        crate::scenes::all(&self.settings.get("scenes").unwrap_or_default())
    }

    /// The scene on, if any (chosen by hand, or by a game or app in front).
    pub fn scene_on(&self) -> Option<crate::scenes::Scene> {
        let id = self.scene.as_ref()?;
        self.scenes().into_iter().find(|s| &s.id == id)
    }

    /// The scene on, by its id.
    pub fn scene_id(&self) -> Option<String> {
        self.scene.clone()
    }

    /// What the desk plays now: the scene form's scene while the form is open
    /// (the devices show it as it's set), else the scene on.
    pub(super) fn playing(&self) -> Option<crate::scenes::Scene> {
        match &self.scene_form {
            Some(f) => Some(f.scene()),
            None => self.scene_on(),
        }
    }

    /// A scene turned on, or none (No scene): by hand, kept to come on again
    /// when Ectomatter next starts; or for a game or app in front, not kept.
    pub fn set_scene(&mut self, id: Option<String>, by_hand: bool) {
        let id = id.filter(|i| self.scenes().iter().any(|s| &s.id == i));
        if by_hand {
            self.scene_back = None;
            if let Err(err) = self.settings.set("scene", id.as_ref().map_or(serde_json::Value::Null, |i| serde_json::json!(i))) {
                self.status(format!("The scene was not saved: {err}"), 2);
            }
        }
        self.scene = id;
        self.scene_play();
    }

    /// The desk lit as the scene playing has it: its frames from now (one at
    /// once), each plugin's device on the scene's profile; with none playing,
    /// each device's own lighting handed back to it.
    pub(super) fn scene_play(&mut self) {
        // (handed back for Ectomatter quitting: nothing lights the devices again)
        if self.scene_quit_done {
            return;
        }
        let playing = self.playing();
        if playing.is_none() {
            self.scene_timer = None;
            self.scene_hand_back();
        } else {
            if self.scene_timer.is_none() {
                self.scene_t0 = Instant::now();
                let t = slint::Timer::default();
                let asks = self.asks.clone();
                // (the keyboard takes 25 frames a second)
                t.start(slint::TimerMode::Repeated, Duration::from_millis(40), move || asks.borrow_mut().push(Ask::SceneFrame));
                self.scene_timer = Some(t);
            }
            self.scene_frame();
        }
        self.scene_moves = playing.as_ref().is_some_and(|s| s.lights.moves());
        self.plugins_sync();
        self.show_scene_button();
        self.show_mouse_lights();
    }

    /// Each device's own lighting again, where the scene was sending it its
    /// part: the keyboard out of its direct colors, the mouse's lights as it
    /// keeps them.
    pub(super) fn scene_hand_back(&mut self) {
        if self.scene_claim.0 {
            self.ask_kb(device::Ask::Preview(serde_json::json!({ "stop": true, "tab": "native", "claim": true })));
        }
        if self.scene_claim.1 {
            self.mouse.hand_back();
        }
        self.scene_claim = (false, false);
        self.scene_colors = Default::default();
        self.scene_mouse = (None, [[0; 3]; 2]);
        self.desk_aside = (false, false);
        self.last_lights = None;
    }

    /// The scene off for Ectomatter quitting, each device its own again; kept
    /// as it was, to come on again next time.
    pub fn scene_quit(&mut self) {
        self.scene_timer = None;
        self.scene_form = None;
        self.scene_hand_back();
        if !crate::desk::offscreen() && !self.scene_quit_done {
            // (each plugin's device Ectomatter switched back on its own, after any call
            // still waiting its turn; up to three seconds for them, then Ectomatter goes)
            self.plugins_to(None, true);
            self.plugin_calls = None;
            if let Some(done) = self.plugin_done.take() {
                let _ = done.recv_timeout(Duration::from_secs(3));
            }
        }
        self.scene_quit_done = true;
    }

    /// Escape on the scene form: its open dropdown or the color picker shut
    /// first, else the form, unkept.
    pub(super) fn scene_form_escape(&mut self) {
        if self.ui.get_menu_open() || self.pick.is_some() {
            self.close_menu();
            self.close_picker();
        } else {
            self.close_scene_form(false);
        }
    }

    /// The scene form shut as Ectomatter closes to the tray, unkept: the scene
    /// on plays again, not the one being set.
    pub fn cancel_scene_form(&mut self) {
        if self.scene_form.is_some() {
            self.close_scene_form(false);
        }
    }

    /// Each device a plugin brought in (OpenRGB's lighting) on the profile it
    /// plays now: the scene playing's, else the one the app in front chose
    /// (`following`), else back on what it was on before Ectomatter switched
    /// it. A device is sent one call when that changes, every call in the
    /// order made, on a thread of their own; a device gone is forgotten, to be
    /// put on its profile again when it's back. Offscreen, nothing is sent.
    pub(super) fn plugins_sync(&mut self) {
        if crate::desk::offscreen() || self.scene_quit_done {
            return;
        }
        let scene = self.playing();
        self.plugins_to(scene.as_ref(), false);
    }

    /// The plugins' devices put on what `scene` and the app in front have them
    /// play; quitting, each one Ectomatter switched back on its own.
    pub(super) fn plugins_to(&mut self, scene: Option<&crate::scenes::Scene>, quitting: bool) {
        let devices = self.hosted.devices();
        let live: std::collections::HashSet<String> = devices.iter().map(|(plugin, d)| format!("{plugin}/{}", d["device"].as_str().unwrap_or(""))).collect();
        self.plugin_applied.retain(|k, _| live.contains(k));
        for (plugin, d) in devices {
            let id = d["device"].as_str().unwrap_or("").to_string();
            let key = format!("{plugin}/{id}");
            let want = if quitting { None } else { scene.and_then(|s| s.plugins.iter().find(|(dev, _)| *dev == id).map(|(_, p)| p.clone())).or_else(|| self.following.get(&key).cloned()) };
            // (a profile chosen by hand stays while the scene playing and the app in front's
            // choice are what they were then; once either changes, what's wanted goes on)
            let why = (scene.map(|s| s.id.clone()), self.following.get(&key).cloned());
            let mut force = quitting && self.plugin_hand.contains_key(&key);
            if let Some(then) = self.plugin_hand.get(&key) {
                if *then == why && !quitting {
                    continue;
                }
                self.plugin_hand.remove(&key);
                force = true;
            }
            if !force && want == self.plugin_applied.get(&key).cloned() {
                continue;
            }
            let Some(core) = self.hosted.get(&plugin).map(|p| p.core.clone()) else { continue };
            let name = d["name"].as_str().unwrap_or("").to_string();
            let (method, params) = match &want {
                Some(p) => ("profiles/activate", serde_json::json!({ "device": id, "profile": p })),
                None => ("profiles/restore", serde_json::json!({ "device": id })),
            };
            eprintln!("ectomatter: {name}: {method}");
            match want {
                Some(p) => self.plugin_applied.insert(key, p),
                None => self.plugin_applied.remove(&key),
            };
            self.plugin_call(core, method, params, name);
        }
    }

    /// A call to a plugin's device, on the thread they all go out on, one after
    /// another (its plugin may take a moment to reach the program it goes through).
    pub(super) fn plugin_call(&mut self, core: crate::core_host::Core, method: &'static str, params: serde_json::Value, name: String) {
        if self.plugin_calls.is_none() {
            let (tx, rx) = mpsc::channel::<(crate::core_host::Core, &'static str, serde_json::Value, String)>();
            let (done_tx, done_rx) = mpsc::channel::<()>();
            let _ = std::thread::Builder::new().name("plugin devices".into()).spawn(move || {
                for (core, method, params, name) in rx {
                    if let Err(e) = core.call(method, &params) {
                        eprintln!("ectomatter: {name} didn’t switch: {e}");
                    }
                }
                let _ = done_tx.send(());
            });
            self.plugin_calls = Some(tx);
            self.plugin_done = Some(done_rx);
        }
        if let Some(tx) = &self.plugin_calls {
            let _ = tx.send((core, method, params, name));
        }
    }

    /// A plugin's device switched to one of its profiles by hand (a control's
    /// job): through its plugin, in turn with the scenes' and the apps'
    /// switches; the person's own choice from then on (profiles/choose).
    pub(super) fn plugin_choose(&mut self, device: &str, profile: &str) {
        if self.scene_quit_done || crate::desk::offscreen() {
            return;
        }
        let Some((plugin, d)) = self.hosted.devices().into_iter().find(|(_, d)| d["device"].as_str() == Some(device)) else { return };
        let name = d["name"].as_str().unwrap_or("").to_string();
        if d["capabilities"]["profiles"]["choose"] != true {
            eprintln!("ectomatter: {name} can’t be switched by hand");
            return;
        }
        let Some(core) = self.hosted.get(&plugin).map(|p| p.core.clone()) else { return };
        let key = format!("{plugin}/{device}");
        let scene = self.playing();
        self.plugin_hand.insert(key.clone(), (scene.map(|s| s.id), self.following.get(&key).cloned()));
        eprintln!("ectomatter: {name}: profiles/choose");
        self.plugin_call(core, "profiles/choose", serde_json::json!({ "device": device, "profile": profile }), name);
    }

    /// Home's Scenes button: the scene on, its name and its chip (with none
    /// on, the button says Scenes).
    pub(super) fn show_scene_button(&mut self) {
        let on = self.scene_on();
        self.ui.set_scene_name(on.as_ref().map_or_else(String::new, |s| s.name.clone()).into());
        self.ui.set_scene_chip(self.scene_chip(on.as_ref()));
    }

    /// The devices on Home's desk as a scene's pictures lay them, left to
    /// right: the keyboard and the mouse by their keys, each where it runs
    /// across the desk.
    pub(super) fn scene_spans(&self) -> Vec<crate::scenes::Span> {
        let mut out: Vec<crate::scenes::Span> = self
            .desk
            .items
            .iter()
            .filter_map(|it| {
                // (the desk keeps each device by its key)
                let key = match it.kind.as_str() {
                    "keyboard" | "mouse" => it.id.as_str(),
                    _ => return None,
                };
                Some(crate::scenes::Span { key: key.into(), x0: it.x / crate::desk::PER_MM, x1: (it.x + it.w) / crate::desk::PER_MM })
            })
            .collect();
        out.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        out
    }

    /// A scene's chip at the window's scale (none's, an empty outline).
    pub(super) fn scene_chip(&self, s: Option<&crate::scenes::Scene>) -> slint::Image {
        let px = (14.0 * self.scale).round().max(14.0) as u32;
        let rgba = crate::scenes::chip(s, &self.scene_spans(), px);
        let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(px, px);
        buf.make_mut_bytes().copy_from_slice(&rgba);
        slint::Image::from_rgba8_premultiplied(buf)
    }

    /// Each scene's chip, No scene's first, in the order a Library page's
    /// Scene row lists them.
    pub fn scene_chips(&self) -> Vec<slint::Image> {
        std::iter::once(self.scene_chip(None)).chain(self.scenes().iter().map(|s| self.scene_chip(Some(s)))).collect()
    }

    /// The scene a device's Lighting page is out of, by its name: the scene on,
    /// while it lights that device (by its key, `z10` or `sensei310`).
    pub(super) fn scene_lighting(&self, device: &str) -> Option<String> {
        let s = self.scene_on()?;
        matches!(s.lights.part(device), crate::scenes::Part::Look(_)).then_some(s.name)
    }
}
