//! Ectomatter was named Ectodeck until 2026-10-22. What an Ectodeck on this
//! computer kept is moved into Ectomatter's own folders once, at the first
//! start (main.rs), and the old name is taken out of what it wrote: the key
//! jobs the settings keep, its KWin script, and the comments in the light
//! setups and the pictures put on keys. A check run offscreen, or a test,
//! moves nothing.

use std::path::{Path, PathBuf};

/// The app id Ectodeck kept its folders under.
const OLD_ID: &str = "com.ectonomic.ectodeck";

pub fn move_old_home() {
    if crate::desk::keeps_nothing() {
        return;
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
    let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
    for (old, new) in [(config.join(OLD_ID), crate::config_dir()), (data.join(OLD_ID), crate::data_dir())] {
        if old.is_dir() && !new.exists() {
            match std::fs::rename(&old, &new) {
                Ok(()) => eprintln!("ectomatter: moved {} to {}", old.display(), new.display()),
                Err(e) => eprintln!("ectomatter: couldn’t move {}: {e}", old.display()),
            }
        }
    }
    // (a key given a job keeps the job's name in the settings)
    replace_in(&crate::config_dir().join("settings.json"), "\"ectodeck-job:", "\"ectomatter-job:");
    // (Ectomatter writes its KWin script again under its own name)
    let _ = std::fs::remove_file(crate::data_dir().join("kwin").join("ectodeck-front-app.js"));
    for f in files_in(&crate::data_dir().join("light")) {
        if f.extension().is_some_and(|e| e == "json" || e == "svg") {
            replace_in(&f, "Ectodeck", "Ectomatter");
        }
    }
}

/// A file with every `from` in it made `to`, written only where there was one.
fn replace_in(path: &Path, from: &str, to: &str) {
    let Ok(text) = std::fs::read_to_string(path) else { return };
    if text.contains(from) {
        match std::fs::write(path, text.replace(from, to)) {
            Ok(()) => eprintln!("ectomatter: the old name taken out of {}", path.display()),
            Err(e) => eprintln!("ectomatter: couldn’t write {}: {e}", path.display()),
        }
    }
}

/// Every file in a folder and the folders in it.
fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(files_in(&p));
        } else {
            out.push(p);
        }
    }
    out
}
