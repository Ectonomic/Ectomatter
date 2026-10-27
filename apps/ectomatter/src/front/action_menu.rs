//! The action menu: what a key, a button or a slider can be set to do, as a menu of its own lists.

use super::*;

impl Front {
    pub(super) fn open_menu(&mut self, layer: usize, anchor: (f32, f32, f32), under: f32) {
        // the job's own kind of list, or the whole menu for a key that does nothing (a key
        // with a job Ectomatter does sends one of the codes a playlist could: its job's page)
        let page = self.page.sel.map(|pos| self.page.def(LAYERS[layer].0, pos)).and_then(|d| match if self.is_job_key(&d) { self.key_tag(&d) } else { keyboard::kind_of(&d) } {
            "Key" => Some(0),
            "Media" => Some(1),
            "Mouse" => Some(2),
            "Macro" => Some(MACRO_PAGE),
            "Profile" => Some(3),
            "Screen" => keyboard::screen_job(&d).map(screen_page),
            "Keyboard" => Some(6),
            _ => None,
        });
        self.open_menu_on(layer, anchor, page, under);
    }

    pub(super) fn open_menu_on(&mut self, layer: usize, anchor: (f32, f32, f32), page: Option<usize>, under: f32) {
        let Some(pos) = self.page.sel else { return };
        // the second click on the button whose menu a press just closed
        if self.just_shut(anchor) {
            return;
        }
        if self.menu.as_ref().is_some_and(|m| m.layer == layer && m.page == page) {
            self.close_menu();
            return;
        }
        self.listening = None;
        self.menu = Some(Menu { layer, pos, page, query: String::new(), anchor, choices: Vec::new() });
        self.ui.set_menu_query("".into());
        // (with its search box, one width as it's searched: Size.menu-search, or its field's;
        // a column for the tick on the key's own job)
        self.ui.set_menu_search(true);
        self.ui.set_menu_marks(true);
        self.ui.set_menu_w(crate::tokens::size::MENU_SEARCH.max(under));
        self.build_menu();
        // under the button when it fits there, or when there is a fair amount of
        // room there anyway (the menu then scrolls); over it only when below is
        // cramped. Where it hangs is fixed now, so it stays put as it is searched.
        let (_, ay, ah) = anchor;
        let (h, win) = (self.ui.get_menu_natural(), self.ui.get_window_height());
        let (top, most) = crate::menu_size::hang(win, (ay, ah), h);
        if std::env::var_os("ECTOMATTER_DEBUG").is_some() {
            eprintln!("menu: natural {h} window {win} anchor {ay}+{ah} top {top} most {most}");
        }
        self.ui.set_menu_top(top);
        self.ui.set_menu_max(most);
        self.ui.set_menu_open(true);
        // (turned to what the key does now, where that's out of view)
        let rows: Vec<MenuRow> = self.ui.get_menu_rows().iter().collect();
        let head = crate::menu_size::height(&[], true, self.ui.get_menu_back(), 0.0);
        self.ui.invoke_scroll_menu(crate::menu_size::reveal(&rows, h.min(most) - head));
        self.ui.invoke_focus_menu();
    }

    pub fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    pub(super) fn close_menu(&mut self) {
        // (one of the scene form's dropdowns: shut, and drawn shut)
        if self.scene_form_menu.take().is_some() {
            self.ui.set_menu_open(false);
            self.show_scene_form();
        }
        if self.bind_menu.take().is_some() || self.light_menu.take().is_some() || self.way_menu.take().is_some() || self.time_menu.take().is_some() || std::mem::take(&mut self.pl_menu) || std::mem::take(&mut self.add_menu) || std::mem::take(&mut self.ob_menu) || self.lib_menu.take().is_some() || self.new_menu.take().is_some() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            self.ui.set_pl_menu(-1);
            self.ui.set_pl_timing(-1);
            self.ui.set_pl_choosing(false);
            self.ui.set_pl_adding(false);
            self.ui.set_ob_choosing(false);
        }
        if self.menu.take().is_some() {
            self.ui.set_menu_open(false);
            self.ui.invoke_blur_menu();
        }
        if self.mouse.menu.take().is_some() || std::mem::take(&mut self.profile_menu) || std::mem::take(&mut self.kb_profile_menu) || self.apps_menu.take().is_some() || self.scene_menu.take().is_some() || self.pad_menu.take().is_some() || self.games.menu.take().is_some() || self.until_menu.take().is_some() {
            self.ui.set_menu_open(false);
            self.ui.set_mouse_menu_open(false);
        }
        // (the next menu keeps a column for ticks, unless it has none)
        self.ui.set_menu_marks(true);
    }

    /// A press somewhere in the window, in its logical pixels: a press away
    /// from the open menu closes it, and a press on the button it hangs from
    /// keeps that button's click from opening it again.
    pub fn pressed_at(&mut self, x: f32, y: f32) {
        self.press_shut = false;
        self.press_closed = None;
        self.let_chosen_go(x, y);
        if self.search_menu {
            let (mx, my, mw, mh) = (self.ui.get_menu_x(), self.ui.get_menu_y(), self.ui.get_menu_width(), self.ui.get_menu_height());
            if !(x >= mx && x < mx + mw && y >= my && y < my + mh) {
                self.close_search_menu();
            }
        }
        // a press away from the playlist's name being changed keeps the name
        if self.pl_renaming {
            let (rx, ry) = (self.ui.get_pl_rename_wx(), self.ui.get_pl_rename_wy());
            if !(x >= rx && x < rx + 195.0 && y >= ry && y < ry + 28.0) {
                let typed = self.ui.get_pl_rename_text().to_string();
                self.end_rename(true, &typed);
            }
        }
        // (every menu the window hangs, whatever opened it, by one rule: a press off it shuts
        // it; the key's menu, which has a find box of its own, is shut below)
        if self.ui.get_menu_open() && self.menu.is_none() {
            let (mx, my, mw, mh) = (self.ui.get_menu_x(), self.ui.get_menu_y(), self.ui.get_menu_width(), self.ui.get_menu_height());
            if !(x >= mx && x < mx + mw && y >= my && y < my + mh) {
                // (a card's menu shut by a press: if the press is on the same card, its click
                // closes the menu rather than opening it again; New screen layout's place is
                // after the last card's)
                if let Some(k) = self.lib_menu.or(self.new_menu.as_ref().map(|_| self.screens.library.len())) {
                    self.card_shut = Some((k, Instant::now()));
                }
                // a press on the dropdown (or the bar) the menu hangs from shuts it, and its click must not open it again
                let (ax, ay, ah) = (self.ui.get_menu_ax(), self.ui.get_menu_ay(), self.ui.get_menu_ah());
                let aw = if self.way_menu.is_some() {
                    self.strip_way as f32
                } else if let Some(t) = &self.time_menu {
                    t.aw
                } else if self.pl_menu {
                    self.ui.get_pl_chooser_w()
                } else if self.add_menu {
                    self.ui.get_pl_hold_w()
                } else if self.ob_menu {
                    self.ui.get_ob_chooser_w()
                } else {
                    self.ui.get_menu_w()
                };
                if x >= ax && x < ax + aw && y >= ay && y < ay + ah {
                    self.shut = Some(((ax, ay), Instant::now()));
                }
                self.press_closed = Some(Instant::now());
                self.close_menu();
                self.refresh();
            }
            return;
        }
        // away from the open picker closes it; on the swatch it hangs from too
        if self.pick.is_some() {
            let (px, py, ph) = (self.ui.get_picker_x().max(crate::tokens::space::INLINE).min(self.ui.get_window_width() - crate::tokens::size::PICKER - crate::tokens::space::INLINE), self.ui.get_picker_top(), self.ui.get_picker_height());
            if !(x >= px && x < px + crate::tokens::size::PICKER && y >= py && y < py + ph) {
                let (ax, ay, ah) = self.pick.as_ref().unwrap().anchor;
                // (a swatch is 34 wide; a pad's Color button, its swatch and its word)
                let aw = if self.pick.as_ref().is_some_and(|p| p.ctl == Ctl::DeskPad) { crate::text::width(crate::tokens::font::MEDIUM, crate::tokens::font::CONTROL, "Color") + 42.0 } else { 34.0 };
                if x >= ax && x < ax + aw && y >= ay && y < ay + ah {
                    self.shut = Some(((ax, ay), Instant::now()));
                }
                self.close_picker();
                self.press_shut = true;
                self.press_closed = Some(Instant::now());
            }
        }
        let Some(m) = &self.menu else { return };
        let (mx, my, mw, mh) = (self.ui.get_menu_x(), self.ui.get_menu_y(), self.ui.get_menu_width(), self.ui.get_menu_height());
        if x >= mx && x < mx + mw && y >= my && y < my + mh {
            return;
        }
        let (ax, ay, ah) = m.anchor;
        if x >= ax && x < ax + 300.0 && y >= ay && y < ay + ah {
            self.shut = Some(((ax, ay), Instant::now()));
        }
        self.close_menu();
    }

    /// The rule for whatever a page has chosen (a key on the keyboard, a
    /// button on the mouse), the same on every page: selecting it again lets
    /// it go, Escape lets it go, and so does a press anywhere off it. A press
    /// keeps it chosen only on what chooses (where a press chooses another,
    /// or the same one again) and on its editor; a press while a menu or the
    /// color picker is open only closes that; one on the title bar only moves
    /// the window.
    pub(super) fn let_chosen_go(&mut self, x: f32, y: f32) {
        // (a press while any menu is open is the menu's: it chooses there, or shuts it)
        if y < 36.0 || self.ui.get_menu_open() || self.menu.is_some() || self.mouse.menu.is_some() || self.profile_menu || self.kb_profile_menu || self.apps_menu.is_some() || self.scene_menu.is_some() || self.scene_form_menu.is_some() || self.pad_menu.is_some() || self.bind_menu.is_some() || self.light_menu.is_some() || self.way_menu.is_some() || self.pick.is_some() || self.search_menu {
            return;
        }
        let Some(keeps) = self.chosen_keeps() else { return };
        if keeps.iter().any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h) {
            return;
        }
        // (a button on the mouse's picture chooses too)
        if self.ui.get_place() == "mouse" && self.mouse_pic_at_window(x, y).0.is_some() {
            return;
        }
        self.let_chosen_go_now();
        self.refresh();
    }

    /// Where a press keeps the page's chosen thing chosen, in the window's
    /// logical pixels; none when nothing is chosen.
    pub(super) fn chosen_keeps(&self) -> Option<Vec<(f32, f32, f32, f32)>> {
        let ui = &self.ui;
        match ui.get_place().as_str() {
            // the keyboard (a key there is chosen instead, and the chassis
            // lets it go itself) and the key's editor
            "keyboard" if self.page.view == View::Keys && self.page.sel.is_some() => {
                let mut k = Vec::new();
                if ui.get_board_shown() {
                    k.push((ui.get_board_x(), ui.get_board_y(), ui.get_board_width(), ui.get_board_height()));
                }
                if ui.get_key_open() {
                    k.push((ui.get_key_tile_x(), ui.get_key_tile_y(), ui.get_key_tile_w(), ui.get_key_tile_h()));
                }
                Some(k)
            }
            // each button's words beside the picture (inside the stage, which
            // clips them), the list and the button's Key detail box
            "mouse" if self.mouse.tab == 0 && self.mouse.button.is_some() => {
                let (x0, y0, w) = (ui.get_mouse_wx(), ui.get_mouse_wy(), ui.get_mouse_w());
                let (sy, sh) = (y0 + ui.get_mouse_stage_y(), ui.get_mouse_stage_height());
                let canvas = x0 + w / 2.0 - 600.0;
                let mut k: Vec<(f32, f32, f32, f32)> = crate::mouse::shape()
                    .buttons
                    .iter()
                    .map(|b| (&b.callout.0, &b.callout.1))
                    .map(|(left, top)| {
                        let (lx, ly) = (canvas + if *left { 220.0 } else { 780.0 }, sy + top);
                        let (a, b) = (lx.max(x0), (lx + 200.0).min(x0 + w));
                        let (c, d) = (ly.max(sy), (ly + 40.0).min(sy + sh));
                        (a, c, (b - a).max(0.0), (d - c).max(0.0))
                    })
                    .collect();
                // (the row of boxes right of the Profiles box: the Key library and the Key detail box)
                let (ly, lh) = (y0 + ui.get_mouse_low_top(), ui.get_mouse_low_height());
                k.push((ui.get_mouse_row_wx(), ly, ui.get_mouse_row_width(), lh));
                Some(k)
            }
            // a pad on Home's desk: the whole desk (where a press chooses
            // another, carries, or draws, and on the empty desk or a device
            // lets it go itself), its bar and its zoom over it
            "home" if self.desk.pad_chosen.is_some() => Some(vec![(ui.get_desk_x(), ui.get_desk_y(), ui.get_desk_w(), ui.get_desk_h())]),
            _ => None,
        }
    }

    /// The page's chosen thing let go.
    pub(super) fn let_chosen_go_now(&mut self) {
        match self.ui.get_place().as_str() {
            "keyboard" => {
                self.page.sel = None;
                self.listening = None;
            }
            "mouse" => self.mouse.button = None,
            "home" => {
                self.desk.pad_chosen = None;
                self.show_desk();
            }
            _ => {}
        }
        self.close_menu();
    }

    /// Escape, with nothing being typed: the page's chosen thing let go, if
    /// it has one (false when it has none, for Escape to go on to the page).
    pub fn escape(&mut self) -> bool {
        // (on the Library, Escape shuts its menu, then its form, then goes back
        // from a page of its own to the Library)
        if self.ui.get_place() == "library" {
            if self.games.menu.is_some() {
                self.close_menu();
                self.refresh();
                return true;
            }
            if self.games.escape() {
                self.close_menu();
                self.refresh();
                return true;
            }
        }
        // (on Home, Escape shuts the color picker open on a pad, then lets the
        // chosen pad go, and then leaves edit mode, as Done does)
        if self.ui.get_place() == "home" && self.pick.is_some() {
            self.close_picker();
            return true;
        }
        if self.ui.get_place() == "home" && self.desk.draw_tool {
            self.desk.draw_tool = false;
            self.show_desk();
            return true;
        }
        if self.ui.get_place() == "home" && self.desk.pad_chosen.is_some() {
            self.let_chosen_go_now();
            return true;
        }
        if self.ui.get_place() == "home" && self.desk.editing {
            self.desk.set_editing(false);
            self.desk_frames_run();
            self.show_desk();
            return true;
        }
        if self.chosen_keeps().is_none() || self.menu.is_some() || self.bind_menu.is_some() || self.light_menu.is_some() || self.way_menu.is_some() || self.pick.is_some() || self.search_menu {
            return false;
        }
        self.let_chosen_go_now();
        self.refresh();
        true
    }

    pub(super) fn menu_chose(&mut self, i: usize) {
        if let Some(row) = self.until_menu {
            self.close_menu();
            let choices = Self::until_choices(row);
            if let Some((v, _)) = choices.get(i) {
                self.set_game_launch(Self::launch_key(row as i32), "until", serde_json::json!(v));
            }
            self.refresh();
            return;
        }
        if let Some(m) = self.games.menu.clone() {
            self.close_menu();
            self.games.menu = Some(m);
            self.games.chose(i);
            return;
        }
        if self.profile_menu {
            let anchor = (self.ui.get_menu_ax(), self.ui.get_menu_ay(), self.ui.get_menu_ah());
            self.close_menu();
            match i {
                // Duplicate: a copy, its name open to change
                0 => {
                    self.mouse.duplicate_profile();
                    self.ui.set_mouse_renaming(true);
                }
                // Use with an app: the apps it can follow
                1 => {
                    if let Some(at) = self.mouse.profiles.as_ref().map(|s| s.active) {
                        self.open_apps_menu(1, at, anchor);
                        return;
                    }
                }
                // Delete, unless it's the only one (that row is dimmed)
                3 if self.mouse.profiles.as_ref().is_some_and(|p| p.list.len() > 1) => {
                    self.mouse.delete_profile();
                    self.show_mouse_lights();
                }
                _ => {}
            }
            self.refresh();
            return;
        }
        if let Some(rows) = self.scene_menu.clone() {
            self.close_menu();
            match rows.get(i) {
                Some(SceneRow::Off) => self.set_scene(None, true),
                // (selecting the scene that's on turns it off, as in iCUE)
                Some(SceneRow::Scene(id)) => {
                    let id = (self.scene.as_ref() != Some(id)).then(|| id.clone());
                    self.set_scene(id, true);
                }
                Some(SceneRow::New) => self.open_scene_form(None),
                _ => {}
            }
            self.refresh();
            return;
        }
        if let Some(m) = self.scene_form_menu.clone() {
            self.close_menu();
            self.scene_form_chose(&m, i);
            self.refresh();
            return;
        }
        if let Some(which) = self.pad_menu {
            match self.pad_rows(which).1.get(i).copied().flatten() {
                Some(PadRow::Size(k)) => {
                    self.close_menu();
                    match which {
                        PadMenu::Add => self.desk.add_pad(k),
                        PadMenu::Size => {
                            let (_, _, w, h) = crate::desk::SIZES[k];
                            self.desk.set_pad_size(Some(w), Some(h));
                        }
                    }
                    self.desk_frames_run();
                }
                Some(PadRow::Draw) => {
                    self.close_menu();
                    self.desk.draw_tool = true;
                    self.desk.pad_chosen = None;
                }
                // (a heading, the rule, or a field's name: the menu stays)
                _ => return,
            }
            self.show_desk();
            return;
        }
        if self.kb_profile_menu {
            let anchor = (self.ui.get_menu_ax(), self.ui.get_menu_ay(), self.ui.get_menu_ah());
            self.close_menu();
            if i == 0 {
                self.open_apps_menu(0, self.page.editing as usize, anchor);
                return;
            }
            self.refresh();
            return;
        }
        if let Some((device, profile, anchor)) = self.apps_menu {
            // (an app ticked or not, the menu open again for the next)
            let tied = self.tied_apps(device, profile);
            let mut apps: Vec<String> = self.front_apps.list.iter().map(|a| a.id.clone()).collect();
            for t in &tied {
                if !apps.contains(t) {
                    apps.push(t.clone());
                }
            }
            if let Some(app) = i.checked_sub(1).and_then(|k| apps.get(k)).cloned() {
                self.tie_app(device, profile, &app, !tied.contains(&app));
                self.open_apps_menu(device, profile, anchor);
                self.refresh();
                return;
            }
            self.close_menu();
            self.refresh();
            return;
        }
        if self.mouse.menu.is_some() {
            // (the row's job as it was offered: a heading's row has none)
            let pick = self.mouse.menu_picks.get(i).cloned().flatten();
            self.close_menu();
            if let (Some(a), Some(b)) = (pick, self.mouse.button) {
                self.mouse.set_job(b, a);
            }
            self.refresh();
            return;
        }
        if self.search_menu {
            self.close_search_menu();
            if let (Some(f), Some((plugin, setting, _))) = (self.found.0.get(i).cloned(), self.typing.take()) {
                self.set_plugin_value(&plugin, &setting, f["value"].clone());
            }
            return;
        }
        if let Some(k) = self.light_menu.take() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            // (a rule, or the value no option has, after a rule of its own, changes nothing)
            if let Some((id, d, _)) = self.page.described(crate::described::Piece::Effect(k)) {
                if let Some(v) = crate::described::menu_pick(&d, i) {
                    self.page.change_settings(|e| crate::described::keep(e, &id, &d, &v));
                    self.push_preview();
                }
            }
            self.refresh();
            return;
        }
        if let Some((s, _)) = self.bind_menu.take() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            if let Some((v, _)) = i.checked_sub(1).and_then(|k| self.slider_choices(s).get(k).cloned()) {
                self.set_slider_bind(s, &v);
            }
            return;
        }
        // the onboard layout's chooser: the screen off, a layout for the keyboard to keep, or
        // the one it keeps opened in the editor
        if self.ob_menu {
            self.close_menu();
            let n = self.page.editing;
            let count = self.screens.library.len();
            match i {
                1 => {
                    self.screens.keep_onboard(n, None);
                    self.pages_edited();
                }
                k if (2..2 + count).contains(&k) => {
                    let id = self.screens.library[k - 2].id.clone();
                    self.screens.keep_onboard(n, Some(&id));
                    self.pages_edited();
                }
                k if k == 3 + count => {
                    if let Some(at) = self.screens.onboard_of(n).and_then(|p| self.screens.index_of(&p.id)) {
                        self.pending_window.push(WindowAsk::OpenLayouts(at));
                    }
                }
                _ => {}
            }
            self.refresh();
            return;
        }
        // the panel's menu: another playlist for the profile, a new one, this one
        // renamed in place or deleted
        if self.pl_menu {
            self.close_menu();
            let n = self.page.editing;
            let count = self.screens.playlists.len();
            let mine = self.screens.of_profile(n);
            match i {
                k if (1..=count).contains(&k) => {
                    if k - 1 != mine {
                        let id = self.screens.playlists[k - 1].id.clone();
                        self.screens.choose(n, &id);
                        self.pages_edited();
                        self.play_from_start();
                    }
                }
                k if k == count + 2 => {
                    self.screens.new_playlist(n);
                    self.pages_edited();
                    self.play_from_start();
                }
                k if k == count + 3 => {
                    self.pl_renaming = true;
                    self.ui.set_pl_rename_text(self.screens.playlists[mine].name.as_str().into());
                    self.ui.set_pl_renaming(true);
                }
                k if k == count + 4 => {
                    let id = self.screens.playlists[mine].id.clone();
                    if self.screens.delete(&id) {
                        self.pages_edited();
                        self.play_from_start();
                    }
                }
                _ => {}
            }
            self.refresh();
            return;
        }
        // New screen layout's menu: a blank layout, a copy of one, or one Ectomatter ships put
        // back, opened in the editor
        if let Some(made) = self.new_menu.clone() {
            self.close_menu();
            let at = match made.get(i).cloned().flatten() {
                Some(NewFrom::Scratch) => Some(self.screens.new_page()),
                Some(NewFrom::Copy(id)) => self.screens.index_of(&id).and_then(|k| self.copy_layout(k)),
                Some(NewFrom::Ready(key)) => self.screens.add_built_in(&key),
                None => None,
            };
            if let Some(at) = at {
                self.pages_edited();
                self.pending_window.push(WindowAsk::OpenLayouts(at));
            }
            self.refresh();
            return;
        }
        // a layout card's menu: edit it, add it to the playlist (or keep it on the keyboard, on
        // the Onboard layout tab), copy it, or delete it once asked (Undo puts it back)
        if let Some(k) = self.lib_menu {
            self.close_menu();
            let Some(id) = self.screens.library.get(k).map(|p| p.id.clone()) else { return };
            let n = self.page.editing;
            match i {
                0 => self.pending_window.push(WindowAsk::OpenLayouts(k)),
                1 if self.ui.get_screen_closed() => {
                    self.screens.keep_onboard(n, Some(&id));
                    self.pages_edited();
                }
                1 => {
                    let shown = self.lcd.page;
                    let playing = self.screens.turns(n).get(shown).map(|e| e.key);
                    let end = self.screens.turns(n).len();
                    self.screens.add_turn(n, end, &id);
                    self.pages_edited();
                    self.keep_playing(playing, shown);
                }
                2 => {
                    if self.copy_layout(k).is_some() {
                        self.pages_edited();
                    }
                }
                4 => self.ask_delete(k),
                _ => {}
            }
            self.refresh();
            return;
        }
        // the holding bar's menu: a layout put on the end of the playlist
        if self.add_menu {
            self.close_menu();
            if let Some(id) = i.checked_sub(1).and_then(|k| self.screens.library.get(k)).map(|p| p.id.clone()) {
                let n = self.page.editing;
                let shown = self.lcd.page;
                let playing = self.screens.turns(n).get(shown).map(|e| e.key);
                let end = self.screens.turns(n).len();
                self.screens.add_turn(n, end, &id);
                self.pages_edited();
                self.keep_playing(playing, shown);
            }
            self.refresh();
            return;
        }
        // how long a turn shows, from the menu on its time
        if let Some(t) = self.time_menu.take() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            self.ui.set_pl_timing(-1);
            if let (Some(pick), Some(e)) = (i.checked_sub(1).and_then(|k| t.picks.get(k)), self.screens.turns_mut(self.page.editing).get_mut(t.turn)) {
                let (secs, stays) = match pick {
                    Some(secs) => (*secs, false),
                    None => (e.secs, true),
                };
                if (e.secs, e.stays) != (secs, stays) {
                    (e.secs, e.stays) = (secs, stays);
                    self.pages_edited();
                }
            }
            self.refresh();
            return;
        }
        // how a turn comes in, from the menu on its way bar
        if let Some(t) = self.way_menu.take() {
            self.ui.set_menu_open(false);
            self.ui.set_menu_search(true);
            self.ui.set_pl_menu(-1);
            if let (Some(way), Some(e)) = (i.checked_sub(1).and_then(|k| crate::lcd_page::TRANSITIONS.get(k)), self.screens.turns_mut(self.page.editing).get_mut(t)) {
                if e.comes_in != *way {
                    e.comes_in = *way;
                    self.pages_edited();
                }
            }
            self.refresh();
            return;
        }
        let Some(m) = &mut self.menu else { return };
        match m.choices.get(i) {
            Some(Choice::Set(d)) => {
                let (layer, pos, d) = (m.layer, m.pos, d.clone());
                self.close_menu();
                // a macro keeps playing the way it already plays on its other keys
                let d = self.page.with_macro_play(&d);
                if self.page.sel == Some(pos) {
                    self.page.set_def(LAYERS[layer].0, pos, &d);
                }
            }
            Some(Choice::Page(p)) => {
                m.page = Some(*p);
                m.query.clear();
                self.ui.set_menu_query("".into());
                self.build_menu();
            }
            Some(Choice::Play(id)) => {
                let (layer, pos, id) = (m.layer, m.pos, id.clone());
                self.close_menu();
                if let Some(code) = self.play_code(&id) {
                    if self.page.sel == Some(pos) {
                        self.page.set_def(LAYERS[layer].0, pos, &keyboard::screen_slot(code));
                    }
                }
            }
            Some(Choice::Job(id)) => {
                let (layer, pos, id) = (m.layer, m.pos, format!("{}{id}", crate::jobs::ON_KEY));
                self.close_menu();
                if let Some(code) = self.play_code(&id) {
                    if self.page.sel == Some(pos) {
                        self.page.set_def(LAYERS[layer].0, pos, &keyboard::screen_slot(code));
                    }
                }
            }
            _ => {}
        }
    }

    /// The code a key plays playlist `id` with (see screen_keys::code_for), kept in Ectomatter's
    /// settings once the playlist has one; None while every code is some key's.
    pub(super) fn play_code(&mut self, id: &str) -> Option<u32> {
        let lists = &self.screens.playlists;
        // (a code kept for one of Ectomatter's jobs is as much in use as a playlist's)
        let code = crate::screen_keys::code_for(id, &self.keyed, &self.page.sent_codes(), |p| p.starts_with(crate::jobs::ON_KEY) || lists.iter().any(|l| l.id == p))?;
        if self.keyed.get(&code).map(String::as_str) != Some(id) {
            self.keyed.insert(code, id.to_string());
            let kept: serde_json::Map<String, serde_json::Value> = self.keyed.iter().map(|(c, id)| (c.to_string(), serde_json::json!(id))).collect();
            if let Err(e) = self.settings.set(&crate::cores::keyboard_setting("screen.keyed"), serde_json::Value::Object(kept)) {
                self.status(format!("The key's playlist was not saved: {e}"), 2);
            }
            self.sync_playlist_keys();
        }
        Some(code)
    }

    /// The keyboard page's names for the playlists keys can play, by their codes (a code whose
    /// playlist was deleted left out).
    pub(super) fn sync_playlist_keys(&mut self) {
        let lists = &self.screens.playlists;
        self.page.playlist_keys = self.keyed.iter().filter_map(|(c, id)| Some((*c, lists.iter().find(|l| &l.id == id)?.name.clone()))).collect();
        // (and the codes Ectomatter's own jobs have, in their words)
        let names = self.job_names();
        let jobs: Vec<(u32, Option<crate::jobs::Job>)> = self.keyed.iter().filter_map(|(c, id)| Some((*c, crate::jobs::Job::of(id.strip_prefix(crate::jobs::ON_KEY)?)))).collect();
        self.page.job_keys = jobs.iter().map(|(c, j)| (*c, j.as_ref().map_or_else(|| "Deleted macro".to_string(), |j| names.of(j)))).collect();
        self.page.job_prints = jobs
            .iter()
            .map(|(c, j)| {
                let print = match j {
                    Some(crate::jobs::Job::Profile { device, to: crate::jobs::To::Id(id) }) => names.device(device).and_then(|d| d.profiles.iter().find(|(k, _)| k == id)).map_or_else(|| "Deleted".to_string(), |(_, n)| n.clone()),
                    Some(crate::jobs::Job::Macro { n, .. }) => names.macros.iter().find(|(k, _)| k == n).map_or_else(|| "Deleted".to_string(), |(_, n)| n.clone()),
                    Some(crate::jobs::Job::Scene(crate::jobs::SceneTo::Id(id))) => names.scenes.iter().find(|(k, _)| k == id).map_or_else(|| "Deleted".to_string(), |(_, n)| n.clone()),
                    Some(j) => names.of(j),
                    None => "Deleted".into(),
                };
                (*c, print)
            })
            .collect();
    }

    /// What a key given one of Ectomatter's own jobs (`id`, see jobs.rs) does,
    /// as the note under its job says it.
    pub(super) fn job_note(&self, id: &str) -> String {
        use crate::jobs::{Job, To};
        let names = self.job_names();
        let Some(j) = Job::of(id) else {
            return "The macro this key played was deleted, so the key does nothing until you give it another job.".into();
        };
        match &j {
            Job::Profile { device, to } => match (names.device(device), to) {
                (None, _) => "The device this key switched isn’t here, so the key does nothing until it’s back.".into(),
                (Some(d), To::Id(p)) => match d.profiles.iter().find(|(k, _)| k == p) {
                    Some((_, name)) => format!("Switches {} to {name} while Ectomatter is running.", d.name),
                    None => format!("The {} profile this key switched to was deleted, so the key does nothing until you give it another job.", d.name),
                },
                (Some(d), To::Next) => format!("Switches {} to its next profile while Ectomatter is running.", d.name),
                (Some(d), To::Previous) => format!("Switches {} to its previous profile while Ectomatter is running.", d.name),
            },
            Job::Macro { .. } => format!("Ectomatter plays {} while it’s running. With Ectomatter closed, this key does nothing.", names.of(&j)),
            Job::Screen(_) => format!("{} while Ectomatter is running.", names.of(&j)),
            Job::Scene(crate::jobs::SceneTo::Id(p)) => match names.scenes.iter().find(|(k, _)| k == p) {
                Some((_, name)) => format!("Turns on the {name} scene while Ectomatter is running. Pressed again, the key turns the scene off."),
                None => "The scene this key turned on was deleted, so the key does nothing until you give it another job.".into(),
            },
            Job::Scene(crate::jobs::SceneTo::Next) => "Switches to the next scene while Ectomatter is running.".into(),
            Job::Scene(crate::jobs::SceneTo::Previous) => "Switches to the previous scene while Ectomatter is running.".into(),
            Job::Scene(crate::jobs::SceneTo::Off) => "Turns the scene off while Ectomatter is running.".into(),
        }
    }

    /// The job a key's action is, if it is one of Ectomatter's own (its host
    /// slot's code kept for one; see jobs.rs).
    pub(super) fn key_job(&self, a: &Action) -> Option<crate::jobs::Job> {
        let code = keyboard::screen_job(a)?;
        crate::jobs::Job::of(self.keyed.get(&code)?.strip_prefix(crate::jobs::ON_KEY)?)
    }

    /// Whether a key's action is one of Ectomatter's own jobs' codes, a job
    /// deleted since included.
    pub(super) fn is_job_key(&self, a: &Action) -> bool {
        keyboard::screen_job(a).and_then(|c| self.keyed.get(&c)).is_some_and(|id| id.starts_with(crate::jobs::ON_KEY))
    }

    /// What kind of job a key has, as its kind switch and the menu's pages
    /// tell it: one of Ectomatter's own, by what it does (another device's
    /// profile is a profile, Ectomatter's macro a macro).
    pub(super) fn key_tag(&self, a: &Action) -> &'static str {
        match self.key_job(a) {
            Some(crate::jobs::Job::Profile { .. } | crate::jobs::Job::Scene(_)) => "Profile",
            Some(crate::jobs::Job::Macro { .. }) => "Macro",
            Some(crate::jobs::Job::Screen(_)) => "Screen",
            None if self.is_job_key(a) => "Macro",
            None => keyboard::kind_of(a),
        }
    }

    /// Other devices' jobs, Ectomatter's own macros and the scenes' jobs, as
    /// the action menu lists them: each other device's (by its name, as they
    /// sit on the desk), the macros, and the scenes'. Each one's row (its
    /// definition, for its tick, once it has a code), what choosing it does,
    /// its row's kind (5 while every code is some key's), and its words under
    /// its heading (Names::short).
    #[allow(clippy::type_complexity)]
    pub(super) fn device_jobs(&self) -> (Vec<(String, Vec<(Entry, Choice, i32, String)>)>, Vec<(Entry, Choice, i32, String)>, Vec<(Entry, Choice, i32, String)>) {
        let sent = self.page.sent_codes();
        let lists = &self.screens.playlists;
        let (mut groups, mut macros, mut scenes) = (Vec::<(String, String, Vec<(Entry, Choice, i32, String)>)>::new(), Vec::new(), Vec::new());
        let names = self.job_names();
        let mut jobs = names.for_device(crate::jobs::keyboard());
        jobs.extend(names.macros.iter().map(|(n, _)| crate::jobs::Job::Macro { n: *n, plays: ectomatter_protocol::Plays::Once }).map(|j| (names.of(&j), j)));
        for (words, j) in jobs {
            let short = names.short(&j);
            let id = format!("{}{}", crate::jobs::ON_KEY, j.id());
            let code = self.keyed.iter().find(|(_, k)| **k == id).map(|(c, _)| *c);
            let can = code.is_some() || crate::screen_keys::code_for(&id, &self.keyed, &sent, |p| p.starts_with(crate::jobs::ON_KEY) || lists.iter().any(|x| x.id == p)).is_some();
            // (a device's job tagged with its sort, so a search for "mouse" finds the mouse's)
            let tag = match j {
                crate::jobs::Job::Macro { .. } => "Macro",
                crate::jobs::Job::Scene(_) => "Scene",
                _ => names.sort_of(&j),
            };
            let a = Entry { label: words, act: code.map_or(Action::Raw(Vec::new()), keyboard::screen_slot), tag };
            let row = if can { (a, Choice::Job(j.id()), 0, short) } else { (a, Choice::Nothing, 5, short) };
            match &j {
                crate::jobs::Job::Macro { .. } => macros.push(row),
                crate::jobs::Job::Scene(_) => scenes.push(row),
                _ => {
                    let (key, head) = names.group(&j).unwrap_or_default();
                    match groups.iter_mut().find(|(k, ..)| *k == key) {
                        Some((_, _, list)) => list.push(row),
                        None => groups.push((key, head, vec![row])),
                    }
                }
            }
        }
        // (each device's by its key, two of a name kept apart; under its name)
        let profiles = groups.into_iter().map(|(_, head, list)| (head, list)).collect();
        (profiles, macros, scenes)
    }

    /// The jobs that each play one playlist, as the action menu lists them: each one's row
    /// (its definition, for its tick, while the playlist has a code), what choosing it does,
    /// and its row's kind: 5, a job no key can be given now, while every code is some key's.
    pub(super) fn play_jobs(&self) -> Vec<(Entry, Choice, i32)> {
        let sent = self.page.sent_codes();
        let lists = &self.screens.playlists;
        lists
            .iter()
            .map(|l| {
                let code = self.keyed.iter().find(|(_, id)| **id == l.id).map(|(c, _)| *c);
                let can = code.is_some() || crate::screen_keys::code_for(&l.id, &self.keyed, &sent, |p| lists.iter().any(|x| x.id == p)).is_some();
                // (a playlist no key plays yet has no job to tick: an empty code matches no key's)
                let a = Entry { label: format!("Play {}", l.name), act: code.map_or(Action::Raw(Vec::new()), keyboard::screen_slot), tag: "Screen" };
                if can { (a, Choice::Play(l.id.clone()), 0) } else { (a, Choice::Nothing, 5) }
            })
            .collect()
    }

    /// A key with a screen job pressed (its code, see keyboard::screen_job), as the keyboard's own
    /// reports tell of it: Record takes it as the key's job; else, while Ectomatter plays the
    /// screen, the job is done for the profile whose playlist plays. Nothing while a layout is
    /// open in the Screen Editor, the Onboard layout tab has the screen, or another program's
    /// screen shows (as a game's launch).
    /// A key with one of the keyboard's host slots pressed (`down`) or let
    /// go: one given one of Ectomatter's own jobs does it (a macro played as
    /// long as it says); a screen job is done as its key goes down.
    pub fn screen_key_event(&mut self, code: u32, down: bool) {
        let job = self.keyed.get(&code).and_then(|id| id.strip_prefix(crate::jobs::ON_KEY)).and_then(crate::jobs::Job::of);
        match job {
            // (Record takes the key as its job, as it takes a screen job's)
            Some(_) if self.listening.is_some() => {
                if down {
                    self.screen_key(code);
                }
            }
            Some(j) => self.play_for(&format!("keyboard/{code}"), Some(j.write()), down),
            None if down => self.screen_key(code),
            None => {}
        }
    }

    pub fn screen_key(&mut self, code: u32) {
        if let Some((layer, _)) = self.listening {
            self.assign(layer, keyboard::screen_slot(code));
            return;
        }
        if !self.controlled || self.editor.is_some() || self.aside || self.lcd.sequence.is_some() {
            return;
        }
        if code == keyboard::NEXT_LAYOUT || code == keyboard::PREV_LAYOUT {
            if let Some(pl) = &self.player {
                pl.tell(crate::lcd_player::Control::Step(if code == keyboard::NEXT_LAYOUT { 1 } else { -1 }));
            }
            return;
        }
        let n = self.page.editing;
        let count = self.screens.playlists.len();
        let mine = self.screens.of_profile(n);
        let to = match code {
            keyboard::NEXT_PLAYLIST => (count > 1).then(|| (mine + 1) % count),
            keyboard::PREV_PLAYLIST => (count > 1).then(|| (mine + count - 1) % count),
            c => self.keyed.get(&c).and_then(|id| self.screens.playlists.iter().position(|l| &l.id == id)).filter(|k| *k != mine),
        };
        if let Some(k) = to {
            let id = self.screens.playlists[k].id.clone();
            self.screens.choose(n, &id);
            self.pages_edited();
            if let Some(pl) = &self.player {
                pl.tell(crate::lcd_player::Control::Begin);
            }
            self.lcd.page = 0;
            // (the playhead as the new playlist has it: none with one layout)
            self.set_playhead();
            self.refresh();
        }
    }

    /// What a key with screen job `code` does, as the note under its job says it, naming the
    /// playlist its profile plays.
    pub(super) fn screen_note(&self, code: u32) -> String {
        // (one of Ectomatter's own jobs)
        if let Some(id) = self.keyed.get(&code).and_then(|id| id.strip_prefix(crate::jobs::ON_KEY)) {
            return self.job_note(id);
        }
        let pl = &self.screens.playlist(self.page.editing).name;
        match code {
            keyboard::NEXT_LAYOUT => format!("Shows the next layout in {pl} while Ectomatter is running."),
            keyboard::PREV_LAYOUT => format!("Shows the previous layout in {pl} while Ectomatter is running."),
            keyboard::NEXT_PLAYLIST => format!("Plays the playlist after {pl} while Ectomatter is running."),
            keyboard::PREV_PLAYLIST => format!("Plays the playlist before {pl} while Ectomatter is running."),
            c => match self.page.playlist_keys.get(&c) {
                Some(name) => format!("Plays {name} while Ectomatter is running."),
                None => "The playlist this key played was deleted, so the key does nothing until you give it another job.".into(),
            },
        }
    }

    pub(super) fn build_menu(&mut self) {
        let Some(m) = &self.menu else { return };
        let def = self.page.def(LAYERS[m.layer].0, m.pos);
        let g = self.page.groups();
        let lists: [&Vec<Entry>; 7] = [&g.keys, &g.media, &g.mouse, &g.profiles, &g.layouts, &g.playlists, &g.board];
        let mut rows: Vec<MenuRow> = Vec::new();
        let mut choices: Vec<Choice> = Vec::new();
        let row = |kind: i32, label: &str, tag: &str, tick: bool| MenuRow { kind, label: label.into(), tag: tag.into(), tick, sub: "".into(), ..Default::default() };
        let q = m.query.trim().to_lowercase();
        // each job as a row: what choosing it does, and the row's kind (0, or 5 for one that
        // can't be given now)
        let set = |list: &[Entry]| list.iter().map(|e| (e.clone(), Choice::Set(e.act.clone()), 0)).collect::<Vec<_>>();
        let plays = self.play_jobs();
        let (by_device, own_macros, scene_jobs) = self.device_jobs();
        // (in whole words, or by their own names under their heading)
        let whole = |v: &[(Entry, Choice, i32, String)]| v.iter().map(|(e, c, k, _)| (e.clone(), c.clone(), *k)).collect::<Vec<_>>();
        let headed = |v: &[(Entry, Choice, i32, String)]| v.iter().map(|(e, c, k, s)| (Entry { label: s.clone(), ..e.clone() }, c.clone(), *k)).collect::<Vec<_>>();
        let (by_device_h, scene_jobs_h): (Vec<(String, Vec<(Entry, Choice, i32)>)>, _) = (by_device.iter().map(|(h, v)| (h.clone(), headed(v))).collect(), headed(&scene_jobs));
        let others: Vec<(Entry, Choice, i32)> = by_device.iter().flat_map(|(_, v)| whole(v)).collect();
        let (own_macros, scene_jobs) = (whole(&own_macros), whole(&scene_jobs));
        // (the jobs Ectomatter does for a key share the keyboard's seven spare codes with
        // the playlists: once keys use all seven, a page with a row that can't be given says why)
        let full = "The keyboard has room for seven of Ectomatter’s jobs, playlists included, and keys on your profiles fill all seven. To make room, give the keys that have one of those jobs something else to do.";
        let job_key = self.is_job_key(&def);
        let def_tag = self.key_tag(&def);
        // a searchable list, as pickList draws it
        let pick = |rows: &mut Vec<MenuRow>, choices: &mut Vec<Choice>, groups: &[(&str, Vec<(Entry, Choice, i32)>)]| {
            let mut n = 0;
            for (head, items) in groups {
                // (a group's heading found finds all of it: "steelseries" lists the mouse's,
                // and "mouse" finds its rows by their tag)
                let whole_group = !q.is_empty() && !head.is_empty() && head.to_lowercase().contains(&q);
                let hit: Vec<&(Entry, Choice, i32)> = items.iter().filter(|(a, ..)| q.is_empty() || whole_group || a.label.to_lowercase().contains(&q) || a.tag.to_lowercase().contains(&q)).collect();
                if hit.is_empty() {
                    continue;
                }
                if !head.is_empty() {
                    rows.push(row(1, head, "", false));
                    choices.push(Choice::Nothing);
                }
                for (a, c, kind) in hit.into_iter().take(if q.is_empty() { 400 } else { 60 }) {
                    n += 1;
                    rows.push(row(*kind, &a.label, if q.is_empty() { "" } else { a.tag }, a.act == def));
                    choices.push(c.clone());
                }
            }
            if n == 0 {
                rows.push(row(3, "Nothing matches.", "", false));
                choices.push(Choice::Nothing);
            }
        };
        let placeholder;
        match m.page {
            Some(MACRO_PAGE) => {
                placeholder = "Search macros".to_string();
                if g.macros.is_empty() && own_macros.is_empty() {
                    rows.push(row(3, "No macros yet. Make one in the Macro Editor.", "", false));
                    choices.push(Choice::Nothing);
                } else {
                    // (the keyboard's own, then those Ectomatter plays)
                    pick(&mut rows, &mut choices, &[("Macros", set(&g.macros)), ("Ectomatter's macros", own_macros.clone())]);
                    if q.is_empty() && own_macros.iter().any(|(.., kind)| *kind == 5) {
                        rows.push(row(3, full, "", false));
                        choices.push(Choice::Nothing);
                    }
                }
            }
            Some(p) => {
                let title = PAGES[p];
                let what = title.trim_start_matches("A ").trim_start_matches("An ").to_lowercase();
                // (the profiles' page holds the scenes too, and says so)
                placeholder = if p == PROFILE_PAGE { "Search profiles and scenes".to_string() } else { format!("Search {what}s") };
                // (the playlists' own jobs after stepping through them; other devices'
                // profiles after the keyboard's, under the devices' names)
                let mut items = set(lists[p]);
                if p == PLAYLIST_PAGE {
                    items.extend(plays.iter().cloned());
                }
                if p == PROFILE_PAGE {
                    // (each device's profiles and the scenes under their headings, each by its own
                    // name, as the Macro Editor lists them; a search across every page has them whole)
                    let own: Vec<(Entry, Choice, i32)> = items
                        .iter()
                        .map(|(e, c, k)| {
                            let label = match &e.act {
                                Action::Profile(ectomatter_protocol::Jump::To(n)) => self.page.profile_name(*n as u8),
                                _ => e.label.clone(),
                            };
                            (Entry { label, ..e.clone() }, c.clone(), *k)
                        })
                        .collect();
                    // (this keyboard's own, then each other device's, then the scenes, each under its name)
                    let mine = crate::cores::keyboard().map_or("", |c| c.name()).to_string();
                    let mut groups: Vec<(&str, Vec<(Entry, Choice, i32)>)> = vec![(mine.as_str(), own)];
                    groups.extend(by_device_h.iter().map(|(h, v)| (h.as_str(), v.clone())));
                    groups.push(("Scenes", scene_jobs_h.clone()));
                    pick(&mut rows, &mut choices, &groups);
                } else {
                    pick(&mut rows, &mut choices, &[(title, items)]);
                }
                let greyed = match p {
                    PLAYLIST_PAGE => plays.iter().any(|(.., kind)| *kind == 5),
                    PROFILE_PAGE => others.iter().chain(&scene_jobs).any(|(.., kind)| *kind == 5),
                    _ => false,
                };
                if q.is_empty() && greyed {
                    rows.push(row(3, full, "", false));
                    choices.push(Choice::Nothing);
                }
            }
            None => {
                placeholder = "Search keys, media, macros".to_string();
                if !q.is_empty() {
                    let mut all = set(&g.macros.iter().chain(&g.keys).chain(&g.media).chain(&g.mouse).chain(&g.profiles).chain(&g.layouts).chain(&g.playlists).chain(&g.board).cloned().collect::<Vec<_>>());
                    all.extend(plays.iter().cloned());
                    all.extend(others.iter().cloned());
                    all.extend(scene_jobs.iter().cloned());
                    all.extend(own_macros.iter().cloned());
                    pick(&mut rows, &mut choices, &[("", all)]);
                } else {
                    if !g.macros.is_empty() {
                        rows.push(row(1, "Macros", "", false));
                        choices.push(Choice::Nothing);
                        for a in &g.macros {
                            let same = matches!((&def, &a.act), (Action::Macro { n: x, .. }, Action::Macro { n: y, .. }) if x == y);
                            rows.push(row(0, &a.label, "", same));
                            choices.push(Choice::Set(a.act.clone()));
                        }
                        rows.push(row(2, "", "", false));
                        choices.push(Choice::Nothing);
                    }
                    // (Ectomatter's own macros after the keyboard's)
                    if !own_macros.is_empty() {
                        rows.push(row(1, "Ectomatter's macros", "", false));
                        choices.push(Choice::Nothing);
                        for (a, c, kind) in &own_macros {
                            rows.push(row(*kind, &a.label, "", a.act == def));
                            choices.push(c.clone());
                        }
                        rows.push(row(2, "", "", false));
                        choices.push(Choice::Nothing);
                    }
                    rows.push(row(1, "Other", "", false));
                    choices.push(Choice::Nothing);
                    let kind = def_tag;
                    for (p, title) in PAGES.iter().enumerate() {
                        // (the screen's two pages each ticked for their own jobs; another
                        // device's job's code is no screen job)
                        let tick = if p == LAYOUT_PAGE || p == PLAYLIST_PAGE {
                            !job_key && keyboard::screen_job(&def).is_some_and(|c| screen_page(c) == p)
                        } else if p == PROFILE_PAGE && job_key {
                            def_tag == "Profile"
                        } else {
                            lists[p].first().is_some_and(|a| a.tag == kind) && !matches!(def, Action::Macro { .. })
                        };
                        let title = if p == PROFILE_PAGE { "Switch profile or scene" } else { title };
                        rows.push(row(0, &format!("{title} ›"), "", tick));
                        choices.push(Choice::Page(p));
                    }
                    rows.push(row(0, "Nothing", "", keyboard::is_nothing(&def)));
                    choices.push(Choice::Set(Action::None));
                }
            }
        }
        let (ax, ay, ah) = m.anchor;
        let back = m.page.is_some();
        self.ui.set_menu_ax(ax);
        self.ui.set_menu_ay(ay);
        self.ui.set_menu_ah(ah);
        self.ui.set_menu_back(back);
        self.ui.set_menu_placeholder(placeholder.into());
        // its height as the page lays its rows out: Slint lays a list's rows out
        // only as it draws them, so the window works it out
        // (its notes wrapped to the width it opened at)
        crate::menu_size::fit_notes(&mut rows, self.ui.get_menu_w());
        self.ui.set_menu_natural(crate::menu_size::height(&rows, true, back, f32::MAX));
        self.ui.set_menu_rows(ModelRc::new(VecModel::from(rows)));
        if let Some(m) = &mut self.menu {
            m.choices = choices;
        }
    }
}
