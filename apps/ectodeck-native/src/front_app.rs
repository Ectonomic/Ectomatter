//! The app in front, for profiles that follow it. On KDE Plasma, Ectodeck
//! loads a small script of its own into KWin as it starts: each time a
//! window comes to the front, the script tells Ectodeck over D-Bus which app
//! it is (KWin's scripting API: workspace.windowActivated, callDBus). On a
//! desktop without KWin nothing is told, and profiles stay as they're chosen.
//! Another desktop's watcher would tell Ectodeck the same thing its own way.
//!
//! The apps seen in front are kept, newest first, for the menu that ties a
//! profile to one; each by its desktop file's name (org.kde.konsole), or its
//! window class where it has none, and named as its desktop file names it.

use std::path::PathBuf;

use serde_json::{Value, json};
use smithay_client_toolkit::reexports::calloop::channel::Sender;

/// Where Ectodeck is on the session bus, for KWin's script to call.
pub const SERVICE: &str = "com.ectonomic.Ectodeck";
pub const PATH: &str = "/com/ectonomic/Ectodeck";
pub const IFACE: &str = "com.ectonomic.Ectodeck.FrontApp";
/// The script's name in KWin.
const SCRIPT: &str = "ectodeck-front-app";

/// What KWin says of a window come to the front.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Front {
    pub class: String,
    pub title: String,
    pub desktop: String,
}

impl Front {
    /// The app, as a profile is tied to it: its desktop file's name, or its
    /// window class where it has none.
    pub fn app(&self) -> String {
        let d = self.desktop.trim().trim_end_matches(".desktop");
        if d.is_empty() { self.class.trim().to_lowercase() } else { d.to_string() }
    }

    /// Whether it's one of Ectodeck's own windows, which no profile follows:
    /// a profile is changed in Ectodeck with its own app in front.
    pub fn is_ectodeck(&self) -> bool {
        let a = self.app().to_lowercase();
        a.contains("ectodeck") || self.class.to_lowercase().contains("ectodeck")
    }
}

struct Listener {
    tell: Sender<Front>,
}

#[zbus::interface(name = "com.ectonomic.Ectodeck.FrontApp")]
impl Listener {
    /// A window come to the front: its class, its title, its desktop file's name.
    fn window_activated(&self, class: String, title: String, desktop: String) {
        let _ = self.tell.send(Front { class, title, desktop });
    }
}

/// The script Ectodeck loads into KWin.
fn script() -> String {
    format!(
        r#"// Ectodeck: tells Ectodeck which app is in front, for the profiles that
// follow one. Ectodeck loads it as it starts and takes it out as it quits.
function tell(w) {{
    if (!w) return;
    callDBus("{SERVICE}", "{PATH}", "{IFACE}", "WindowActivated",
        String(w.resourceClass || ""), String(w.caption || ""), String(w.desktopFileName || ""));
}}
workspace.windowActivated.connect(tell);
tell(workspace.activeWindow);
"#
    )
}

/// Start hearing which app is in front: Ectodeck on the session bus, told
/// through `tell`, and its script loaded into KWin. The connection, kept for
/// as long as Ectodeck runs; none where the bus or KWin isn't there.
pub fn start(tell: Sender<Front>) -> Option<zbus::blocking::Connection> {
    let conn = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(SERVICE))
        .and_then(|b| b.serve_at(PATH, Listener { tell }))
        .and_then(|b| b.build())
        .map_err(|e| eprintln!("ectodeck-native: the app in front won't be heard: {e}"))
        .ok()?;
    match load(&conn) {
        Ok(()) => eprintln!("ectodeck-native: KWin tells Ectodeck the app in front"),
        Err(e) => eprintln!("ectodeck-native: KWin won't tell Ectodeck the app in front: {e}"),
    }
    Some(conn)
}

fn kwin(conn: &zbus::blocking::Connection) -> zbus::Result<zbus::blocking::Proxy<'static>> {
    zbus::blocking::Proxy::new(conn, "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting")
}

/// The script written to Ectodeck's data folder and loaded into KWin, in
/// place of one an Ectodeck before left there.
fn load(conn: &zbus::blocking::Connection) -> Result<(), String> {
    let path = crate::data_dir().join("kwin").join(format!("{SCRIPT}.js"));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, script()).map_err(|e| e.to_string())?;
    let k = kwin(conn).map_err(|e| e.to_string())?;
    let loaded: bool = k.call("isScriptLoaded", &(SCRIPT,)).map_err(|e| e.to_string())?;
    if loaded {
        let _: bool = k.call("unloadScript", &(SCRIPT,)).map_err(|e| e.to_string())?;
    }
    let id: i32 = k.call("loadScript", &(path.to_string_lossy().as_ref(), SCRIPT)).map_err(|e| e.to_string())?;
    if id < 0 {
        return Err("KWin didn't load the script".into());
    }
    let () = k.call("start", &()).map_err(|e| e.to_string())?;
    Ok(())
}

/// Ectodeck's script taken out of KWin, as Ectodeck quits.
pub fn stop(conn: &zbus::blocking::Connection) {
    if let Ok(k) = kwin(conn) {
        let _: zbus::Result<bool> = k.call("unloadScript", &(SCRIPT,));
    }
}

/// An app seen in front: as profiles are tied to it, and its name.
#[derive(Clone, Debug, PartialEq)]
pub struct App {
    pub id: String,
    pub name: String,
}

/// The apps seen in front, newest first, kept in Ectodeck's own state.
#[derive(Debug, Default)]
pub struct Seen {
    pub list: Vec<App>,
}

/// How many apps the menu remembers.
const KEPT: usize = 24;

impl Seen {
    pub fn load() -> Seen {
        let list = crate::uistate::get("front_apps")
            .and_then(|v| v.as_array().cloned())
            .map(|a| a.iter().filter_map(|v| Some(App { id: v["id"].as_str()?.to_string(), name: v["name"].as_str()?.to_string() })).collect())
            .unwrap_or_default();
        Seen { list }
    }

    /// An app come to the front, first now; true if the list changed.
    pub fn saw(&mut self, f: &Front) -> bool {
        let id = f.app();
        if id.is_empty() || f.is_ectodeck() {
            return false;
        }
        if self.list.first().is_some_and(|a| a.id == id) {
            return false;
        }
        let app = self.list.iter().position(|a| a.id == id).map(|i| self.list.remove(i)).unwrap_or_else(|| App { name: name_of(&id, &f.class), id });
        self.list.insert(0, app);
        self.list.truncate(KEPT);
        crate::uistate::set("front_apps", Value::Array(self.list.iter().map(|a| json!({ "id": a.id, "name": a.name })).collect()));
        true
    }

    /// An app's name, by its id: as it was seen, or its id.
    pub fn name(&self, id: &str) -> String {
        self.list.iter().find(|a| a.id == id).map_or_else(|| name_of(id, id), |a| a.name.clone())
    }
}

/// An app's name, as its desktop file has it (`Name=`, in its Desktop Entry),
/// looked for where desktop files are kept; else its class with a capital.
pub fn name_of(id: &str, class: &str) -> String {
    let file = format!("{id}.desktop");
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local/share/applications"));
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    let data = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(data.split(':').filter(|d| !d.is_empty()).map(|d| PathBuf::from(d).join("applications")));
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    for d in dirs {
        if let Some(name) = std::fs::read_to_string(d.join(&file)).ok().and_then(|t| desktop_name(&t)) {
            return name;
        }
    }
    let c = if class.is_empty() { id } else { class };
    let mut chars = c.chars();
    chars.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + chars.as_str())
}

/// The name in a desktop file's Desktop Entry (its plain `Name=`, not a
/// translation's).
fn desktop_name(text: &str) -> Option<String> {
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if in_entry {
            if let Some(n) = line.strip_prefix("Name=") {
                return Some(n.trim().to_string()).filter(|n| !n.is_empty());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An app is its desktop file's name where it has one, its class else;
    /// Ectodeck's own windows are no app a profile follows; a desktop file's
    /// name is its Desktop Entry's own.
    #[test]
    fn the_app_in_front_is_named_as_its_desktop_file_names_it() {
        let f = Front { class: "konsole".into(), title: "~ : bash".into(), desktop: "org.kde.konsole".into() };
        assert_eq!(f.app(), "org.kde.konsole");
        assert_eq!(Front { class: "Steam".into(), ..Default::default() }.app(), "steam");
        assert!(Front { class: "ectodeck-native".into(), ..Default::default() }.is_ectodeck());
        let text = "[Desktop Entry]\nName=Konsole\nName[de]=Konsole DE\nExec=konsole\n[Desktop Action new]\nName=New Window\n";
        assert_eq!(desktop_name(text).as_deref(), Some("Konsole"));
        assert_eq!(desktop_name("[Other]\nName=No\n"), None);
        assert!(script().contains("workspace.windowActivated.connect(tell)"));
    }
}
