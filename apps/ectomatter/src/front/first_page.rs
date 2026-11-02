//! Each place at its first page. A press on one of the rail's items opens its
//! place there, as a tab bar's item does (iOS's: a tab pressed again goes
//! back to its first screen): its first tab, nothing on it chosen, nothing
//! typed in its find box, and no menu, game's page or form open. Pressed on the place already showing it,
//! where the place is at its first page already, the press opens or closes
//! the rail. The window opened afresh (from the tray, or Ectomatter opened
//! again) starts at Home's first page, every other place at its own.
//! tests/renders.rs, a_main_menu_item_opens_its_place_at_its_first_page,
//! holds each place to its first page's approved picture.

use super::*;

impl Front {
    /// The places the rail opens: Home, each device's page, the Library and Settings.
    fn places(&self) -> Vec<String> {
        let mut out = vec!["home".to_string()];
        out.extend(self.devices_in_order().into_iter().map(|d| d.0.to_string()));
        out.extend(["library".to_string(), "settings".to_string()]);
        out
    }

    /// Whether `place` stands at its first page.
    pub(super) fn at_first_page(&self, place: &str) -> bool {
        match place {
            "home" => !self.desk.editing && self.desk.pad_chosen.is_none(),
            "library" => self.games.open.is_none() && self.games.tab == 0 && self.games.query.is_empty(),
            "settings" => self.settings_tab == crate::ecto_settings::Tab::default(),
            "keyboard" => self.page.view == View::Keys && self.page.sel.is_none() && self.layer == 0 && self.slider_sel == 0 && self.lib_query.is_empty(),
            "mouse" => self.mouse.tab == 0 && self.mouse.button.is_none() && self.mouse.menu.is_none() && self.mouse_lib_query.is_empty(),
            _ => true,
        }
    }

    /// `place` put at its first page, whatever was open on it.
    pub(super) fn to_first_page(&mut self, place: &str) {
        self.close_menu();
        self.close_picker();
        // (no find box typed in)
        self.ui.set_find_stop(self.ui.get_find_stop() + 1);
        match place {
            "home" => {
                if self.desk.editing {
                    self.desk.set_editing(false);
                    self.desk_frames_run();
                }
                self.desk.pad_chosen = None;
                self.show_desk();
            }
            "library" => {
                if self.games.open.is_some() {
                    self.games.back();
                }
                self.games.tab_chosen(0, &self.ui);
                // (nothing typed in its find box)
                self.games.find("");
                self.ui.set_gl_query("".into());
            }
            "settings" => {
                self.close_credits();
                self.open_settings_tab(crate::ecto_settings::Tab::default());
            }
            "keyboard" => {
                if self.page.view != View::Keys {
                    self.page.view = View::Keys;
                    self.stop_preview();
                }
                self.page.sel = None;
                self.listening = None;
                self.layer = 0;
                self.page.fn_layer = false;
                self.slider_sel = 0;
                // (the key list back at its top, its groups shut, nothing typed in its find box)
                self.lib_query.clear();
                self.ui.set_lib_query("".into());
                self.lib_shut = lib_first_shut();
                self.lib_revealed = None;
                self.ui.set_lib_reveal_y(0.0);
                self.ui.set_lib_reveal_n(self.ui.get_lib_reveal_n() + 1);
            }
            "mouse" => {
                self.mouse.tab = 0;
                self.mouse.button = None;
                self.mouse.menu = None;
                self.mouse_lib_query.clear();
                self.ui.set_mouse_lib_query("".into());
                self.mouse_lib_shut = None;
                self.mouse_lib_revealed = None;
                self.ui.set_mouse_lib_reveal_y(0.0);
                self.ui.set_mouse_lib_reveal_n(self.ui.get_mouse_lib_reveal_n() + 1);
            }
            _ => {}
        }
    }

    /// `place` opened at its first page, as the tray's menu opens a device's.
    pub fn open_at_first_page(&mut self, place: &str) {
        self.to_first_page(place);
        self.ui.set_place(place.into());
        self.refresh();
    }

    /// The window opened afresh: every place at its first page, and Home shown.
    pub fn open_fresh(&mut self) {
        eprintln!("ectomatter: the window opens at Home, each place at its first page");
        for place in self.places() {
            self.to_first_page(&place);
        }
        self.ui.set_place("home".into());
        self.refresh();
    }
}
