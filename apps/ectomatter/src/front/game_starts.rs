//! Games started through Ectomatter: what the plugin that says how games start is told of a game, and its page's sections.

use super::*;

impl Front {
    /// What the plugin that says how games start (the Game Configurator) is
    /// told of a page: what's known of this computer, and for a Steam game its
    /// own (its Steam number, whether it runs without Proton, whether it
    /// starts through Ectomatter, its prefix and its backups), for a game
    /// added by hand that it's one; for Every game's page, how many games
    /// start through Ectomatter. Null until what's known is read.
    pub(super) fn launch_facts(&self, id: &str) -> serde_json::Value {
        let Some(k) = &self.known else { return serde_json::Value::Null };
        let mut f = k.facts();
        let games: Vec<&crate::steam::App> = self.games.library.steam.as_ref().map(|s| s.apps.iter().filter(|a| a.kind == crate::steam::Kind::Game).collect()).unwrap_or_default();
        if id == crate::games::EVERY {
            f["through"] = serde_json::json!([games.iter().filter(|a| k.through(a.id)).count(), games.len()]);
        } else if let Some(app) = crate::games::steam_number(id) {
            let a = games.iter().find(|a| a.id == app);
            let lib = a.map_or_else(|| k.root.clone(), |a| a.library.clone());
            let (bytes, made, backups) = k.prefixes.get(&app).copied().unwrap_or((0, false, 0));
            f["game"] = serde_json::json!({
                "id": id, "name": self.games.name(id, &self.front_apps), "app": app, "native": a.is_some_and(|a| k.native(app, a.linux)), "through": k.through(app),
                "prefix": { "path": crate::launcher::prefix_of(&lib, app), "bytes": bytes, "made": made }, "backups": backups,
            });
            // (until it starts through Ectomatter, the build Steam starts it with; a Linux
            // version, the runtime Steam starts it in)
            if !k.through(app) {
                f["game"]["steam"] = serde_json::json!(k.steam_build(app));
            }
            if a.is_some_and(|a| k.native(app, a.linux)) {
                f["game"]["runtime"] = serde_json::json!(k.runtime_of(app, a.and_then(|a| a.runtime.as_deref())));
            }
        } else {
            f["game"] = serde_json::json!({ "id": id, "name": self.games.name(id, &self.front_apps), "hand": true });
        }
        f
    }

    /// A game's recipe for the launcher, as the plugin that says how games
    /// start answers for it (Every game's, for games without one of their own).
    pub(super) fn recipe_of(&self, id: &str) -> Option<serde_json::Value> {
        let facts = self.launch_facts(id);
        if facts.is_null() {
            return None;
        }
        let every = id == crate::games::EVERY;
        let name = if every { String::new() } else { self.games.name(id, &self.front_apps) };
        let lib = &self.games.library;
        let values = |plugin: &str| lib.wanted_values(id, plugin);
        let every_values = |plugin: &str| lib.plugin_values(crate::games::EVERY, plugin);
        let game = serde_json::json!({ "id": id, "name": name, "app": crate::games::steam_number(id).unwrap_or(0) });
        match self.hosted.launch(&game, &values, &every_values, &facts)? {
            Ok(r) => {
                // (what the person typed in the plugin's settings: their own variables)
                let own: Vec<String> = self.hosted.launcher().map(|p| [values(p), every_values(p)].iter().flat_map(|v| v.as_object().into_iter().flatten().filter_map(|(_, x)| x.as_object()).flat_map(|o| o.keys().cloned()).collect::<Vec<_>>()).collect()).unwrap_or_default();
                let starts = self.hosted.launch_grant().cloned().unwrap_or_default();
                let (r, left_out) = crate::launcher::checked_recipe(&r, &facts, &starts, &own);
                if !left_out.is_empty() {
                    eprintln!("ectomatter: how {} starts, left out as the plugin’s manifest doesn’t name them, or they run code: {}", if every { "every game" } else { name.as_str() }, left_out.join(", "));
                }
                Some(r)
            }
            Err(e) => {
                eprintln!("ectomatter: how {} starts: {e}", if every { "every game" } else { name.as_str() });
                None
            }
        }
    }

    /// Each Steam game's recipe written for the launcher, and Every game's
    /// (launch/default.json), when the plugin's choices or what's known have
    /// changed since they were last written. None offscreen: a snapshot
    /// writes nothing.
    pub(super) fn write_recipes(&mut self) {
        if crate::desk::offscreen() {
            return;
        }
        let (Some(plugin), Some(k)) = (self.hosted.launcher().map(String::from), self.known.clone()) else { return };
        let stamp = (self.games.library.plugin_all(&plugin), k.clone());
        if self.recipes_for.as_ref() == Some(&stamp) {
            return;
        }
        let ids: Vec<String> = self.games.library.steam.as_ref().map(|s| s.apps.iter().filter(|a| a.kind == crate::steam::Kind::Game).map(|a| crate::games::steam_id(a.id)).collect()).unwrap_or_default();
        for id in std::iter::once(crate::games::EVERY.to_string()).chain(ids) {
            let Some(r) = self.recipe_of(&id) else { continue };
            let file = crate::games::steam_number(&id).map_or_else(|| "default".to_string(), |n| n.to_string());
            if let Err(e) = crate::launcher::write_recipe(&file, &r) {
                eprintln!("ectomatter: the recipe {file}.json wasn’t written: {e}");
            }
        }
        self.recipes_for = Some(stamp);
    }

    /// What's known read again (on its thread; offscreen, now), with the
    /// prefix of the game whose page is open.
    pub(super) fn ask_known(&mut self) {
        let Some(root) = self.games.library.steam.as_ref().map(|s| s.root.clone()) else { return };
        self.known_asked = Some(Instant::now());
        let prefixes: Vec<(u32, std::path::PathBuf)> = self
            .games
            .open
            .as_deref()
            .and_then(crate::games::steam_number)
            .and_then(|app| self.games.library.steam.as_ref()?.apps.iter().find(|a| a.id == app).map(|a| (app, a.library.clone())))
            .into_iter()
            .collect();
        if crate::desk::offscreen() {
            self.known = Some(crate::launcher::know(&root, &prefixes));
            return;
        }
        self.facts_reader.get_or_insert_with(crate::launcher::Reader::start).ask(root, prefixes);
    }

    /// A game starting through Ectomatter's launcher, which waits: its
    /// changes made now, before the Library's watch would see the game, by
    /// Steam's reaper for it; the launcher answered once they're made.
    pub fn game_starting(&mut self, s: crate::launcher::Start) {
        // (a shortcut to a program outside Steam has no Steam number to know it by:
        // the Library's watch makes its changes, by its program)
        if s.id == crate::games::steam_id(0) {
            let _ = s.done.send(());
            return;
        }
        match self.starting.get(&s.id).copied() {
            // (seen by the watch first: its changes are made, or being made)
            Some((_, true)) => {
                let id = s.id.clone();
                self.let_start(&id, s.done);
                return;
            }
            Some((seq, false)) => {
                self.holds.push((s.id, seq, s.done, Instant::now()));
                return;
            }
            None => {}
        }
        let reaper = crate::launcher::reaper_above(s.pid).unwrap_or(s.pid);
        // (by the Library's name for it; a game it doesn't hold yet, by its recipe's)
        let name = match self.games.name(&s.id, &self.front_apps) {
            n if n == s.id && !s.name.is_empty() => s.name.clone(),
            n => n,
        };
        eprintln!("ectomatter: {name} starting through Ectomatter");
        let r = crate::games::Running { id: s.id.clone(), pid: reaper, since: crate::games::now() };
        let plan = self.game_plan(&r, &name);
        let seq = plan.seq;
        self.worker.start_game(plan);
        self.starting.insert(s.id.clone(), (seq, false));
        self.early.push((s.id.clone(), reaper, Instant::now()));
        self.holds.push((s.id, seq, s.done, Instant::now()));
    }

    /// A launcher waiting on a game answered: the game's recipe written
    /// afresh first, with the main monitor as the game's changes left it (in
    /// HDR, say, for gamescope to show the game in HDR).
    pub(super) fn let_start(&mut self, id: &str, done: mpsc::Sender<()>) {
        if !crate::desk::offscreen() {
            if let Some(k) = &mut self.known {
                k.main = crate::launcher::main_monitor();
            }
            if let (Some(r), Some(n)) = (self.recipe_of(id), crate::games::steam_number(id)) {
                if let Err(e) = crate::launcher::write_recipe(&n.to_string(), &r) {
                    eprintln!("ectomatter: the recipe {n}.json wasn’t written: {e}");
                }
            }
        }
        let _ = done.send(());
    }

    /// A game's start changes made, as the put-back thread says: the
    /// launchers waiting on it answered.
    pub(super) fn started_through(&mut self, id: &str, seq: u64) {
        // (an older plan's word for the same game answers nothing of a newer start)
        match self.starting.get_mut(id) {
            Some(s) if s.0 == seq => s.1 = true,
            _ => return,
        }
        let (waiting, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.holds).into_iter().partition(|(g, n, _, _)| g == id && *n == seq);
        self.holds = rest;
        for (_, _, done, _) in waiting {
            self.let_start(id, done);
        }
    }

    /// Each turn: what's known read again when it's due (each minute; each
    /// few seconds while a page of the Library shows), the launcher installed
    /// in Steam once, the recipes kept up to date, the waits past their time
    /// let go, a start made early whose game never showed put back, a program
    /// chosen to run in a prefix started, and the jobs finished said. True if
    /// the Library has news.
    pub(super) fn launch_tick(&mut self) -> bool {
        // (the launchers waiting, and starts made early, looked after first: the tool stays
        // in Steam's settings while the plugin that says how games start is off)
        self.holds.retain(|(_, _, _, t)| t.elapsed() < Duration::from_secs(31));
        let gone: Vec<String> = self.early.iter().filter(|(_, pid, t)| t.elapsed() > Duration::from_secs(10) && !std::path::Path::new(&format!("/proc/{pid}")).exists()).map(|(id, _, _)| id.clone()).collect();
        for id in gone {
            self.early.retain(|(g, _, _)| *g != id);
            if !self.games.running.iter().any(|r| r.id == id) {
                eprintln!("ectomatter: {} ended before it was seen running", self.games.name(&id, &self.front_apps));
                self.worker.quit_game(&id);
                self.starting.remove(&id);
            }
        }
        if self.hosted.launcher().is_none() {
            return false;
        }
        let mut news = false;
        let showing = self.ui.get_place() == "library" && self.games.open.is_some();
        if self.known_asked.is_none_or(|t| t.elapsed() > Duration::from_secs(if showing { 5 } else { 60 })) {
            self.ask_known();
        }
        if let Some(r) = &self.facts_reader {
            while let Ok(k) = r.known.try_recv() {
                if self.known.as_ref() != Some(&k) {
                    self.known = Some(k);
                    news = true;
                }
            }
        }
        if !self.tool_installed && !crate::desk::offscreen() {
            if let Some(root) = self.games.library.steam.as_ref().map(|s| s.root.clone()) {
                self.tool_installed = true;
                let _ = std::thread::Builder::new().name("launcher-install".into()).spawn(move || match crate::launcher::install(&root) {
                    Ok(true) => eprintln!("ectomatter: the launcher is installed in Steam (Steam lists it under Compatibility once it starts again)"),
                    Ok(false) => {}
                    Err(e) => eprintln!("ectomatter: the launcher wasn’t installed in Steam: {e}"),
                });
            }
        }
        self.write_recipes();
        if let Some((id, rx)) = &self.run_pick {
            if let Ok(chosen) = rx.try_recv() {
                let id = id.clone();
                self.run_pick = None;
                if let Some(p) = chosen {
                    self.in_prefix(&id, true, vec![p.to_string_lossy().into_owned()]);
                }
            }
        }
        let mut said = Vec::new();
        self.jobs.retain(|rx| match rx.try_recv() {
            Ok(s) => {
                said.push(s);
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => false,
        });
        for (words, bad) in said {
            self.games.say(words, bad);
            self.ask_known();
            news = true;
        }
        news
    }

    /// The Proton build a game starts with: the one its recipe names (the
    /// plugin that says how games start answers it), else Steam's default,
    /// as installed.
    pub(super) fn build_for(&self, id: &str) -> Option<crate::launcher::Build> {
        let k = self.known.as_ref()?;
        let want = self.recipe_of(id).and_then(|r| r["proton"].as_str().map(std::path::PathBuf::from));
        k.builds.iter().find(|b| want.as_ref() == Some(&b.path)).or_else(|| k.builds.iter().find(|b| b.id == k.default)).or_else(|| k.builds.first()).cloned()
    }

    /// A Steam game's number and library, for its prefix.
    pub(super) fn prefix_place(&self, id: &str) -> Option<(u32, std::path::PathBuf)> {
        let app = crate::games::steam_number(id)?;
        let lib = self.games.library.steam.as_ref()?.apps.iter().find(|a| a.id == app)?.library.clone();
        Some((app, lib))
    }

    /// A program run for a game's prefix with the game's Proton build:
    /// inside it (a Windows program), or a program of this computer's given
    /// the prefix and the build's Wine (launcher::in_prefix).
    pub(super) fn in_prefix(&mut self, id: &str, inside: bool, what: Vec<String>) {
        let name = self.games.name(id, &self.front_apps);
        let (Some((app, lib)), Some(b), Some(root)) = (self.prefix_place(id), self.build_for(id), self.known.as_ref().map(|k| k.root.clone())) else {
            self.games.say(format!("Ectomatter can’t find {name}’s prefix or a Proton build for it."), true);
            return;
        };
        if let Some(p) = what.first().filter(|p| !inside && !crate::launcher::on_path(p)) {
            self.games.say(format!("{p} isn’t installed. Install it from your system’s software center, then try again."), true);
            return;
        }
        match crate::launcher::in_prefix(&root, &lib, app, &b.path, inside, &what).spawn() {
            Ok(_) => self.games.say(format!("Opening it in {name}’s prefix, with {}.", b.name), false),
            Err(e) => self.games.say(format!("It didn’t open: {e}"), true),
        }
    }

    /// A button in a plugin's section on the open page, which does what it
    /// says it does (`does`, docs/core-protocol.md, section 11): one of the
    /// actions Ectomatter makes, on the page's game. A game's start and its
    /// prefix are only a plugin's that uses `launch`, and a program run in
    /// the prefix only one its manifest names among its `tools` (or one the
    /// person picks). A disabled button, or one that says nothing Ectomatter
    /// makes, does nothing.
    pub(super) fn section_action(&mut self, plugin: &str, action: &serde_json::Value) {
        use crate::launcher::Does;
        let Some(id) = self.games.open.clone() else { return };
        let Some(does) = self.hosted.uses_of(plugin).and_then(|u| Does::of(action, u)) else { return };
        let name = self.games.name(&id, &self.front_apps);
        let running = self.games.running.iter().any(|r| r.id == id);
        match does {
            Does::StartThrough => {
                if let Some(app) = crate::games::steam_number(&id) {
                    self.start_through(vec![app]);
                }
            }
            Does::StartEveryThrough => {
                let Some(k) = &self.known else { return };
                let apps: Vec<u32> = self.games.library.steam.as_ref().map(|s| s.apps.iter().filter(|a| a.kind == crate::steam::Kind::Game && !k.through(a.id)).map(|a| a.id).collect()).unwrap_or_default();
                self.start_through(apps);
            }
            Does::OpenPrefix => {
                if let Some((app, lib)) = self.prefix_place(&id) {
                    crate::picker::show_folder(&crate::launcher::prefix_of(&lib, app));
                }
            }
            Does::BackUpPrefix | Does::ResetPrefix => {
                if running {
                    self.games.say(format!("Quit {name} first: its prefix is in use."), true);
                    return;
                }
                let Some((app, lib)) = self.prefix_place(&id) else { return };
                let reset = does == Does::ResetPrefix;
                let (tx, rx) = mpsc::channel();
                self.games.say(if reset { format!("Backing up {name}’s prefix, then resetting it…") } else { format!("Backing up {name}’s prefix…") }, false);
                let _ = std::thread::Builder::new().name("prefix".into()).spawn(move || {
                    let r = if reset { crate::launcher::reset(&lib, app) } else { crate::launcher::back_up(&lib, app) };
                    let _ = tx.send(match r {
                        Ok(p) if reset => (format!("Reset {name}’s prefix. Its backup is in {}.", crate::launcher::shown(&p)), false),
                        Ok(p) => (format!("Backed up {name}’s prefix to {}.", crate::launcher::shown(&p)), false),
                        Err(e) => (e, true),
                    });
                });
                self.jobs.push(rx);
            }
            Does::RunInPrefix(None) => {
                if self.run_pick.is_none() {
                    self.run_pick = Some((id.clone(), crate::picker::open_file(&format!("Choose a program to run in {name}’s prefix"), "Windows programs", &["*.exe", "*.msi", "*.bat"])));
                }
            }
            Does::RunInPrefix(Some(p)) => self.in_prefix(&id, true, vec![p]),
            Does::RunWithPrefix(p) => self.in_prefix(&id, false, vec![p]),
        }
    }

    /// Games set to start through Ectomatter in Steam (each of `apps`, by
    /// its Steam number): what the plugin that says how games start keeps for
    /// each from how Steam started it, first (keep_through), then Steam
    /// closed, its settings changed, and Steam opened again if it was open.
    /// Refused while a Steam game runs, which Steam closing would end.
    pub(super) fn start_through(&mut self, apps: Vec<u32>) {
        if apps.is_empty() || self.steam_busy.as_ref().is_some_and(|b| b.load(std::sync::atomic::Ordering::Relaxed)) {
            return;
        }
        if let Some(r) = self.games.running.iter().find(|r| crate::games::steam_number(&r.id).is_some()) {
            let n = self.games.name(&r.id, &self.front_apps);
            self.games.say(format!("Quit {n} first: Steam has to close for this change."), true);
            return;
        }
        let Some(k) = self.known.clone() else { return };
        if self.hosted.launcher().is_none() {
            return;
        }
        let linux: Vec<u32> = self.games.library.steam.as_ref().map(|s| s.apps.iter().filter(|a| k.native(a.id, a.linux)).map(|a| a.id).collect()).unwrap_or_default();
        for app in &apps {
            self.keep_through(*app);
        }
        let switches: Vec<crate::launcher::Switch> = apps.iter().map(|a| crate::launcher::Switch { app: a.to_string(), through: if linux.contains(a) { crate::launcher::TOOL_LINUX } else { crate::launcher::TOOL } }).collect();
        let n = apps.len();
        let busy = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        self.steam_busy = Some(busy.clone());
        let (tx, rx) = mpsc::channel();
        let backups = crate::data_dir().join("steam-backups");
        let root = k.root.clone();
        let one = (n == 1).then(|| self.games.name(&crate::games::steam_id(apps[0]), &self.front_apps));
        self.games.say("Closing Steam to change its settings…", false);
        let _ = std::thread::Builder::new().name("steam-settings".into()).spawn(move || {
            // (looked at again as Steam is asked to close, which would end a game running)
            if !crate::steam::running().is_empty() {
                let _ = tx.send(("Quit the game first: Steam has to close for this change.".to_string(), true));
                busy.store(false, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            let was_open = crate::launcher::steam_running();
            // (the launcher in place first, for Steam to list it as it opens again)
            let r = crate::launcher::install(&root).and_then(|_| crate::launcher::close_steam()).and_then(|_| crate::launcher::change_steam(&root, &switches, &backups));
            let reopened = !was_open || crate::launcher::open_steam();
            let _ = tx.send(match r {
                Ok(_) => {
                    let done = match one {
                        Some(g) => format!("{g} starts through Ectomatter now."),
                        None => format!("{n} games start through Ectomatter now."),
                    };
                    if reopened { (done, false) } else { (format!("{done} Steam didn’t open again: open it from your apps."), true) }
                }
                Err(e) => (format!("Steam’s settings weren’t changed: {e}"), true),
            });
            busy.store(false, std::sync::atomic::Ordering::Relaxed);
        });
        self.jobs.push(rx);
    }

    /// What the plugin that says how games start keeps for a game about to
    /// start through Ectomatter, put on the game's page where the page has
    /// none of its own (game/through).
    pub(super) fn keep_through(&mut self, app: u32) {
        let id = crate::games::steam_id(app);
        let facts = self.launch_facts(&id);
        let Some(plugin) = self.hosted.launcher().map(String::from) else { return };
        if facts.is_null() {
            return;
        }
        let own = self.games.library.plugin_values(&id, &plugin);
        let every = self.games.library.plugin_values(crate::games::EVERY, &plugin);
        let game = serde_json::json!({ "id": id, "name": self.games.name(&id, &self.front_apps), "app": app });
        match self.hosted.through(&game, &own, &every, &facts) {
            Some(Ok(keep)) => {
                for (key, v) in keep.as_object().into_iter().flatten() {
                    if self.games.library.plugin_values(&id, &plugin).get(key).is_none() {
                        self.games.library.set_plugin_value(&id, &plugin, key, Some(v.clone()));
                    }
                }
            }
            Some(Err(e)) => eprintln!("ectomatter: what {} keeps as it starts through Ectomatter: {e}", game["name"].as_str().unwrap_or("")),
            None => {}
        }
    }

    /// What the put-back thread said: a command run (kept with it), what was
    /// put back after Ectomatter stopped short (said, here and as a notice), a
    /// change that couldn't be made. True if the Library has news.
    pub(super) fn worker_told(&mut self) -> bool {
        let mut news = false;
        while let Ok(t) = self.worker.told.try_recv() {
            news = true;
            match t {
                crate::putback::Told::Ran { script, game, quit, ran, at } => self.games.script_ran(script, &game, quit, &ran, at),
                crate::putback::Told::PutBack { name, words, after_stop } => {
                    eprintln!("ectomatter: put back for {name}: {}", words.join(", "));
                    if after_stop && !words.is_empty() {
                        let said = format!("Ectomatter stopped while {name} ran, so it put back {} now.", crate::text::list(&words));
                        self.games.say(said.clone(), false);
                        crate::tray::notify("Ectomatter put things back", &said);
                    }
                }
                crate::putback::Told::Failed { words, why } => {
                    eprintln!("ectomatter: couldn’t change {words}: {why}");
                    self.games.say(format!("Ectomatter couldn’t change {words}: {why}"), true);
                }
                crate::putback::Told::Started { id, seq } => self.started_through(&id, seq),
            }
        }
        news
    }

    /// Games started and quit, as the Library's watch saw them: the window in
    /// front looked at again (a game's window may have come up before its
    /// game was seen), the plugins' changes made or put back, and the
    /// keyboard's screen told.
    pub(super) fn games_seen(&mut self, events: Vec<crate::games_page::Event>) {
        use crate::lcd_player::{Control, GameSeen};
        for e in &events {
            let (crate::games_page::Event::Started(r) | crate::games_page::Event::Quit(r)) = e;
            let name = self.games.name(&r.id, &self.front_apps);
            let started = matches!(e, crate::games_page::Event::Started(_));
            eprintln!("ectomatter: {name} {}", if started { "started" } else { "quit" });
            // what the plugins change, for games and apps alike (SteamVR's monitor turned off)
            if started {
                if self.starting.contains_key(&r.id) {
                    // (started through Ectomatter: its changes were made as its launcher asked)
                    self.early.retain(|(g, _, _)| *g != r.id);
                } else {
                    let plan = self.game_plan(r, &name);
                    self.starting.insert(r.id.clone(), (plan.seq, false));
                    self.worker.start_game(plan);
                }
            } else {
                self.worker.quit_game(&r.id);
                self.starting.remove(&r.id);
                self.early.retain(|(g, _, _)| *g != r.id);
            }
            // (an app Steam starts, such as SteamVR or Wallpaper Engine, is no game starting)
            if !self.games.library.is_game(&r.id) {
                continue;
            }
            let entry = self.games.library.entries(&|id| self.front_apps.name(id)).into_iter().find(|x| x.id == r.id);
            let game = if started {
                crate::lcd_page::Game { name, since: r.since, ended: 0, minutes: entry.as_ref().map_or(0, |x| x.minutes), last: entry.as_ref().map_or(0, |x| x.last), changing: String::new(), done: (0, 0) }
            } else {
                let mut g = self.game_now.clone().filter(|g| g.name == name).unwrap_or_else(|| crate::lcd_page::Game { name, since: r.since, ..Default::default() });
                g.ended = crate::games::now();
                g
            };
            self.game_now = Some(game.clone());
            if let Some(pl) = &self.player {
                pl.tell(Control::Game(if started { GameSeen::Started(game) } else { GameSeen::Quit(game) }));
            }
        }
        if let Some(f) = self.last_front.clone() {
            self.front_app(f);
        }
    }

    /// An app come to the front, as KWin says (see front_app.rs): each device
    /// switches to its profile tied to the app; once an app it followed
    /// leaves the front, back to the profile chosen by hand. Ectomatter's own
    /// windows change nothing, so a profile can be changed with Ectomatter in front.
    pub fn front_app(&mut self, f: crate::front_app::Front) {
        // (a game's window goes by its game's id, whatever the window calls itself)
        let app = self.games.front_id(&f);
        self.app_windows(&f, &app);
        if f.closed {
            return;
        }
        // a window just opened, in front or not: only a running game's matters here, to
        // its launch screen; what's in front is told as it comes to the front
        if f.opened {
            if self.games.running.iter().any(|r| r.id == app) {
                if let Some(pl) = &self.player {
                    pl.tell(crate::lcd_player::Control::Game(crate::lcd_player::GameSeen::Window));
                }
            }
            return;
        }
        let seen = {
            let (games, seen) = (&self.games, &mut self.front_apps);
            let name = games.name(&app, seen);
            seen.saw_as(&f, &app, || name)
        };
        if f.is_ectomatter() {
            return;
        }
        // a running game's window come to the front: its launch screen can go on
        if self.games.running.iter().any(|r| r.id == app) {
            if let Some(pl) = &self.player {
                pl.tell(crate::lcd_player::Control::Game(crate::lcd_player::GameSeen::Window));
            }
        }
        self.last_front = Some(f);
        let mut changed = false;
        // what the Library says each device does while this is in front (a game
        // with no choice of its own follows Every game's); Don't switch is none
        let wanted = |d: &str| match self.games.library.wanted(&app, d) {
            Some(crate::games::Choice::Profile(p)) => Some(p),
            _ => None,
        };
        let (mouse_wants, kb_wants) = (wanted(crate::mouse::key()), wanted(crate::cores::keyboard_key()).and_then(|p| p.parse::<u8>().ok()));
        let wants: Vec<(String, String, String, Option<String>)> = self
            .hosted
            .devices()
            .into_iter()
            .map(|(plugin, d)| {
                let id = d["device"].as_str().unwrap_or("").to_string();
                let w = wanted(&id);
                (plugin, id, d["name"].as_str().unwrap_or("").to_string(), w)
            })
            .collect();
        // the mouse
        if let Some(store) = &self.mouse.profiles {
            let (active, count) = (store.active, store.list.len());
            match mouse_wants.and_then(|id| store.list.iter().position(|p| p.id == id)) {
                Some(i) if i != active => {
                    self.follow_back[1].get_or_insert(active);
                    self.mouse.choose_profile(i);
                    eprintln!("ectomatter: the mouse follows {app}");
                    changed = true;
                }
                Some(_) => {}
                None => {
                    if let Some(back) = self.follow_back[1].take().filter(|b| *b < count && *b != active) {
                        self.mouse.choose_profile(back);
                        changed = true;
                    }
                }
            }
        }
        // the keyboard
        if self.page.dev.is_some() {
            let active = self.page.active();
            match kb_wants.filter(|n| crate::cores::profiles().contains(n)) {
                Some(n) if n != active => {
                    self.follow_back[0].get_or_insert(active as usize);
                    self.choose_profile(n);
                    eprintln!("ectomatter: the keyboard follows {app}");
                    changed = true;
                }
                Some(_) => {}
                None => {
                    if let Some(back) = self.follow_back[0].take().filter(|b| *b as u8 != active) {
                        self.choose_profile(back as u8);
                        changed = true;
                    }
                }
            }
        }
        // each device a plugin brought in (OpenRGB's lighting): the profile this app wants for
        // it, put on below with the scene's, which comes first (see plugins_to)
        for (plugin, id, name, want) in wants {
            let follow = format!("{plugin}/{id}");
            match want {
                Some(p) if self.following.get(&follow) != Some(&p) => {
                    eprintln!("ectomatter: {name} follows {app}");
                    self.following.insert(follow, p);
                    changed = true;
                }
                Some(_) => {}
                None => {
                    if self.following.remove(&follow).is_some() {
                        changed = true;
                    }
                }
            }
        }
        // the scene: the one this app or game chose while it's in front (a game with none
        // of its own follows Every game's); once it switched, back to the one before
        match self.games.library.wanted(&app, "scene") {
            // (a scene since deleted is no choice)
            Some(crate::games::Choice::Profile(p)) if p == "none" || self.scenes().iter().any(|s| s.id == p) => {
                let want = (p != "none").then_some(p);
                if want != self.scene {
                    if self.scene_back.is_none() {
                        self.scene_back = Some(self.scene.clone());
                    }
                    eprintln!("ectomatter: the scene follows {app}");
                    self.set_scene(want, false);
                    changed = true;
                }
            }
            _ => {
                if let Some(back) = self.scene_back.take() {
                    if back != self.scene {
                        self.set_scene(back, false);
                        changed = true;
                    }
                }
            }
        }
        self.plugins_sync();
        if changed {
            self.show_mouse_lights();
        }
        if changed || seen {
            self.refresh();
        }
    }

    /// The chosen button's jobs of one kind, under the Key detail box's menu field.
    pub(super) fn open_mouse_menu(&mut self, kind: usize, anchor: (f32, f32, f32), under: f32) {
        if self.just_shut(anchor) {
            return;
        }
        self.close_menu();
        self.close_picker();
        let Some(b) = self.mouse.button else { return };
        let now = self.mouse.job(b);
        // (a macro ticked whatever it plays as)
        let same = |a: &serde_json::Value| *a == now || (a.get("macro").is_some() && a["macro"] == now["macro"]);
        let choices = self.mouse.choices_for(kind);
        let mut rows: Vec<MenuRow> = Vec::new();
        // (each row's job, kept as it was offered: the list may change while the menu's
        // open, a plugin's device coming or going; none for a heading or a note)
        let mut picks: Vec<Option<serde_json::Value>> = Vec::new();
        if kind == crate::mouse::DEVICES_KIND {
            // other devices' jobs and the scenes', under their headings, each by its own name,
            // as the keys' Profile page and the Macro Editor list them
            let names = self.job_names();
            let group = |a: &serde_json::Value| crate::jobs::Job::read(a).and_then(|j| names.group(&j)).unwrap_or_default();
            let mut groups: Vec<(String, String)> = Vec::new();
            for (_, a) in &choices {
                if !groups.iter().any(|(k, _)| *k == group(a).0) {
                    groups.push(group(a));
                }
            }
            for (key, head) in groups {
                rows.push(MenuRow { kind: 1, label: head.as_str().into(), ..Default::default() });
                picks.push(None);
                for (label, a) in choices.iter().filter(|(_, a)| group(a).0 == key) {
                    let words = crate::jobs::Job::read(a).map_or_else(|| label.clone(), |j| names.short(&j));
                    rows.push(MenuRow { kind: 0, label: words.into(), tick: same(a), ..Default::default() });
                    picks.push(Some(a.clone()));
                }
            }
        } else {
            for (label, a) in &choices {
                rows.push(MenuRow { kind: 0, label: label.as_str().into(), tag: "".into(), tick: same(a), sub: "".into(), ..Default::default() });
                picks.push(Some(a.clone()));
            }
        }
        if rows.is_empty() && kind == crate::mouse::MACRO_KIND {
            rows.push(MenuRow { kind: 3, label: "No macros yet. Make one in the Macro Editor.".into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() });
            picks.push(None);
        }
        if rows.is_empty() && kind == crate::mouse::DEVICES_KIND {
            rows.push(MenuRow { kind: 3, label: "Plug in another device to give this button one of its jobs.".into(), tag: "".into(), tick: false, sub: "".into(), ..Default::default() });
            picks.push(None);
        }
        self.mouse.menu_picks = picks;
        self.hang_menu(rows, under, anchor);
        self.ui.set_mouse_menu_open(true);
        self.mouse.menu = Some(kind);
    }

    /// The Lighting page's effects and its tile, laid out for the draft. The
    /// pieces are changed in place, so one being used is never rebuilt under
    /// the pointer.
    /// The Lighting page, drawn from the keyboard's description: the effects'
    /// tiles, and the chosen effect's boxes.
    pub(super) fn refresh_lighting(&mut self) {
        let mode = self.page.effect_id();
        let effects = self.page.effects();
        let ids: Vec<String> = effects.iter().map(|e| e["id"].as_str().unwrap_or("").to_string()).collect();
        let scale = self.scale;
        let wide = pic_wide(self.ui.get_light_fx_w());
        if self.icons.as_ref().map(|(s, w, i, _)| (*s, *w, i)) != Some((scale, wide, &ids)) {
            self.icons = Some((scale, wide, ids.clone(), ids.iter().map(|id| lighting::icon(crate::described::tile(id).0, scale, wide)).collect()));
        }
        let icons = &self.icons.as_ref().unwrap().3;
        let tiles: Vec<EffectItem> = effects
            .iter()
            .zip(icons)
            .map(|(e, icon)| {
                let id = e["id"].as_str().unwrap_or("");
                EffectItem { name: e["label"].as_str().unwrap_or(id).into(), icon: icon.clone(), on: id == mode, motion: crate::described::tile(id).1 }
            })
            .collect();
        self.ui.set_effect_moves(tiles.iter().any(|e| e.on && e.motion != 0));
        show_rows(self.ui.get_effects(), |m| self.ui.set_effects(m), tiles);
        let profile = self.page.profile_name(self.page.editing);
        let look = crate::described::Look {
            kind: "keyboard",
            profile: &profile,
            open: self.pick.as_ref().and_then(|p| match p.piece {
                Some(crate::described::Piece::Effect(k)) => Some((k, p.index)),
                _ => None,
            }),
            painting: self.pick.as_ref().is_some_and(|p| p.ctl == Ctl::Paint),
            menu: self.light_menu,
            hot: self.picture_area(),
            demo: self.page.demo,
            paint: self.paint,
        };
        let (set_w, place_w) = self.light_box_widths();
        let b = crate::described::boxes(&self.page.keys_light(), &self.page.effect_desc(&mode), &self.page.effect_settings(), &look, set_w, place_w);
        // (where and when beside the settings, for an effect with places to light)
        let mut shown = vec![crate::boxes::Laid { title: b.title, least: crate::described::SET_W, h: b.set_h, cells: b.set }];
        if b.kind == 0 && !b.place_title.is_empty() {
            shown.push(crate::boxes::Laid { title: b.place_title, least: crate::described::PLACE_W, h: b.place_h, cells: b.place });
        }
        self.light_boxes.show(shown);
    }

    /// How wide the page's row of boxes made the Lighting tab's settings and its where
    /// and when (their least widths until it has drawn them).
    pub(super) fn light_box_widths(&self) -> (f32, f32) {
        (self.light_boxes.width(0, crate::described::SET_W), self.light_boxes.width(1, crate::described::PLACE_W))
    }

    /// Whether the Lighting page lights some zones of the keyboard, not all of
    /// it: the effect's zones setting, if it has one, on Some zones.
    pub(super) fn some_areas(&self) -> bool {
        let p = &self.page;
        // (nothing to say before the keyboard's profiles are read)
        if p.view != View::Lighting || !p.drafts.contains_key(&p.editing) {
            return false;
        }
        let e = p.effect_desc(&p.effect_id());
        let light = p.keys_light();
        crate::described::params(&e).iter().find(|(_, d)| d["type"] == "zones").is_some_and(|(id, d)| !crate::described::whole(&light, crate::described::kept(&p.effect_settings(), id, d).as_u64().unwrap_or(0)))
    }

    /// The zone a key is in, by its place in the keys' light's zones (never the whole keyboard).
    pub(super) fn area_of(&self, pos: u32) -> Option<usize> {
        let light = self.page.keys_light();
        let n = light["zones"].as_array().map_or(0, |z| z.len());
        (0..n).filter(|&i| light["zones"][i]["whole"] != true).find(|&i| crate::described::zone_bit(&light, i).is_some_and(|bit| self.page.zone_set(bit as u8).is_some_and(|keys| keys.contains(&pos))))
    }

    /// On Some areas, the area the pointer is over on the keyboard's picture.
    pub(super) fn picture_area(&self) -> Option<usize> {
        if !self.some_areas() {
            return None;
        }
        self.area_of(self.picture_key?)
    }

    /// The keys of the area the pointer is over on the Lighting page, if it is
    /// over one: its chip, or the area itself on the keyboard's picture.
    pub(super) fn area_keys(&self) -> Option<Vec<u32>> {
        let p = &self.page;
        if p.view != View::Lighting {
            return None;
        }
        let bit = crate::described::zone_bit(&p.keys_light(), self.hover_area.or_else(|| self.picture_area())?)? as u8;
        Some(p.zone_set(bit).unwrap_or_else(|| p.layout.iter().filter(|k| k.led).map(|k| k.pos).collect()))
    }

    /// Where the keys of the area the pointer is over are, in the keyboard's
    /// own pixels `w` across: the rest of the picture is dimmed around them.
    pub fn dim_rects(&self, w: u32) -> Option<Vec<[f32; 4]>> {
        let keys = self.area_keys()?;
        let (x0, y0, vw, _) = self.page.view_box;
        let s = w as f64 / vw;
        Some(
            self.page
                .layout
                .iter()
                .filter(|k| keys.contains(&k.pos))
                .map(|k| [((k.x - x0 + keyboard::GAP / 2.0) * s) as f32, ((k.y - y0 + keyboard::GAP / 2.0) * s) as f32, ((k.w - keyboard::GAP) * s) as f32, ((k.h - keyboard::GAP) * s) as f32])
                .collect(),
        )
    }

    /// The tile's pieces, changed in place where they can be.
    /// Show the page's pieces, changing only the ones that changed. Pieces that
    /// stay keep their place in the model, so one being dragged (a slider) keeps
    /// its pointer even when pieces before or after it come or go.
    pub(super) fn show_cells(&mut self, cells: Vec<Cell>) {
        sync_cells(&self.cells, cells);
    }

    /// The keyboard's firmware, as its core describes it (section 4,
    /// firmware): whose it is, and what a custom build can do.
    pub(super) fn firmware(&self) -> serde_json::Value {
        self.page.dev.as_ref().map_or(serde_json::Value::Null, |d| d["firmware"].clone())
    }
}
