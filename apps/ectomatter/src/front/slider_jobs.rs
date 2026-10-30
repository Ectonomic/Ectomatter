//! The sliders' jobs, and the Lighting page's dropdowns: what each slider does, held and given back.

use super::*;

impl Front {
    /// A dropdown on the Lighting page opened: its choices, under it, as wide as it is.
    pub(super) fn open_light_menu(&mut self, piece: crate::described::Piece, anchor: (f32, f32, f32), under: f32) {
        if self.just_shut(anchor) {
            return;
        }
        let crate::described::Piece::Effect(k) = piece else { return };
        let Some((id, d, e)) = self.page.described(piece) else { return };
        let rows: Vec<MenuRow> = crate::described::menu_rows(&d, &crate::described::kept(&e, &id, &d)).into_iter().map(|(kind, label, tick)| MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: "".into(), ..Default::default() }).collect();
        self.close_menu();
        self.close_picker();
        self.hang_menu(rows, under, anchor);
        self.light_menu = Some(k);
    }

    /// A turn's mark selected: the ways in, under it, the turn's own ticked.
    pub(super) fn open_way_menu(&mut self, i: usize, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        let Some(e) = self.screens.turns(self.page.editing).get(i) else { return };
        let name = self.screens.page_of(e).map_or("The screen".to_string(), |p| p.name.clone());
        let mut rows = vec![MenuRow { kind: 1, label: format!("How {name} comes in").into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() }];
        for t in crate::lcd_page::TRANSITIONS {
            rows.push(MenuRow { kind: 0, label: t.name().into(), tag: "".into(), tick: t == e.comes_in, sub: "".into(), ..Default::default() });
        }
        // the only turn: nothing comes in after it, so its way in waits for another
        if self.screens.turns(self.page.editing).len() == 1 {
            rows.push(MenuRow { kind: 3, label: "Plays once the playlist has a second layout.".into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() });
        }
        self.close_menu();
        self.close_picker();
        self.hang_menu(rows, 0.0, anchor);
        self.way_menu = Some(i);
        self.ui.set_pl_menu(i as i32);
    }

    /// A turn's time selected: under it, the times a turn often has, its own among them, and
    /// staying until the layouts are switched; the turn's own ticked.
    pub(super) fn open_time_menu(&mut self, i: usize, anchor: (f32, f32, f32), aw: f32) {
        if self.just_shut(anchor) {
            return;
        }
        let Some(e) = self.screens.turns(self.page.editing).get(i) else { return };
        let name = self.screens.page_of(e).map_or("The screen".to_string(), |p| p.name.clone());
        let mut secs: Vec<u32> = TIMES.to_vec();
        if !secs.contains(&e.secs) {
            secs.push(e.secs);
            secs.sort();
        }
        let mut picks: Vec<Option<u32>> = secs.into_iter().map(Some).collect();
        picks.push(None);
        let row = |kind: i32, label: String, tick: bool| MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: "".into(), ..Default::default() };
        let mut rows = vec![row(1, format!("How long {name} shows"), false)];
        for p in &picks {
            rows.push(match p {
                Some(s) => row(0, format!("{s} seconds"), !e.stays && e.secs == *s),
                None => row(0, STAYS.into(), e.stays),
            });
        }
        self.close_menu();
        self.close_picker();
        self.ui.set_menu_marks(true);
        self.hang_menu(rows, aw, anchor);
        self.time_menu = Some(TimeMenu { turn: i, aw, picks });
        self.ui.set_pl_timing(i as i32);
    }

    /// The panel's playlist selected: under it, every playlist (the one the
    /// profile plays ticked, each with the profiles that play it), then a new
    /// one, and this one renamed or deleted (never the last).
    pub(super) fn open_pl_menu(&mut self, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        let n = self.page.editing;
        let mine = self.screens.of_profile(n);
        let row = |kind: i32, label: String, tick: bool, sub: String| MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: sub.into(), ..Default::default() };
        let mut rows = vec![row(1, format!("{} plays", self.page.profile_name(n)), false, String::new())];
        for (k, l) in self.screens.playlists.iter().enumerate() {
            // (which profiles play it, by their names)
            let by: Vec<String> = self.screens.profiles_of(&l.id).iter().map(|p| self.page.profile_name(*p)).collect();
            let by = if by.is_empty() { "No profile".to_string() } else { by.join(", ") };
            rows.push(row(0, l.name.clone(), k == mine, by));
        }
        let name = self.screens.playlists[mine].name.clone();
        rows.push(row(2, String::new(), false, String::new()));
        rows.push(row(0, "New playlist".into(), false, String::new()));
        rows.push(row(0, format!("Rename {name}"), false, String::new()));
        rows.push(row(if self.screens.playlists.len() > 1 { 4 } else { 5 }, format!("Delete {name}"), false, String::new()));
        self.close_menu();
        self.close_picker();
        self.hang_menu(rows, self.ui.get_pl_chooser_w(), anchor);
        self.pl_menu = true;
        self.ui.set_pl_choosing(true);
    }

    /// The onboard layout's chooser selected: under it, the screen off and every screen
    /// layout, each that the keyboard can't show whole saying what stays blank, then the
    /// layout kept, to edit.
    pub(super) fn open_ob_menu(&mut self, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        let n = self.page.editing;
        let kept = self.screens.onboard_of(n).map(|p| (p.id.clone(), p.name.clone()));
        let row = |kind: i32, label: String, tick: bool, sub: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick, sub: sub.into(), ..Default::default() };
        let mut rows = vec![row(1, format!("{}’s onboard layout", self.page.profile_name(n)), false, "")];
        rows.push(row(0, "Screen off".into(), kept.is_none(), ""));
        for p in &self.screens.library {
            rows.push(row(0, p.name.clone(), kept.as_ref().is_some_and(|(id, _)| *id == p.id), &crate::lcd_page::Gaps::of(p).tag()));
        }
        rows.push(row(2, String::new(), false, ""));
        match &kept {
            Some((_, name)) => rows.push(row(0, format!("Edit {name}"), false, "")),
            None => rows.push(row(5, "Edit".into(), false, "")),
        }
        self.close_menu();
        self.close_picker();
        self.hang_menu(rows, self.ui.get_ob_chooser_w(), anchor);
        self.ob_menu = true;
        self.ui.set_ob_choosing(true);
    }

    /// Whether the keyboard can keep an onboard layout: its screen has one, and its
    /// firmware the feature its core names for it (or, for the window's pictures of the tab,
    /// ECTOMATTER_ONBOARD).
    pub(super) fn onboard_ready(&self) -> bool {
        let onboard = &crate::lcd_page::screen()["onboard"];
        onboard.is_object() && onboard["feature"].as_str().is_none_or(|f| crate::display::has(&self.firmware(), f)) || std::env::var_os("ECTOMATTER_ONBOARD").is_some()
    }

    /// What the keyboard knows by itself, for the picture of its onboard layout: the time
    /// (its own clock, which Ectomatter sets from this computer's), and the name and number
    /// of the profile the tab shows; the rest as read here.
    pub(super) fn onboard_readings(&self) -> crate::lcd_page::Readings {
        let mut r = self.readings.clone();
        r.now = Some(crate::lcd_page::Moment::now());
        if self.page.dev.is_some() {
            r.profile = Some(self.page.profile_name(self.page.editing));
            r.profile_number = Some(self.page.editing);
        }
        r
    }

    /// The layout the second tab's picture shows: one carried over the screen, before it
    /// is let go, or else the one the profile keeps on the keyboard; None for the screen off.
    pub(super) fn ob_page(&self) -> Option<&crate::lcd_page::Page> {
        match &self.ob_over {
            Some(id) => self.screens.library.iter().find(|p| &p.id == id),
            None => self.screens.onboard_of(self.page.editing),
        }
    }

    /// The second tab's row under the screen: the chooser, named for the layout the
    /// profile keeps on the keyboard and as wide as that name, and the line beside it,
    /// which says what stays blank, if anything; on a keyboard without build 5 the row
    /// says what the keyboard needs instead.
    pub(super) fn refresh_onboard(&mut self) {
        self.ui.set_ob_ready(self.onboard_ready());
        let kept = self.screens.onboard_of(self.page.editing);
        let name = kept.map_or_else(|| "Screen off".to_string(), |p| p.name.clone());
        let say = match kept {
            None => "The screen and its backlight stay off while Ectomatter isn’t running.".to_string(),
            Some(p) => crate::lcd_page::Gaps::of(p).line().unwrap_or_else(|| "While this tab is open, the keyboard shows this layout instead of the playlist.".to_string()),
        };
        self.ui.set_ob_chooser_w(crate::form::chooser_width(&name).min(OB_CHOOSER_MOST).ceil());
        self.ui.set_ob_name(name.into());
        self.ui.set_ob_say(say.into());
        self.ui.set_ob_over(self.ob_over.is_some());
    }

    /// "New screen layout" selected, at the pointer: From scratch, a copy of a layout, or a
    /// ready-made one put back.
    pub(super) fn open_new_menu(&mut self, (x, y): (f32, f32)) {
        if self.card_shut.take().is_some_and(|(j, when)| j == self.screens.library.len() && when.elapsed() < Duration::from_millis(800)) {
            return;
        }
        let (rows, made) = new_rows(&self.screens.library);
        self.close_menu();
        self.close_picker();
        // (nothing in it is ticked)
        self.ui.set_menu_marks(false);
        self.hang_menu(rows, 0.0, (x, y, 0.0));
        self.new_menu = Some(made);
    }

    /// Layout `i` moved to place `to` in the Library; the editor's window stays on the layout
    /// it is open on, and each card keeps its picture.
    pub(super) fn move_layout(&mut self, i: usize, to: usize) {
        let Some(to) = self.screens.move_page(i, to) else { return };
        if let Some(ed) = self.editor.as_mut() {
            ed.page = match ed.page {
                p if p == i => to,
                p if i < p && to >= p => p - 1,
                p if i > p && to <= p => p + 1,
                p => p,
            };
        }
        if i < self.pl_thumbs.len() {
            let pic = self.pl_thumbs.remove(i);
            self.pl_thumbs.insert(to.min(self.pl_thumbs.len()), pic);
        }
        self.pages_edited();
        self.refresh_editor();
    }

    /// Layout `k` copied (the copy after it); the editor's window, open on a later layout,
    /// stays on that layout. The copy's place.
    pub(super) fn copy_layout(&mut self, k: usize) -> Option<usize> {
        let at = self.screens.duplicate_page(k)?;
        if let Some(ed) = self.editor.as_mut().filter(|ed| ed.page >= at) {
            ed.page += 1;
        }
        self.refresh_editor();
        Some(at)
    }

    /// Where the readings that follow a choice come from now, for the editor's From line:
    /// the main card's whole name, and the driver's name for the processor's own temperature.
    pub(super) fn reading_from(&mut self) -> crate::screen_edit::From {
        use crate::lcd_page::Kind;
        let src = self.editor.as_ref().and_then(|ed| self.screens.library.get(ed.page)?.pieces.get(ed.chosen?)).and_then(|p| match p.kind {
            Kind::Data(s, _) | Kind::Bar(s) => Some(s),
            _ => None,
        });
        let Some(src) = src else { return Default::default() };
        let setup = crate::sensors::setup();
        let cat = self.reader.get_or_insert_with(|| crate::lcd_stats::Stats::of(self.hosted.readings(), "page")).catalog();
        crate::screen_edit::From { line: crate::screen_edit::from_line(src, cat, &setup) }
    }

    /// The computer's readings a block can show, for the Shows menu: the sensors checked,
    /// under the devices they read.
    pub(super) fn shows_groups(&mut self) -> Vec<(String, Vec<(String, crate::lcd_page::Source)>)> {
        let setup = crate::sensors::setup();
        let cat = self.reader.get_or_insert_with(|| crate::lcd_stats::Stats::of(self.hosted.readings(), "page")).catalog();
        crate::screen_edit::computer_groups(cat, &setup)
    }

    /// Settings, at its Sensor Feed box (from the screen editor's From line, or its Shows menu).
    /// A Settings tab opened from its bar: its page from its top.
    pub fn open_settings_tab(&mut self, tab: crate::ecto_settings::Tab) {
        if self.settings_tab == tab {
            return;
        }
        self.settings_tab = tab;
        self.ui.set_settings_goto(-1.0);
        self.ui.set_settings_goto(0.0);
        self.refresh();
    }

    pub fn show_sensors(&mut self) {
        self.ui.set_place("settings".into());
        // (the sensors are on General)
        self.settings_tab = crate::ecto_settings::Tab::General;
        self.refresh();
        let to = (self.sensor_map.top - 24.0).max(0.0);
        self.ui.set_settings_goto(-1.0);
        self.ui.set_settings_goto(to);
    }

    /// What the Sensor Feed box shows: the computer's sensors and their readings, read again
    /// when a second old; the choices; the tab open, the runs opened, the sensor renamed. A
    /// card that has come in as the main card has its usual sensors checked as it is seen.
    pub(super) fn sensors_view(&mut self) -> crate::ecto_settings::Sensors {
        if self.sensors_seen.as_ref().is_none_or(|(t, _, _)| t.elapsed() >= Duration::from_secs(1)) {
            let stats = self.reader.get_or_insert_with(|| crate::lcd_stats::Stats::of(self.hosted.readings(), "page"));
            let values = stats.read_all();
            let cat = stats.catalog().clone();
            let mut setup = crate::sensors::setup();
            if setup.settle(&cat) {
                self.keep_setup(setup);
            }
            self.sensors_seen = Some((Instant::now(), cat, values));
        }
        let (_, cat, values) = self.sensors_seen.clone().unwrap();
        crate::ecto_settings::Sensors { cat, setup: crate::sensors::setup(), values, tab: self.sensor_tab, open: self.sensor_open.clone(), renaming: self.renaming.clone() }
    }

    /// The sensors' choices changed: kept in the settings, and gone by at once by every
    /// reader of the screen's readings; the editor's and the playlist's pictures read again.
    pub(super) fn keep_setup(&mut self, s: crate::sensors::Setup) {
        if let Err(e) = self.settings.set("sensors", s.to_json()) {
            self.settings_say(format!("The sensors were not saved: {e}"), true);
            return;
        }
        if let Some((_, cat, _)) = &self.sensors_seen {
            crate::sensors::remember(cat, &s);
        }
        crate::sensors::set_up(s);
        self.read_at = None;
    }

    /// The layouts that show a sensor, by name: one shown by its number, and the processor's,
    /// the memory's and the main card's usual readings shown as the ready-made layouts show them.
    pub(super) fn shown_in(&self, id: &str, cat: &ectomatter_sensors::list::Catalog) -> Vec<String> {
        use crate::lcd_page::{Kind, Source};
        let main = crate::sensors::setup().main_card(cat).map(|d| d.key.clone()).unwrap_or_default();
        let reads = |s: Source| -> bool {
            match s {
                Source::Sensor(n) => crate::sensors::id_of(n) == id,
                Source::CpuTemp => cat.cpu_temp().is_some_and(|t| t.id == id),
                Source::CpuLoad => id == "cpu/load",
                Source::CpuClock => id == "cpu/clock/average",
                Source::Memory => id == "memory/used",
                Source::GpuTemp => id == format!("{main}/temperature"),
                Source::GpuLoad => id == format!("{main}/load"),
                Source::GpuClock => id == format!("{main}/clock"),
                Source::VideoMemory => id == format!("{main}/video-memory"),
                _ => false,
            }
        };
        self.screens.library.iter().filter(|p| p.pieces.iter().any(|q| matches!(q.kind, Kind::Data(s, _) | Kind::Bar(s) if reads(s)))).map(|p| p.name.clone()).collect()
    }

    /// A sensor's checkbox clicked: checked; or unchecked, asked about first where a layout
    /// shows it, in the app's own dialog.
    pub(super) fn toggle_sensor(&mut self, id: &str) {
        let Some((_, cat, _)) = self.sensors_seen.clone() else { return };
        let mut s = crate::sensors::setup();
        let on = s.checked_in(&cat).contains(id);
        let shows = if on { self.shown_in(id, &cat) } else { Vec::new() };
        if shows.is_empty() {
            s.check(&cat, id, !on);
            self.keep_setup(s);
            return;
        }
        let what = crate::screen_edit::sensor_phrase(id, &cat, &s);
        let them = if shows.len() == 1 { "it" } else { "them" };
        let show = if shows.len() == 1 { "shows" } else { "show" };
        self.ui.set_confirm_title(format!("Uncheck {what}?").into());
        self.ui.set_confirm_body(format!("{} {show} {what}. Unchecked, it shows as unavailable in {them}.", ectomatter_sensors::parts::and_list(&shows)).into());
        self.ui.set_confirm_keep("Keep".into());
        self.ui.set_confirm_do("Uncheck".into());
        self.unchecking = Some(id.to_string());
        self.ui.set_confirm_open(true);
    }

    /// Layout `k`'s Delete chosen from its card's menu: first the question, over the window,
    /// saying what goes with the layout.
    pub(super) fn ask_delete(&mut self, k: usize) {
        let Some(page) = self.screens.library.get(k) else { return };
        let says = self.screens.delete_says(k, self.onboard_ready(), |n| self.page.profile_name(n));
        self.ui.set_confirm_title(format!("Delete {}?", page.name).into());
        self.ui.set_confirm_body(says.into());
        self.ui.set_confirm_keep("Cancel".into());
        self.ui.set_confirm_do("Delete layout".into());
        self.deleting = Some(page.id.clone());
        self.ui.set_confirm_open(true);
    }

    /// Layout `k` deleted, Delete answered: it comes off the playlists (and the keyboard),
    /// and the editor's window, open on a later layout or on this one, follows.
    pub(super) fn delete_layout(&mut self, k: usize) {
        if !self.screens.delete_page(k) {
            return;
        }
        if let Some(ed) = &mut self.editor {
            if ed.page > k || (ed.page == k && ed.page >= self.screens.library.len()) {
                ed.page = ed.page.saturating_sub(1);
            }
            ed.chosen = None;
            if self.screens.library.is_empty() {
                self.editor = None;
            }
        }
        self.pages_edited();
        self.refresh_editor();
    }

    /// The playlist's holding bar selected: under it, every screen layout,
    /// to put one on the end of the playlist.
    /// A layout card's own menu, right-clicked, at the pointer: Edit, Add to the playlist
    /// (on the Onboard layout tab, Keep on the keyboard), Duplicate, and Delete.
    pub(super) fn open_lib_menu(&mut self, k: usize, at: (f32, f32)) {
        if self.card_shut.take().is_some_and(|(j, when)| j == k && when.elapsed() < Duration::from_millis(800)) {
            return;
        }
        let Some(name) = self.screens.library.get(k).map(|p| p.name.clone()) else { return };
        let row = |kind: i32, label: String| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() };
        let second = if self.ui.get_screen_closed() {
            format!("Keep {name} on the keyboard")
        } else {
            format!("Add {name} to {}", self.screens.playlist(self.page.editing).name)
        };
        let rows = vec![row(0, format!("Edit {name}")), row(0, second), row(0, format!("Duplicate {name}")), row(2, String::new()), row(4, format!("Delete {name}…"))];
        self.close_menu();
        self.close_picker();
        // (nothing in it is ticked)
        self.ui.set_menu_marks(false);
        let (x, y) = at;
        self.hang_menu(rows, 0.0, (x, y, 0.0));
        self.lib_menu = Some(k);
    }

    pub(super) fn open_add_menu(&mut self, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        let row = |kind: i32, label: &str| MenuRow { kind, label: label.into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() };
        let mut rows = vec![row(1, "Add a screen layout")];
        for p in &self.screens.library {
            rows.push(row(0, &p.name));
        }
        self.close_menu();
        self.close_picker();
        self.ui.set_menu_marks(false);
        self.hang_menu(rows, 0.0, anchor);
        self.add_menu = true;
        self.ui.set_pl_adding(true);
    }

    /// The playlist's name changed in place, kept or let go; then the
    /// chooser again.
    pub(super) fn end_rename(&mut self, keep: bool, typed: &str) {
        if !std::mem::take(&mut self.pl_renaming) {
            return;
        }
        self.ui.set_pl_renaming(false);
        // (its field gone, nothing is being typed in)
        slint::ComponentHandle::global::<crate::Typing>(&self.ui).set_on(false);
        if keep {
            let n = self.page.editing;
            let (id, was) = { let l = self.screens.playlist(n); (l.id.clone(), l.name.clone()) };
            self.screens.rename(&id, typed);
            if self.screens.playlist(n).name != was {
                self.pages_edited();
            }
        }
        self.refresh();
    }

    /// The playlist the page's profile plays, on the screen from its first turn.
    pub(super) fn play_from_start(&mut self) {
        self.tell_page();
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::Show(0));
        }
        self.lcd.page = 0;
    }

    /// What a slider can control (display::choices), on this keyboard.
    pub(super) fn slider_choices(&self, i: usize) -> Vec<(String, String)> {
        crate::display::sliders().get(i).map_or_else(Vec::new, |s| crate::display::choices(s, crate::display::backlight_setting().is_some()))
    }

    pub(super) fn open_bind_menu(&mut self, i: usize, anchor: (f32, f32, f32), under: f32) {
        self.close_menu();
        let Some(s) = crate::display::sliders().get(i).cloned() else { return };
        let mut rows = vec![MenuRow { kind: 1, label: format!("The {} controls", s.called).into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() }];
        for (v, label) in self.slider_choices(i) {
            rows.push(MenuRow { kind: 0, label: label.into(), tag: "".into(), tick: self.bind.get(i) == Some(&v), sub: "".into(), ..Default::default() });
        }
        self.hang_menu(rows, under, anchor);
        self.bind_menu = Some((i, anchor));
    }

    /// One job, one slider: taking a job another one does leaves that one
    /// idle, rather than the two of them sending it different positions.
    pub(super) fn set_slider_bind(&mut self, i: usize, v: &str) {
        let sliders = crate::display::sliders();
        let Some(me) = sliders.get(i) else { return };
        self.bind.resize(sliders.len(), "own".into());
        self.idled.resize(sliders.len(), None);
        self.bind[i] = v.into();
        let want = crate::display::drives(me, v);
        for (other, o) in sliders.iter().enumerate().filter(|(j, _)| *j != i) {
            if v != "none" && crate::display::drives(o, &self.bind[other]) == want {
                // the other slider did this job: it rests, and remembers the job
                self.idled[other] = Some(std::mem::replace(&mut self.bind[other], "none".into()));
            } else if let Some(job) = self.idled[other].take() {
                // this slider has given up the other's job: the other has it back
                if self.bind[other] == "none" && crate::display::drives(o, &job) != want {
                    self.bind[other] = job;
                }
            }
        }
        // (a slider given a job of its own forgets the one it rested from)
        self.idled[i] = None;
        crate::uistate::set("slider_bind", serde_json::json!(self.bind));
        crate::uistate::set("slider_idled", serde_json::json!(self.idled));
        self.hold_backlight();
        self.sent = vec![None; sliders.len()];
        self.tell_free();
    }

    /// The keyboard does its sliders' jobs by itself again (each one the
    /// firmware can free, given back), and lets a held backlight go.
    pub(super) fn give_sliders_back(&mut self) {
        let fw = self.firmware();
        for s in crate::display::sliders() {
            if s.frees.as_ref().is_some_and(|f| crate::display::has(&fw, f)) {
                self.ask_kb(device::Ask::Free(s.id.clone(), false));
            }
        }
        if crate::display::backlight_setting().is_some_and(|(_, f)| f.is_none_or(|f| crate::display::has(&fw, &f))) {
            self.ask_kb(device::Ask::Backlight(None));
        }
    }

    /// Quitting: the keyboard does its sliders' jobs by itself again, so
    /// they keep working with Ectomatter gone.
    pub fn release_sliders(&mut self) {
        if self.kb.is_none() || self.released.is_some() {
            return;
        }
        self.give_sliders_back();
        self.released = Some(Instant::now());
    }

    /// The slider whose own job is the keyboard's backlight, if it has one.
    pub(super) fn backlight_slider(&self) -> Option<(usize, crate::display::Slider)> {
        crate::display::sliders().into_iter().enumerate().find(|(_, s)| s.own == "backlight")
    }

    /// The backlight follows its slider inside the keyboard, so taking that
    /// slider off it means asking the keyboard to hold the backlight where it is.
    pub(super) fn hold_backlight(&mut self) {
        let Some((i, s)) = self.backlight_slider() else { return };
        if self.bind.get(i).is_none_or(|b| b == "own") {
            self.held = None;
            self.ask_kb(device::Ask::Backlight(None));
            return;
        }
        if self.held.is_none() {
            self.held = Some(self.last_sliders.as_ref().and_then(|r| r[s.id.as_str()].as_u64()).unwrap_or(255) as u8);
        }
        self.ask_kb(device::Ask::Backlight(self.held));
    }

    /// A slider doing another job is freed of its own, where the keyboard's
    /// firmware can free it, so the two do not fight.
    pub(super) fn tell_free(&mut self) {
        let fw = self.firmware();
        for (i, s) in crate::display::sliders().iter().enumerate() {
            if s.frees.as_ref().is_some_and(|f| crate::display::has(&fw, f)) {
                let free = self.bind.get(i).is_some_and(|b| b != "own");
                self.ask_kb(device::Ask::Free(s.id.clone(), free));
            }
        }
    }

    /// Set what a slider drives, but only once it has really moved, so opening
    /// Ectomatter never throws the volume to wherever the slider sits.
    pub(super) fn drive_sliders(&mut self, r: &serde_json::Value, f: &[f32]) {
        let sliders = crate::display::sliders();
        // the backlight's level, which every key's light inside the keyboard is scaled by
        if let Some((_, s)) = self.backlight_slider() {
            self.page.dimmer = r[s.id.as_str()].as_u64().map(|v| v.min(255) as u8);
        }
        let prev = self.last_sliders.replace(r.clone());
        let Some(prev) = prev else { return };
        let raw = |v: &serde_json::Value, s: &crate::display::Slider| v[format!("{}_raw", s.id)].as_f64().unwrap_or(0.0);
        self.sent.resize(sliders.len(), None);
        let send = |me: &mut Self, i: usize, target: &str| {
            let level = (f.get(i).copied().unwrap_or(0.0) as f64 * 100.0).round() / 100.0;
            if me.sent[i] == Some(level) {
                return;
            }
            me.sent[i] = Some(level);
            if target == "backlight" {
                me.held = Some((level * 255.0).round() as u8);
                me.ask_kb(device::Ask::Backlight(me.held));
            } else {
                set_audio(target, level);
            }
        };
        for (i, s) in sliders.iter().enumerate() {
            let bound = self.bind.get(i).cloned().unwrap_or_else(|| "own".into());
            if bound == "own" {
                continue;
            }
            // the keyboard lets a held level go when its slider moves; hold it again
            if s.own == "backlight" && !r[format!("{}_held", s.id)].as_bool().unwrap_or(false) && self.held.is_some() {
                self.ask_kb(device::Ask::Backlight(self.held));
            }
            let moved = (raw(r, s) - raw(&prev, s)).abs() > 12.0;
            if moved && bound != "none" {
                send(self, i, &bound);
            }
        }
    }

    /// Where a scene's effect stands, in seconds: since it started playing,
    /// or at the moment a snapshot fixes (`light_at`, as the keys' light).
    pub(super) fn scene_t(&self) -> f32 {
        self.light_at.map_or_else(|| self.scene_t0.elapsed().as_secs_f32(), |ms| (ms / 1000.0) as f32)
    }

    /// The keys' light now, if it has changed since it was last asked for.
    pub fn lights(&mut self) -> Option<Vec<Light>> {
        let t = self.light_at.unwrap_or_else(|| self.t0.elapsed().as_secs_f64() * 1000.0);
        let mut lights = self.page.lights(t);
        // (the scene's light, while it lights the keyboard, on each key: but on the
        // keyboard's own Lighting page, where its own lighting is chosen)
        if !self.scene_colors.0.is_empty() && !self.lighting_open().0 {
            for l in &mut lights {
                if let Some(c) = self.scene_colors.0.get(&l.pos) {
                    (l.rgb, l.strength) = (*c, 1.0);
                }
            }
        }
        if self.last_lights.as_ref() == Some(&lights) {
            return None;
        }
        self.last_lights = Some(lights.clone());
        Some(lights)
    }

    pub fn animated(&self) -> bool {
        self.page.animated() || (self.scene_timer.is_some() && self.scene_moves) || self.blob_moves()
    }

    /// Home's Scenes menu: No scene, the scenes that come with Ectomatter, the
    /// person's own (each with a pencil to change it), and New scene…; the one
    /// on ticked, each with a chip of its colors.
    pub(super) fn open_scene_menu(&mut self, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let scenes = self.scenes();
        let on = self.scene.clone();
        let mut rows = vec![MenuRow { kind: 0, label: "No scene".into(), tick: on.is_none(), chip: self.scene_chip(None), chipped: true, ..Default::default() }];
        let mut does = vec![SceneRow::Off];
        let (built, own): (Vec<&crate::scenes::Scene>, Vec<&crate::scenes::Scene>) = scenes.iter().partition(|s| s.built_in);
        for group in [built, own] {
            if group.is_empty() {
                continue;
            }
            rows.push(MenuRow { kind: 2, ..Default::default() });
            does.push(SceneRow::Line);
            for sc in group {
                rows.push(MenuRow { kind: 0, label: sc.name.as_str().into(), tick: on.as_deref() == Some(sc.id.as_str()), chip: self.scene_chip(Some(sc)), chipped: true, edit: !sc.built_in, ..Default::default() });
                does.push(SceneRow::Scene(sc.id.clone()));
            }
        }
        rows.push(MenuRow { kind: 2, ..Default::default() });
        does.push(SceneRow::Line);
        rows.push(MenuRow { kind: 0, label: "New scene…".into(), chip: crate::plus_glyph((14.0 * self.scale).round() as u32), chipped: true, ..Default::default() });
        does.push(SceneRow::New);
        self.ui.set_menu_marks(true);
        self.hang_menu(rows, 0.0, anchor);
        self.scene_menu = Some(does);
    }

    /// Add pad's menu (the makers' sizes, and Draw your own), or the chosen
    /// pad's Size menu (the sizes, its own ticked, and a width and depth to
    /// type), under its button.
    pub(super) fn open_pad_menu(&mut self, which: PadMenu, anchor: (f32, f32, f32)) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let (rows, _) = self.pad_rows(which);
        // (Add pad ticks nothing, so it keeps no column for ticks)
        self.ui.set_menu_marks(which == PadMenu::Size);
        self.hang_menu(rows, 0.0, anchor);
        self.pad_menu = Some(which);
    }

    /// A pad menu's rows, and what each stands for: the sizes under their
    /// kinds' headings, each with its size in inches; then Draw your own, or
    /// the chosen pad's width and depth to type.
    pub(super) fn pad_rows(&self, which: PadMenu) -> (Vec<MenuRow>, Vec<Option<PadRow>>) {
        let row = |kind: i32, label: &str, tag: String, tick: bool, sub: &str| MenuRow { kind, label: label.into(), tag: tag.into(), tick, sub: sub.into(), ..Default::default() };
        let now = self.desk.pad_size_mm();
        let at = now.and_then(|(w, h)| crate::desk::standard(w, h));
        let (mut rows, mut what) = (Vec::new(), Vec::new());
        let mut kind = "";
        for (k, (g, label, w, h)) in crate::desk::SIZES.iter().enumerate() {
            if *g != kind {
                rows.push(row(1, g, String::new(), false, ""));
                what.push(None);
                kind = g;
            }
            rows.push(row(0, label, String::new(), which == PadMenu::Size && at == Some(k), &crate::desk::size_words(*w, *h)));
            what.push(Some(PadRow::Size(k)));
        }
        rows.push(row(2, "", String::new(), false, ""));
        what.push(None);
        match which {
            PadMenu::Add => {
                rows.push(row(0, "Draw your own", String::new(), false, ""));
                what.push(Some(PadRow::Draw));
            }
            PadMenu::Size => {
                let (w, h) = now.unwrap_or((0.0, 0.0));
                rows.push(row(1, "Your own size", String::new(), false, ""));
                what.push(None);
                rows.push(row(6, "Width", crate::desk::inches(w), false, "in"));
                what.push(Some(PadRow::Width));
                rows.push(row(6, "Depth", crate::desk::inches(h), false, "in"));
                what.push(Some(PadRow::Depth));
            }
        }
        (rows, what)
    }

    /// A field in the open menu typed in and set: the chosen pad's width or
    /// depth, in inches (or in mm or cm, said so), the menu staying open for
    /// the other.
    pub(super) fn menu_field(&mut self, i: usize, t: &str) {
        let Some(which) = self.pad_menu else { return };
        let mm = crate::desk::typed_mm(t);
        match (self.pad_rows(which).1.get(i).copied().flatten(), mm) {
            (Some(PadRow::Width), Some(mm)) => self.desk.set_pad_size(Some(mm), None),
            (Some(PadRow::Depth), Some(mm)) => self.desk.set_pad_size(None, Some(mm)),
            _ => {}
        }
        // (what it is now, in its rows: a size it took, or what it was, for words it couldn't)
        let (rows, _) = self.pad_rows(which);
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        self.show_desk();
    }
}
