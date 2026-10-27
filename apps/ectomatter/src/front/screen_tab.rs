//! The keyboard's screen: the player, the Screen tab's playlist and layouts, and Game launch.

use super::*;

impl Front {
    /// Ectomatter playing the screen, from now on.
    pub fn set_player(&mut self, p: crate::lcd_player::Player) {
        if self.lcd_paused {
            p.tell(crate::lcd_player::Control::Paused(true));
        }
        self.player = Some(p);
        self.tell_page();
        self.tell_context();
    }

    // ------------------------------------------------------------ Game launch

    /// A row of Game launch by its place: the game starting, or quitting.
    pub(super) fn launch_key(i: i32) -> &'static str {
        if i <= 0 { "start" } else { "quit" }
    }

    /// What Ectomatter's settings keep for a row of Game launch (devices.<keyboard>.screen.game).
    pub(super) fn game_launch_value(&self, k: &str) -> serde_json::Value {
        self.settings.get(&crate::cores::keyboard_setting("screen.game")).map(|v| v[k].clone()).filter(|v| v.is_object()).unwrap_or_else(|| serde_json::json!({}))
    }

    pub(super) fn set_game_launch(&mut self, k: &str, field: &str, v: serde_json::Value) {
        if let Err(e) = self.settings.set(&crate::cores::keyboard_setting(&format!("screen.game.{k}.{field}")), v) {
            self.status(format!("Game launch was not saved: {e}"), 2);
        }
        self.tell_game_launch();
    }

    /// Until when a row can show, as kept and as said: a game starting until its
    /// window opens or some seconds, one quitting until everything is put back or some seconds.
    pub(super) fn until_choices(row: usize) -> Vec<(&'static str, &'static str)> {
        if row == 0 {
            vec![("window", "the game’s window opens"), ("5", "5 seconds have passed"), ("10", "10 seconds have passed"), ("30", "30 seconds have passed")]
        } else {
            vec![("done", "everything is put back"), ("5", "5 seconds have passed"), ("10", "10 seconds have passed")]
        }
    }

    /// What the Game launch shows as a game starts and quits, as Ectomatter's settings keep it,
    /// each row's layout from Layouts (a row is on unless turned off; before there was a Game
    /// launch, the screen's announce said whether launches broke in).
    pub fn game_launch(&self) -> crate::lcd_player::Launch {
        use crate::lcd_player::{Showing, Until};
        let was_on = self.settings.on(&crate::cores::keyboard_setting("screen.announce"));
        let show = |row: usize| {
            let v = self.game_launch_value(Self::launch_key(row as i32));
            let id = v["layout"].as_str().unwrap_or(crate::lcd_page::GAME_LAYOUTS[row.min(1)]);
            let until = match v["until"].as_str() {
                Some("window") => Until::Window,
                Some("done") => Until::Done,
                Some(n) if n.parse::<u32>().is_ok() => Until::Secs(n.parse().unwrap_or(5)),
                _ if row == 0 => Until::Window,
                _ => Until::Done,
            };
            Showing { on: v["on"].as_bool().unwrap_or(was_on), layout: self.screens.library.iter().find(|p| p.id == id).cloned(), until }
        };
        crate::lcd_player::Launch { start: show(0), quit: show(1) }
    }

    pub(super) fn tell_game_launch(&self) {
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::GameSetup(self.game_launch()));
        }
    }

    /// Game launch's two rows, as the Screen tab shows them.
    pub(super) fn show_game_rows(&mut self) {
        let launch = self.game_launch();
        let rows: Vec<crate::GlRow> = [(&launch.start, "When a game starts, show"), (&launch.quit, "When a game quits, show")]
            .iter()
            .enumerate()
            .map(|(i, (s, words))| {
                let v = self.game_launch_value(Self::launch_key(i as i32));
                let kept = v["until"].as_str().unwrap_or(if i == 0 { "window" } else { "done" }).to_string();
                let until = Self::until_choices(i).into_iter().find(|(k, _)| *k == kept).map_or("", |(_, w)| w);
                crate::GlRow {
                    on: s.on,
                    words: (*words).into(),
                    layout: s.layout.as_ref().map_or_else(|| "No layout".to_string(), |p| p.name.clone()).into(),
                    until: until.into(),
                    chosen: self.gl_pick == Some(i),
                    over: self.gl_over == Some(i),
                }
            })
            .collect();
        let ui = &self.ui;
        show_rows(ui.get_gl_rows(), |m| ui.set_gl_rows(m), rows);
    }

    /// A row's until menu, under its dropdown.
    pub(super) fn open_until_menu(&mut self, row: usize, anchor: (f32, f32, f32), under: f32) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        let kept = self.game_launch_value(Self::launch_key(row as i32))["until"].as_str().unwrap_or(if row == 0 { "window" } else { "done" }).to_string();
        let rows: Vec<MenuRow> = Self::until_choices(row).iter().map(|(k, w)| MenuRow { kind: 0, label: (*w).into(), tag: "".into(), tick: *k == kept, sub: "".into(), ..Default::default() }).collect();
        self.hang_menu(rows, under, anchor);
        self.until_menu = Some(row);
    }

    /// The game a page shows in its readings: the one starting, playing or quitting, or the
    /// last one; before any, the game played last, as it was then (for the editor's and the
    /// Screen tab's pictures).
    pub(super) fn game_reading(&self) -> Option<crate::lcd_page::Game> {
        if let Some(g) = &self.game_now {
            return Some(g.clone());
        }
        let e = self.games.library.entries(&|id| self.front_apps.name(id)).into_iter().filter(|e| e.kind == crate::games::Kind::Game).max_by_key(|e| e.last)?;
        Some(crate::lcd_page::Game { name: e.name, since: e.last, ended: e.last, minutes: e.minutes, last: e.last, changing: String::new(), done: (0, 0) })
    }

    /// A page edited: the keyboard shows the change at once, and it is saved
    /// once the editing pauses.
    pub(super) fn pages_edited(&mut self) {
        self.pages_changed = Some(Instant::now());
        self.tell_page();
    }

    /// The pages the screen plays in turn, to the player: the playlist's
    /// turns, or, while a page is edited, that page alone.
    pub(super) fn tell_page(&mut self) {
        let pages = match self.editor.as_ref().and_then(|ed| self.screens.library.get(ed.page)) {
            Some(p) => vec![p.clone()],
            None => self.screens.played(self.page.editing),
        };
        // an empty playlist: the screen blank, in the profile's default backlight
        let pages = if pages.is_empty() { vec![crate::lcd_page::Page::blank()] } else { pages };
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::Pages(pages));
        }
        self.tell_game_launch();
    }

    /// The screen plays the playlist of the profile the page shows, as it
    /// shows that profile's screen light: when another profile is chosen, in
    /// Ectomatter or on the keyboard, the screen plays that profile's playlist
    /// from its first turn. (Not while a page is edited: the keyboard stays
    /// on that page.)
    pub(super) fn follow_profile(&mut self) {
        let a = self.page.editing;
        if a == self.played_by || self.editor.is_some() {
            return;
        }
        self.played_by = a;
        self.play_from_start();
    }

    /// After the playlist changed: the turn that was playing (by its key)
    /// goes on playing where it now is; taken off, the turn now in its place.
    pub(super) fn keep_playing(&mut self, playing: Option<u64>, was: usize) {
        let now = playing.and_then(|k| self.screens.turns(self.page.editing).iter().position(|e| e.key == k));
        let at = now.unwrap_or(was.min(self.screens.turns(self.page.editing).len().saturating_sub(1)));
        if now != Some(was) {
            if let Some(pl) = &self.player {
                pl.tell(crate::lcd_player::Control::Show(at));
            }
            // (marked at once, not a moment later when the player says so)
            self.lcd.page = at;
        }
    }

    /// What a page may show besides the computer's readings, to the player:
    /// the profile the keyboard is using, and the weather outside.
    pub(super) fn tell_context(&mut self) {
        let profile = self.page.dev.as_ref().map(|_| (self.page.active(), self.page.profile_name(self.page.active())));
        let outside = self.outside();
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::Context(profile, outside));
        }
    }

    /// The layouts Ectomatter ships that the Library was never given, put at its end once and
    /// saved, with the list of those given, so a layout deleted after stays deleted (and a
    /// layout Ectomatter ships later is given once too).
    pub(super) fn give_built_ins(&mut self) {
        let given = self.settings.get(&crate::cores::keyboard_setting("screen.built_ins"));
        let all = serde_json::json!(crate::lcd_page::BUILT_INS);
        if given.as_ref() == Some(&all) {
            return;
        }
        let added = self.screens.give_built_ins(given.as_ref());
        // (the layouts first, so the list never names layouts the Library wasn't saved with)
        let saved = if added > 0 { self.settings.set(&crate::cores::keyboard_setting("screen.pages"), self.screens.to_json()["pages"].clone()) } else { Ok(()) };
        if let Err(e) = saved.and_then(|_| self.settings.set(&crate::cores::keyboard_setting("screen.built_ins"), all)) {
            eprintln!("ectomatter: the new screen layouts were not saved: {e}");
        }
    }

    /// The Library and the playlist into Ectomatter's settings, if they
    /// changed since last saved.
    pub(super) fn save_pages(&mut self) {
        self.note_changes();
        if self.screens == self.screens_saved {
            return;
        }
        let v = self.screens.to_json();
        let saved = self
            .settings
            .set(&crate::cores::keyboard_setting("screen.pages"), v["pages"].clone())
            .and_then(|_| self.settings.set(&crate::cores::keyboard_setting("screen.playlists"), v["playlists"].clone()))
            .and_then(|_| self.settings.set(&crate::cores::keyboard_setting("screen.plays"), v["plays"].clone()))
            .and_then(|_| self.settings.set(&crate::cores::keyboard_setting("screen.onboard"), v["onboard"].clone()));
        match saved {
            Ok(()) => {
                self.screens_saved = self.screens.clone();
                // said as a profile's saving is, with Undo beside it
                self.status("Saved", 1);
                self.ui.set_undo_shown(!self.undo.is_empty());
            }
            Err(e) => self.status(format!("The screen layouts and playlists were not saved: {e}"), 2),
        }
        self.pages_changed = None;
    }

    /// This computer's readings now, for the editor's picture of the page.
    pub(super) fn read_now(&mut self) {
        let mut r = self.reader.get_or_insert_with(|| {
            let mut s = crate::lcd_stats::Stats::of(self.hosted.readings(), "page");
            s.prime();
            s
        }).read();
        r.now = Some(crate::lcd_page::Moment::now());
        r.profile = self.page.dev.as_ref().map(|_| self.page.profile_name(self.page.active()));
        r.profile_number = self.page.dev.as_ref().map(|_| self.page.active());
        r.outside = self.outside();
        r.game = self.game_reading();
        self.readings = r;
        self.read_at = Some(Instant::now());
    }

    /// The screen's light while Ectomatter plays it: the page's own, or the
    /// profile's.
    pub(super) fn page_light(&self, profile: [u8; 3]) -> [u8; 3] {
        self.shown().and_then(|p| p.light).unwrap_or(profile)
    }

    /// The page on the keyboard's screen: the one being edited, or the
    /// page of the turn the playlist has come to.
    pub(super) fn shown(&self) -> Option<&crate::lcd_page::Page> {
        match &self.editor {
            Some(ed) => self.screens.library.get(ed.page),
            None => self.screens.turns(self.page.editing).get(self.lcd.page).and_then(|e| self.screens.page_of(e)),
        }
    }

    /// The playlist's turn on the keyboard's screen.
    pub(super) fn shown_page(&self) -> usize {
        self.lcd.page
    }

    /// The player said what the screen shows now.
    pub fn screen_news(&mut self, n: crate::lcd_player::News) {
        let state_changed = n.state != self.lcd.state || n.sequence != self.lcd.sequence || n.page != self.lcd.page;
        // a page coming on, or playing again after a pause, has its whole time
        if n.page != self.lcd.page || (n.state == crate::lcd_player::State::Playing && self.lcd.state != crate::lcd_player::State::Playing) {
            self.page_from = Instant::now();
        }
        self.lcd = n;
        // the picture of the keyboard shows each frame as the keyboard gets it (it read the
        // frames' file every two seconds, so it ran up to two seconds behind, Grant saw)
        if self.player_drives() {
            let rows = self.lcd.rows.clone();
            self.show_lcd(true, &rows);
        }
        self.set_playhead();
        if self.page.view == View::Screen && self.ui.get_place() == "keyboard" {
            if state_changed {
                self.refresh();
            } else {
                self.draw_screen();
            }
        }
    }

    /// The playhead, where the player last said the playlist stands: moving
    /// on its own from there while the playlist plays, still while it is
    /// paused; not shown for a single turn, while a script's screen shows or
    /// while a page is edited.
    pub(super) fn set_playhead(&mut self) {
        use crate::lcd_player::State;
        let n = &self.lcd;
        let shown = self.controlled && self.editor.is_none() && n.sequence.is_none() && matches!(n.state, State::Playing | State::Paused) && self.screens.turns(self.page.editing).len() > 1;
        // a turn that stays until the layouts are switched has no end to come to: the
        // playhead stands at its bar's end, by its infinity mark
        let stays = !n.turning && n.length.is_infinite();
        let len = if stays { 1.0 } else { n.length.max(0.001) };
        let tick = self.ui.get_pl_tick();
        self.ui.set_pl_head(shown);
        self.ui.set_pl_in(n.turning);
        self.ui.set_pl_len((len * 1000.0).round().max(1.0) as i64);
        self.ui.set_pl_t0(tick - (n.elapsed.min(len) * 1000.0).round() as i64);
        self.ui.set_pl_moving(shown && n.state == State::Playing && !stays);
        self.ui.set_pl_p0(if stays { 1.0 } else { (n.elapsed / len).clamp(0.0, 1.0) as f32 });
    }

    /// Whether the keyboard shows its own status screen for the page: while the Screen
    /// tab is open on it, in sight, and no page is edited, on a keyboard Ectomatter controls.
    /// Then the playlist stands aside, as it does for good once Ectomatter quits.
    pub(super) fn status_showing(&self) -> bool {
        self.controlled
            && self.page.dev.is_some()
            && !self.hidden
            && self.editor.is_none()
            && self.ui.get_place() == "keyboard"
            && self.page.view == View::Screen
            && self.ui.get_screen_closed()
    }

    /// The player told whether to stand aside, as the page now has it.
    pub(super) fn sync_aside(&mut self) {
        let aside = self.status_showing();
        if aside == self.aside {
            return;
        }
        self.aside = aside;
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::Aside(aside));
        }
    }

    /// The window closed into the tray, or shown again.
    pub fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
        self.sync_aside();
    }

    /// Ectomatter closing: the screen handed back to the keyboard's own screens.
    pub fn quit_screen(&mut self) {
        if let Some(p) = self.player.take() {
            p.quit(true);
        }
    }

    /// The screen layout editor's window opened on layout `i` of the Library (the window
    /// itself made by the window's side): the keyboard shows the layout and stays on it
    /// while the layout is edited.
    pub fn open_layouts(&mut self, i: usize) {
        self.close_menu();
        if !self.layouts_shown {
            self.edit_from = self.lcd.page;
            self.layouts_shown = true;
        }
        let i = i.min(self.screens.library.len().saturating_sub(1));
        let grid = self.editor.as_ref().map(|e| e.grid).unwrap_or_else(|| crate::uistate::get("screen_grid").and_then(|v| v.as_bool()).unwrap_or(true));
        self.editor = (!self.screens.library.is_empty()).then_some(crate::screen_edit::Editor { page: i, chosen: None, grid });
        self.read_now();
        self.hold_edited();
        self.show_layouts();
        if let Some(l) = &self.layouts {
            l.reveal(i);
        }
        self.refresh();
    }

    /// The keyboard's screen while the editor is open: the layout open, and the screen
    /// held on it.
    pub(super) fn hold_edited(&mut self) {
        self.tell_page();
        if let (Some(pl), true) = (&self.player, self.editor.is_some()) {
            pl.tell(crate::lcd_player::Control::Hold(Some(0)));
        }
    }

    /// The editor's window closed: the layouts saved, and the screen plays the playlist
    /// again, from the turn it was on.
    pub fn layouts_closed(&mut self) {
        if !std::mem::take(&mut self.layouts_shown) {
            return;
        }
        self.editor = None;
        self.save_pages();
        self.tell_page();
        if let Some(pl) = &self.player {
            pl.tell(crate::lcd_player::Control::Show(self.edit_from.min(self.screens.turns(self.page.editing).len().saturating_sub(1))));
            pl.tell(crate::lcd_player::Control::Hold(None));
        }
        self.refresh();
    }

    /// The editor window's asks, carried out on the layouts and the screen.
    pub fn act_layouts(&mut self) -> Vec<WindowAsk> {
        if self.layouts.is_none() {
            return Vec::new();
        }
        let avail = crate::lcd_page::available(&self.readings, self.outside().is_some());
        let light = self.layouts_light();
        let t = self.edit_t();
        let (onboard, profiles, from, groups) = (self.onboard_ready(), crate::cores::profiles().into_iter().map(|n| self.page.profile_name(n)).collect(), self.reading_from(), self.shows_groups());
        let Some(l) = &mut self.layouts else { return Vec::new() };
        let mut cx = crate::layout_page::Cx { screens: &mut self.screens, editor: &mut self.editor, readings: &self.readings, avail: &avail, light, t, onboard, profiles, from, groups };
        let out = l.act(&mut cx);
        l.show(&cx);
        if out.closed {
            self.layouts_closed();
        } else {
            if out.edited {
                self.pages_edited();
            }
            if out.opened {
                if let Some(i) = self.editor.as_ref().map(|e| e.page) {
                    if let Some(l) = &self.layouts {
                        l.reveal(i);
                    }
                }
                self.hold_edited();
            }
            if out.edited || out.opened {
                self.refresh();
            }
        }
        out.window
    }

    /// The editor's window shown as the layouts are now (each second too, for its readings).
    pub fn show_layouts(&mut self) {
        if self.layouts.is_none() {
            return;
        }
        let avail = crate::lcd_page::available(&self.readings, self.outside().is_some());
        let light = self.layouts_light();
        let t = self.edit_t();
        let (onboard, profiles, from, groups) = (self.onboard_ready(), crate::cores::profiles().into_iter().map(|n| self.page.profile_name(n)).collect(), self.reading_from(), self.shows_groups());
        let Some(l) = &mut self.layouts else { return };
        let cx = crate::layout_page::Cx { screens: &mut self.screens, editor: &mut self.editor, readings: &self.readings, avail: &avail, light, t, onboard, profiles, from, groups };
        l.show(&cx);
    }

    /// Seconds since the layout open in the editor came up (another layout opened starts
    /// the count again), which its scrolling words are timed from.
    pub(super) fn edit_t(&mut self) -> f64 {
        let Some(id) = self.editor.as_ref().and_then(|ed| self.screens.library.get(ed.page)).map(|p| p.id.clone()) else {
            self.edit_clock = None;
            return 0.0;
        };
        if self.edit_clock.as_ref().is_none_or(|(open, _)| *open != id) {
            self.edit_clock = Some((id, Instant::now()));
        }
        self.edit_clock.as_ref().map_or(0.0, |(_, since)| since.elapsed().as_secs_f64())
    }

    /// Whether the layout open in the editor has words scrolling now, so its pictures
    /// change between readings.
    pub(super) fn edit_moving(&self) -> bool {
        self.editor.as_ref().and_then(|ed| self.screens.library.get(ed.page)).is_some_and(|p| crate::lcd_page::moving(p, &self.readings))
    }

    /// The default backlight of the profile shown, for the editor's window: its
    /// brightness and color (until the keyboard's state has come, the screen's white).
    pub(super) fn layouts_light(&self) -> [u8; 4] {
        if self.page.dev.is_none() {
            return [255, 255, 255, 255];
        }
        screen_of(&self.page.draft()["lighting"]).0
    }

    /// Whether the editor's window changed under the pointer since it last moved (a block
    /// dropped or deleted where it rests), so what is under it is asked for again.
    pub fn layouts_rehover(&mut self) -> bool {
        self.layouts.as_mut().is_some_and(|l| l.take_rehover())
    }

    /// Whether words are being typed in the editor's window (its name, a Text block's words).
    pub fn typing_in_layouts(&self) -> bool {
        self.layouts.as_ref().is_some_and(|l| slint::ComponentHandle::global::<crate::Typing>(&l.ui).get_on())
    }

    /// What the screen is doing when the strip can't show it, said on the
    /// line over the screen: another program has it, or it isn't there.
    pub(super) fn screen_status(&self) -> String {
        use crate::lcd_player::State;
        match (self.lcd.state, self.lcd.sequence) {
            (_, _) if !self.controlled => "Ectomatter leaves this keyboard to other programs.".into(),
            (State::Missing, _) => "Ectomatter can’t reach the keyboard’s screen.".into(),
            // (the strip shows what plays, paused or not, and a script's screen)
            _ => String::new(),
        }
    }

    /// The playlist's panel: its chooser as wide as the playlist's name, the
    /// strip after it, each turn at its place (room made for a screen being
    /// carried, the strip fitted to its width unless a turn's end is being
    /// dragged); and the Library's cards. `profile` is the profile's screen light.
    pub(super) fn refresh_playlist(&mut self, profile: [u8; 3], level: u8) {
        use crate::lcd_player::State;
        let who = self.page.editing;
        let played = self.screens.played(who);
        let n = played.len();
        let sc = self.scale.max(1.0);
        let px = |w: f32| (w * sc).round() as u32;
        let lv = level as f32 / 255.0;
        let shown = self.shown_page();
        // the turn on the screen is marked while Ectomatter plays the screen, playing or paused
        let marked = self.controlled && self.editor.is_none() && matches!(self.lcd.state, State::Playing | State::Paused);
        // the panel's head: the playlist's name in its chooser (as wide as the name needs,
        // to its most) or the field it is renamed in; the strip Space.row after it, to
        // where the round starts again (Space.row on) and then Pause (Space.inline on, at
        // the panel's padding)
        use crate::tokens::{size, space};
        let under_w = self.ui.get_under_w() as f64;
        let name = self.screens.playlist(who).name.clone();
        let chooser = (crate::form::chooser_width(&name) as f64).min(CHOOSER_MOST as f64).ceil();
        let track_x = (space::PAD + space::ROW) as f64 + if self.pl_renaming { CHOOSER_MOST as f64 } else { chooser };
        let right = (space::PAD + size::COMPACT + space::INLINE + size::ROUND + space::ROW) as f64;
        let track_w = (under_w - right - track_x).max(TRACK_LEAST);
        self.ui.set_pl_name(name.as_str().into());
        self.ui.set_pl_chooser_most(CHOOSER_MOST);
        self.ui.set_pl_chooser_w(chooser as f32);
        self.ui.set_pl_track_x(track_x as f32);
        self.ui.set_pl_track_w(track_w as f32);
        // the turns shown and the room, in order: (the turn, or None for the room; its time)
        let carried = self.carry.as_ref().filter(|c| c.from_strip).map(|c| c.index);
        let mut items: Vec<(Option<usize>, f64)> = (0..n).filter(|i| Some(*i) != carried).map(|i| (Some(i), played[i].secs as f64)).collect();
        if let Some(at) = self.carry.as_ref().and_then(|c| c.slot) {
            items.insert(at.min(items.len()), (None, self.carry.as_ref().map_or(8.0, |c| c.secs as f64)));
        }
        // the empty holding bar at the end, its room kept from the turns' own; the whole strip
        // while the playlist is empty (and none while a carried layout's room takes its place)
        let hold = if n > 0 { HOLD_W } else { 0.0 };
        let fit_w = if n > 0 { track_w - HOLD_W - BETWEEN } else { track_w };
        let (pps, way, spans) = lay_strip(&items.iter().map(|x| x.1).collect::<Vec<_>>(), self.sizing.map(|s| (s.2, self.strip_way)), fit_w);
        self.strip_pps = pps;
        self.strip_way = way;
        self.ui.set_pl_way_w(way as f32);
        let end = spans.last().map_or(0.0, |(x, w)| x + w);
        let (hold_x, hold_w) = match (n, items.is_empty()) {
            (0, true) => (0.0, track_w - BETWEEN),
            (0, false) => (0.0, 0.0),
            _ => (end + BETWEEN, hold),
        };
        self.ui.set_pl_hold_x(hold_x as f32);
        self.ui.set_pl_hold_w(hold_w as f32);
        let room = items.iter().zip(&spans).find(|(it, _)| it.0.is_none()).map(|(_, sp)| *sp);
        self.ui.set_pl_slot_x(room.map_or(0.0, |r| r.0) as f32);
        self.ui.set_pl_slot_w(room.map_or(0.0, |r| r.1) as f32);
        self.strip_spans = items.iter().zip(&spans).filter(|(it, _)| it.0.is_some()).map(|(_, sp)| *sp).collect();
        let mut place: Vec<Option<(f64, f64)>> = vec![None; n];
        for (it, sp) in items.iter().zip(&spans) {
            if let Some(i) = it.0 {
                place[i] = Some(*sp);
            }
        }
        let old: HashMap<i32, (f32, f32)> = (0..self.blocks.row_count()).filter_map(|i| self.blocks.row_data(i)).map(|b| (b.key, (b.x, b.w))).collect();
        let mut rows = Vec::with_capacity(n);
        for (i, p) in played.iter().enumerate() {
            let e = &self.screens.turns(who)[i];
            let key = e.key as i32;
            // (a turn being carried keeps its place, unseen, until it lands)
            let (x, w) = place[i].map(|(x, w)| (x as f32, w as f32)).or(old.get(&key).copied()).unwrap_or((0.0, 0.0));
            rows.push(crate::PlBlock {
                key,
                x,
                w,
                name: p.name.as_str().into(),
                on: marked && i == shown,
                away: place[i].is_none(),
                secs: if n == 1 {
                    "Shows all the time".into()
                } else if e.stays {
                    STAYS.into()
                } else {
                    format!("{} s", e.secs).into()
                },
                time: if n > 1 && !e.stays { format!("{} s", e.secs).into() } else { "".into() },
                stays: e.stays,
                way: crate::lcd_page::TRANSITIONS.iter().position(|t| *t == e.comes_in).unwrap_or(0) as i32,
                way_name: e.comes_in.name().into(),
            });
        }
        sync_blocks(&self.blocks, rows);
        self.ui.set_pl_single(n == 1);
        self.ui.set_pl_empty(n == 0);
        // a script's screen on the keyboard: the strip waits
        let script = match self.lcd.sequence {
            Some("launch") => "A game is starting",
            Some(_) => "A game just closed",
            None => "",
        };
        self.ui.set_pl_dim(!script.is_empty() && self.controlled);
        self.ui.set_pl_script(script.into());
        // the Library: every page, its picture two pixels to a dot, its name, and what it
        // shows in at most two lines of its card's room
        let card_w = if self.ui.get_shelf_two() { (under_w - 32.0 - 8.0) / 2.0 } else { under_w - 32.0 };
        // (the words end short of the card's right edge, where Edit shows under the pointer)
        let words_w = (card_w - 358.0).max(60.0) as f32;
        self.pl_thumbs.resize(self.screens.library.len(), None);
        // (a layout carried along the Library: the others make room where it will land)
        let along = self.carry.as_ref().filter(|c| !c.from_strip).and_then(|c| Some((c.index, c.shelf?)));
        let places = shelf_places(self.screens.library.len(), along);
        self.ui.set_lib_gap(along.map_or(-1, |(_, to)| to as i32));
        let mut cards = Vec::with_capacity(self.screens.library.len());
        for (k, p) in self.screens.library.iter().enumerate() {
            let rgb = crate::keyboard::as_lit(p.light.unwrap_or(profile));
            let key: PicKey = (crate::lcd_page::render_still(p, &self.readings), rgb, level, px(260.0));
            let pic = match &self.pl_thumbs[k] {
                Some((had, img)) if *had == key => img.clone(),
                _ => {
                    let img = crate::lcd_draw::picture(&key.0, key.1, lv, px(260.0), px(52.0));
                    self.pl_thumbs[k] = Some((key, img.clone()));
                    img
                }
            };
            let (line1, line2) = two_lines(&p.about(), words_w);
            cards.push(crate::PlPage {
                name: p.name.as_str().into(),
                line1: line1.into(),
                line2: line2.into(),
                pic,
                field: crate::lcd_draw::field(rgb, lv, px(260.0), px(52.0)),
                on: false,
                now: false,
                paused: false,
                removable: false,
                at: places[k] as i32,
                carried: along.is_some_and(|(from, _)| from == k),
            });
        }
        show_rows(self.ui.get_lib_pages(), |m| self.ui.set_lib_pages(m), cards);
        // the layout asked about gone another way (from the editor's window): no question left
        if self.deleting.as_ref().is_some_and(|id| self.screens.index_of(id).is_none()) {
            self.deleting = None;
            self.ui.set_confirm_open(false);
        }
    }

    /// The screen drawn large, the playlist's picture of Stats, and its
    /// picture of a launch, each in the profile's screen light, when changed.
    pub(super) fn draw_screen(&mut self) {
        use crate::lcd_player::State;
        let state = match self.lcd.state {
            _ if !self.controlled => 4,
            State::Playing => 0,
            State::Paused => 1,
            State::Missing => 3,
        };
        self.ui.set_lcd_state(state);
        // (the screen's shape, as the keyboard's core describes it)
        let (cols, rows) = crate::lcd_page::size();
        self.ui.set_screen_cols(cols as i32);
        self.ui.set_screen_rows(rows as i32);
        if self.page.dev.is_none() {
            return;
        }
        let (light, _) = screen_of(&self.page.draft()["lighting"]);
        // the second tab: what the keyboard shows by itself, whatever the playlist plays. Its
        // onboard layout, from build 5 of its firmware on (Some, or Some(None) for the screen
        // off); before that, EVGA's status screen, in the default backlight
        let closed = self.ui.get_screen_closed() && self.editor.is_none();
        // the third tab: the layout of the row selected (or a layout carried over a row's bar,
        // as it would show there; else the starting game's), drawn with the game's readings
        let game_tab = self.ui.get_screen_game() && self.editor.is_none();
        let game_page = game_tab.then(|| {
            let launch = self.game_launch();
            let carried = self.gl_over.and_then(|_| self.carry.as_ref()).and_then(|c| self.screens.library.iter().find(|p| p.id == c.page)).cloned();
            carried.or_else(|| if self.gl_pick == Some(1) { launch.quit.layout } else { launch.start.layout })
        });
        let onboard = (closed && self.onboard_ready()).then(|| self.ob_page().cloned());
        let profile = [light[1], light[2], light[3]];
        let lit = match &onboard {
            Some(Some(p)) => p.light.unwrap_or(profile),
            // (no light at all)
            Some(None) => [0, 0, 0],
            None if closed => profile,
            None => self.page_light(profile),
        };
        // the light as the keyboard's screen shows it, bled as its backlight bleeds (and none
        // with the screen off)
        let level = if matches!(onboard, Some(None)) { 0 } else { light[0] };
        let rgb = crate::keyboard::as_lit(lit);
        let sc = self.scale.max(1.0);
        let px = |w: f32| (w * sc).round() as u32;
        // editing: the page as edited, read here; otherwise what the keyboard shows
        let t = self.edit_t();
        let rows = match self.editor.as_ref().and_then(|ed| self.screens.library.get(ed.page)) {
            Some(p) => crate::lcd_page::render_at(p, &self.readings, t),
            None if game_tab => match game_page.flatten() {
                Some(p) => crate::lcd_page::render_at(&p, &self.readings, t),
                None => Vec::new(),
            },
            None => match &onboard {
                Some(Some(p)) => crate::lcd_page::render_onboard(p, &self.onboard_readings()),
                Some(None) => Vec::new(),
                // (nothing sends the keyboard the time, so its clock's row is blank)
                None if closed => {
                    let d = self.page.draft();
                    crate::lcd_page::status_rows(d["lighting"]["screen"]["apps"].as_u64().unwrap_or(0), self.page.editing, d["name"].as_str().unwrap_or(""), None)
                }
                None => self.lcd.rows.clone(),
            },
        };
        // (as large as its box leaves it)
        let (lw, lh) = (self.ui.get_lcd_w(), self.ui.get_lcd_h());
        let key = (rows.clone(), rgb, level, px(lw));
        if self.lcd_drawn.as_ref() != Some(&key) {
            let lv = level as f32 / 255.0;
            self.ui.set_lcd_big(crate::lcd_draw::picture(&rows, rgb, lv, px(lw), px(lh)));
            self.ui.set_lcd_field(crate::lcd_draw::field(rgb, lv, px(lw), px(lh)));
            self.lcd_drawn = Some(key);
        }
        self.refresh_playlist([light[1], light[2], light[3]], light[0]);
    }

    /// The layout editor's window follows what the main window changes (the
    /// profile's default backlight, the layouts).
    pub(super) fn refresh_editor(&mut self) {
        if self.layouts_shown {
            self.show_layouts();
        }
    }
}
