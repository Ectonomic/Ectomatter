//! Add a core and Add a plugin: the list of packages Ectonomic reviewed
//! (registry.rs), a row for each of the kind asked for, the one chosen with
//! what it can reach; Add puts it in place as reviewed, and a core whose
//! devices this computer has no rule for asks for the system's password once
//! to write one (permission.rs). "From an address or a file…" leads to the
//! form that adds a package nobody reviewed.

use super::*;

/// The list as Add a core or Add a plugin shows it.
pub struct Browser {
    kind: crate::packages::Kind,
    listed: Vec<crate::registry::Listed>,
    chosen: Option<usize>,
    why: Option<String>,
    loading: Option<mpsc::Receiver<Result<Vec<crate::registry::Listed>, String>>>,
    adding: Option<mpsc::Receiver<Result<crate::core_host::Manifest, String>>>,
}

impl Front {
    /// The list opened for a kind of package, with one already chosen where
    /// it's named (Review, on a version waiting): the list kept within the day
    /// at once, else read from the web meanwhile.
    pub(super) fn open_browser(&mut self, kind: crate::packages::Kind, choose: Option<&str>) {
        // (a list in a file is read at once; one on the web from the cache, or meanwhile)
        let (kept, why) = if crate::registry::is_local() {
            match crate::registry::fetch() {
                Ok(l) => (Some(l), None),
                Err(e) => (Some(Vec::new()), Some(format!("Ectomatter can’t read the list right now: {e}."))),
            }
        } else {
            (crate::registry::cached().filter(|(_, age)| *age < Duration::from_secs(24 * 3600)).map(|(l, _)| l), None)
        };
        let loading = kept.is_none().then(|| {
            let (tx, rx) = mpsc::channel();
            let _ = std::thread::Builder::new().name("package list".into()).spawn(move || {
                let _ = tx.send(crate::registry::fetch());
            });
            rx
        });
        let listed: Vec<_> = kept.unwrap_or_default().into_iter().filter(|l| l.kind() == kind).collect();
        let chosen = choose.and_then(|id| listed.iter().position(|l| l.id() == id));
        self.browser = Some(Browser { kind, listed, chosen, why, loading, adding: None });
        self.show_browser();
    }

    /// The list as it stands, into the dialog.
    pub(super) fn show_browser(&mut self) {
        let Some(b) = &self.browser else { return };
        self.ui.set_gl_modal(6);
        let word = b.kind.word();
        self.ui.set_gl_plugin_title(format!("Add a {word}").into());
        self.ui.set_gl_plugin_sub(format!("Each {word} here was reviewed by Ectonomic. Choose one to see what the {word} can reach on this computer.").into());
        let rows: Vec<crate::PackageItem> = b
            .listed
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let have = self.packages.iter().find(|p| p.id() == l.id());
                let tag = match have {
                    Some(h) if crate::packages::version_key(h.version()) < crate::packages::version_key(l.version()) => format!("{} waiting", l.version()),
                    Some(h) if !h.added => "Comes with Ectomatter".to_string(),
                    Some(_) => "Added".to_string(),
                    None => String::new(),
                };
                let mut can = crate::packages::can_words(&l.manifest);
                can.push(format!("Version {} · {}", l.version(), l.manifest["license"].as_str().unwrap_or("no license named")));
                crate::PackageItem {
                    name: l.manifest["name"].as_str().unwrap_or(l.id()).into(),
                    by: l.publisher.as_str().into(),
                    about: l.manifest["description"].as_str().unwrap_or("").into(),
                    tag: tag.into(),
                    can: can.join("\n").into(),
                    chosen: b.chosen == Some(i),
                }
            })
            .collect();
        self.ui.set_gl_browse(ModelRc::new(VecModel::from(rows)));
        let (mut state, mut bad, mut can_add, mut go) = (String::new(), false, false, "Add");
        if let Some(why) = &b.why {
            (state, bad) = (why.clone(), true);
        } else if b.loading.is_some() && b.listed.is_empty() {
            state = format!("Reading the list of {word}s…");
        } else if b.listed.is_empty() {
            state = format!("No {word} is on the list for this computer yet.");
        }
        if let Some(l) = b.chosen.and_then(|i| b.listed.get(i)) {
            let name = l.manifest["name"].as_str().unwrap_or(l.id());
            let have = self.packages.iter().find(|p| p.id() == l.id());
            if b.adding.is_some() {
                state = format!("Adding {name}…");
            } else if let Some(h) = have.filter(|h| crate::packages::version_key(h.version()) >= crate::packages::version_key(l.version())) {
                state = format!("Ectomatter already has {} {}.", h.name(), h.version());
            } else if b.why.is_none() {
                can_add = true;
                if have.is_some() {
                    go = "Update";
                }
                if !crate::permission::missing(&l.manifest).is_empty() {
                    state = format!("Adding {name} asks for your password once, so this computer lets Ectomatter open the device.");
                } else if b.why.is_none() {
                    state = String::new();
                }
            }
        }
        self.ui.set_gl_plugin_state(state.into());
        self.ui.set_gl_plugin_bad(bad);
        self.ui.set_gl_can_add(can_add);
        self.ui.set_gl_browse_go(go.into());
    }

    /// A row pressed: chosen, or let go where it was.
    pub(super) fn browse_pick(&mut self, i: i32) {
        let Some(b) = &mut self.browser else { return };
        if b.adding.is_some() {
            return;
        }
        b.why = None;
        b.chosen = if b.chosen == Some(i as usize) { None } else { Some(i as usize) };
        self.show_browser();
    }

    /// "From an address or a file…": the form that adds a package nobody reviewed.
    pub(super) fn browse_other(&mut self) {
        let Some(b) = self.browser.take() else { return };
        self.add_package_form(b.kind);
    }

    /// Add, in the list: the chosen package put in place, as reviewed, on a thread of its own.
    pub(super) fn browse_add_done(&mut self) {
        let Some(b) = &mut self.browser else { return };
        let Some(l) = b.chosen.and_then(|i| b.listed.get(i)).cloned() else { return };
        if b.adding.is_some() {
            return;
        }
        let into = crate::packages::added_dir(l.kind());
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new().name("package add".into()).spawn(move || {
            let _ = tx.send(crate::registry::install(&l, &into));
        });
        b.adding = Some(rx);
        self.show_browser();
    }

    /// What came of the list being read, and of a package being added.
    pub(super) fn browser_tick(&mut self) {
        let Some(b) = &mut self.browser else { return };
        let mut changed = false;
        if let Some(rx) = &b.loading {
            if let Ok(r) = rx.try_recv() {
                b.loading = None;
                match r {
                    Ok(all) => {
                        let id = b.chosen.and_then(|i| b.listed.get(i)).map(|l| l.id().to_string());
                        b.listed = all.into_iter().filter(|l| l.kind() == b.kind).collect();
                        b.chosen = id.and_then(|id| b.listed.iter().position(|l| l.id() == id));
                    }
                    // (the list kept from before stays where there is one)
                    Err(e) if b.listed.is_empty() => b.why = Some(format!("Ectomatter can’t read the list right now: {e}.")),
                    Err(_) => {}
                }
                changed = true;
            }
        }
        if let Some(rx) = &b.adding {
            if let Ok(r) = rx.try_recv() {
                b.adding = None;
                match r {
                    Ok(m) => {
                        self.browser = None;
                        self.ui.set_gl_modal(0);
                        self.package_added(&m.id, &m.name);
                        return;
                    }
                    Err(e) => b.why = Some(format!("It wasn’t added: {e}.")),
                }
                changed = true;
            }
        }
        if changed {
            self.show_browser();
        }
    }

    /// A core's devices this computer has no rule for given one, with the
    /// system's password prompt, on a thread of its own; Settings says how it went.
    pub(super) fn allow_devices(&mut self, manifest: &serde_json::Value) {
        if crate::permission::missing(manifest).is_empty() || self.granting.is_some() {
            return;
        }
        let name = manifest["name"].as_str().unwrap_or("the device").to_string();
        let m = manifest.clone();
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new().name("device rule".into()).spawn(move || {
            let _ = tx.send(crate::permission::grant(&m));
        });
        self.granting = Some((name, rx));
    }

    /// How the rule for a core's devices went: said in Settings' bar.
    pub(super) fn granting_tick(&mut self) {
        let Some((name, rx)) = &self.granting else { return };
        let Ok(r) = rx.try_recv() else { return };
        let name = name.clone();
        self.granting = None;
        match r {
            Ok(()) => self.settings_say(format!("This computer lets Ectomatter open the {name} now."), false),
            Err(e) => self.settings_say(format!("This computer still doesn’t let Ectomatter open the {name}: {e}. Allow, under Devices, asks again."), true),
        }
        self.refresh();
    }
}
