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

/// The settings kept under names from before, moved to the names they have
/// now: the keyboard's Control was `devices.z10.control` before every core
/// was turned on and off alike, by its switch, and Home's desk kept the
/// mouse as "mouse". A check run offscreen, or a test, moves nothing.
pub fn move_old_settings(settings: &crate::settings::Settings) {
    if crate::desk::keeps_nothing() {
        return;
    }
    let switch = crate::packages::switch_of(crate::packages::Kind::Core, "evga-z10");
    if let Some(v) = settings.take("devices.z10.control").filter(|_| settings.get(&switch).is_none()) {
        let _ = settings.set(&switch, v);
    }
    // (Home's desk kept the mouse as "mouse" before every device was kept by
    // its key, the Sensei 310's "sensei310"; the window's state copied first)
    if let Some(mut desk) = crate::uistate::get("desk").and_then(|d| d.as_object().cloned()).filter(|d| d.contains_key("mouse")) {
        let file = crate::config_dir().join("window.json");
        let kept = file.with_file_name("window.json.before-device-keys");
        if !kept.exists() {
            let _ = std::fs::copy(&file, &kept);
        }
        desk_by_key(&mut desk);
        crate::uistate::set("desk", serde_json::Value::Object(desk));
    }
}

/// Home's desk's places, the mouse's under its key, "sensei310", where it
/// was kept as "mouse"; a place kept under the key already stays.
fn desk_by_key(desk: &mut serde_json::Map<String, serde_json::Value>) {
    if let Some(at) = desk.remove("mouse") {
        desk.entry("sensei310".to_string()).or_insert(at);
    }
}

/// The cores' folders of files kept under the names they had before: the
/// mouse's core was "steelseries" before it was named for its model.
pub fn core_folders(data: &Path) -> Vec<(PathBuf, PathBuf)> {
    vec![(data.join("cores").join("steelseries"), data.join("cores").join("steelseries-sensei-310"))]
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    /// The mouse's place on Home's desk moves to its key, and one already
    /// kept there stays.
    #[test]
    fn the_desk_keeps_the_mouse_by_its_key() {
        let mut desk = json!({ "z10": [-720.0, 48.0], "mouse": [-1008.0, 96.0] }).as_object().unwrap().clone();
        super::desk_by_key(&mut desk);
        assert_eq!(serde_json::Value::Object(desk.clone()), json!({ "z10": [-720.0, 48.0], "sensei310": [-1008.0, 96.0] }));
        let mut both = json!({ "mouse": [0.0, 0.0], "sensei310": [5.0, 5.0] }).as_object().unwrap().clone();
        super::desk_by_key(&mut both);
        assert_eq!(serde_json::Value::Object(both), json!({ "sensei310": [5.0, 5.0] }));
    }
}
