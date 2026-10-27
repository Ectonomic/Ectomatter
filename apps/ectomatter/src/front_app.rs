//! The app in front, for profiles that follow it. On KDE Plasma, Ectomatter
//! loads a small script of its own into KWin as it starts: each time a
//! window comes to the front, the script tells Ectomatter over D-Bus which app
//! it is (KWin's scripting API: workspace.windowActivated, callDBus). On a
//! desktop without KWin nothing is told, and profiles stay as they're chosen.
//! Another desktop's watcher would tell Ectomatter the same thing its own way.
//!
//! The apps seen in front are kept, newest first, for the menu that ties a
//! profile to one; each by its desktop file's name (org.kde.konsole), or its
//! window class where it has none, and named as its desktop file names it.
//! KWin also says which process the window is from, so a game's windows are
//! known for the game's, whatever each window calls itself (see games.rs).

use std::path::PathBuf;

use serde_json::{Value, json};
use smithay_client_toolkit::reexports::calloop::channel::Sender;

/// Where Ectomatter is on the session bus, for KWin's script to call.
pub const SERVICE: &str = "com.ectonomic.Ectomatter";
pub const PATH: &str = "/com/ectonomic/Ectomatter";
pub const IFACE: &str = "com.ectonomic.Ectomatter.FrontApp";
/// The script's name in KWin: with the Ectomatter it belongs to after it, so an
/// Ectomatter quitting as the next one starts takes out its own script, not the
/// next one's (an Ectomatter before this kept it under the name alone).
const SCRIPT: &str = "ectomatter-front-app";

fn script_name() -> String {
    format!("{SCRIPT}-{}", std::process::id())
}

/// Where the name of the script last loaded is kept, so one an Ectomatter left
/// loaded when it stopped without quitting is taken out by the next.
fn loaded_file() -> PathBuf {
    crate::data_dir().join("kwin").join("loaded")
}

/// What KWin says of a window come to the front: its class, title and
/// desktop file's name, and the process it's from (0 where KWin doesn't know);
/// or (`opened`) of a window just opened, which may not be in front, or
/// (`closed`) of one closed; and which window it is (KWin's own id for it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Front {
    pub class: String,
    pub title: String,
    pub desktop: String,
    pub pid: u32,
    pub opened: bool,
    pub closed: bool,
    pub window: String,
}

impl Front {
    /// The app, as a profile is tied to it: its desktop file's name, or its
    /// window class where it has none.
    pub fn app(&self) -> String {
        let d = self.desktop.trim().trim_end_matches(".desktop");
        if d.is_empty() { self.class.trim().to_lowercase() } else { d.to_string() }
    }

    /// Whether it's one of Ectomatter's own windows, which no profile follows:
    /// a profile is changed in Ectomatter with its own app in front.
    pub fn is_ectomatter(&self) -> bool {
        let a = self.app().to_lowercase();
        a.contains("ectomatter") || self.class.to_lowercase().contains("ectomatter")
    }
}

struct Listener {
    tell: Sender<Front>,
}

/// When KWin's script last told Ectomatter anything, and when it was last loaded
/// (milliseconds since 1970): a script that has run says which window is in front
/// at once, so one loaded a while ago that has said nothing since isn't running.
static HEARD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static LOADED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

#[zbus::interface(name = "com.ectonomic.Ectomatter.FrontApp")]
impl Listener {
    /// A window come to the front: its class, its title, its desktop file's
    /// name, and its process.
    /// Each says which Ectomatter's script tells it (`from`, the Ectomatter's process):
    /// one left in KWin by an Ectomatter before is told to stop telling.
    /// (the Ectomatter as words: a number of the script's own goes over the bus as
    /// whatever kind of number KWin makes of it)
    fn window_activated(&self, class: String, title: String, desktop: String, pid: i32, from: String) -> bool {
        if from != std::process::id().to_string() {
            return false;
        }
        HEARD.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
        let _ = self.tell.send(Front { class, title, desktop, pid: u32::try_from(pid).unwrap_or(0), ..Default::default() });
        true
    }

    /// A window just opened, in front or not (a game's, its launch screen waits for).
    fn window_opened(&self, class: String, title: String, desktop: String, pid: i32, from: String) -> bool {
        if from != std::process::id().to_string() {
            return false;
        }
        HEARD.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
        let _ = self.tell.send(Front { class, title, desktop, pid: u32::try_from(pid).unwrap_or(0), opened: true, ..Default::default() });
        true
    }

    /// A window come to the front ("activated"), opened ("opened", every one
    /// open as the script starts too) or closed ("closed"), with KWin's own id
    /// for it: for an app that doesn't run through Steam, its first window
    /// opening is the app starting, and its last closing the app quitting.
    #[allow(clippy::too_many_arguments)]
    fn window(&self, kind: String, class: String, title: String, desktop: String, pid: i32, window: String, from: String) -> bool {
        if from != std::process::id().to_string() {
            return false;
        }
        HEARD.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
        if std::env::var_os("ECTOMATTER_WINDOWS").is_some() {
            eprintln!("ectomatter: KWin: {kind} {class} {desktop} {pid} {window}");
        }
        let _ = self.tell.send(Front { class, title, desktop, pid: u32::try_from(pid).unwrap_or(0), opened: kind == "opened", closed: kind == "closed", window });
        true
    }
}

/// The script Ectomatter loads into KWin.
fn script() -> String {
    let me = std::process::id();
    format!(
        r#"// Ectomatter: tells Ectomatter which app is in front, for the profiles that
// follow one. Ectomatter loads it as it starts and takes it out as it quits;
// each says which Ectomatter loaded it ({me}), and one an Ectomatter before left
// here stops telling once the Ectomatter running says so.
const from = "{me}";
function heard(keep) {{
    if (keep === false) {{
        workspace.windowActivated.disconnect(tell);
        workspace.windowAdded.disconnect(opened);
        workspace.windowRemoved.disconnect(closed);
    }}
}}
function say(kind, w) {{
    if (!w) return;
    callDBus("{SERVICE}", "{PATH}", "{IFACE}", "Window", kind,
        String(w.resourceClass || ""), String(w.caption || ""), String(w.desktopFileName || ""), w.pid || 0, String(w.internalId || ""), from, heard);
}}
function tell(w) {{ say("activated", w); }}
// and each window as it opens, in front or not (a game's, which its launch screen waits for), and as it closes
function opened(w) {{ say("opened", w); }}
function closed(w) {{ say("closed", w); }}
workspace.windowActivated.connect(tell);
workspace.windowAdded.connect(opened);
workspace.windowRemoved.connect(closed);
// (the windows open already, then the one in front)
const all = workspace.windowList();
for (let i = 0; i < all.length; i++) {{
    if (all[i].normalWindow) opened(all[i]);
}}
tell(workspace.activeWindow);
"#
    )
}

/// Start hearing which app is in front: Ectomatter on the session bus, told
/// through `tell`, and its script loaded into KWin. The connection, kept for
/// as long as Ectomatter runs; none where the bus or KWin isn't there.
pub fn start(tell: Sender<Front>) -> Option<zbus::blocking::Connection> {
    let conn = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(SERVICE))
        .and_then(|b| b.serve_at(PATH, Listener { tell }))
        .and_then(|b| b.build())
        .map_err(|e| eprintln!("ectomatter: the app in front won't be heard: {e}"))
        .ok()?;
    match load(&conn) {
        Ok(()) => eprintln!("ectomatter: KWin tells Ectomatter the app in front"),
        Err(e) => eprintln!("ectomatter: KWin won't tell Ectomatter the app in front: {e}"),
    }
    Some(conn)
}

fn kwin(conn: &zbus::blocking::Connection) -> zbus::Result<zbus::blocking::Proxy<'static>> {
    zbus::blocking::Proxy::new(conn, "org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting")
}

/// The script written to Ectomatter's data folder and loaded into KWin under
/// this Ectomatter's own name: in place of one an Ectomatter before left there
/// (by the name alone, or one whose Ectomatter has gone); one whose Ectomatter is
/// still quitting is left for it to take out.
fn load(conn: &zbus::blocking::Connection) -> Result<(), String> {
    let path = crate::data_dir().join("kwin").join(format!("{SCRIPT}.js"));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, script()).map_err(|e| e.to_string())?;
    let k = kwin(conn).map_err(|e| e.to_string())?;
    let mut stale = vec![SCRIPT.to_string()];
    if let Ok(before) = std::fs::read_to_string(loaded_file()) {
        let before = before.trim().to_string();
        let gone = before.rsplit_once('-').and_then(|(_, pid)| pid.parse::<u32>().ok()).is_none_or(|pid| !crate::computer::here(format!("/proc/{pid}")).exists());
        if gone {
            stale.push(before);
        }
    }
    for name in &stale {
        let loaded: bool = k.call("isScriptLoaded", &(name.as_str(),)).map_err(|e| e.to_string())?;
        if loaded {
            let _: bool = k.call("unloadScript", &(name.as_str(),)).map_err(|e| e.to_string())?;
        }
    }
    let name = script_name();
    let id: i32 = k.call("loadScript", &(path.to_string_lossy().as_ref(), name.as_str())).map_err(|e| e.to_string())?;
    if id < 0 {
        return Err("KWin didn't load the script".into());
    }
    // (run by its own object, as KWin's own tools run a script: a script started by
    // Scripting.start, which looks through every installed script first, didn't stay
    // loaded on Plasma 6.7)
    let run = zbus::blocking::Proxy::new(conn, "org.kde.KWin", format!("/Scripting/Script{id}"), "org.kde.kwin.Script").map_err(|e| e.to_string())?;
    let () = run.call("run", &()).map_err(|e| e.to_string())?;
    LOADED.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
    let _ = std::fs::write(loaded_file(), &name);
    Ok(())
}

/// This Ectomatter's script heard from once it's loaded, or loaded again (a few
/// times at most, said in the log).
/// (KWin's own word on whether a script is loaded can't be taken: on Plasma 6.7 a
/// script that runs on is often no longer found by its name. What says it runs is
/// what it tells: as it starts, it says which window is in front.)
pub fn keep(conn: &zbus::blocking::Connection) {
    use std::sync::atomic::Ordering;
    static TRIES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let (heard, loaded) = (HEARD.load(Ordering::Relaxed), LOADED.load(Ordering::Relaxed));
    if heard >= loaded || now_ms().saturating_sub(loaded) < 3000 || TRIES.fetch_add(1, Ordering::Relaxed) >= 3 {
        return;
    }
    if let Ok(k) = kwin(conn) {
        let _: zbus::Result<bool> = k.call("unloadScript", &(script_name().as_str(),));
    }
    match load(conn) {
        Ok(()) => eprintln!("ectomatter: KWin's script hadn't started telling; it's loaded again"),
        Err(e) => eprintln!("ectomatter: KWin's script hadn't started telling, and won't load again: {e}"),
    }
}

/// This Ectomatter's script taken out of KWin, as Ectomatter quits.
pub fn stop(conn: &zbus::blocking::Connection) {
    let name = script_name();
    if let Ok(k) = kwin(conn) {
        let _: zbus::Result<bool> = k.call("unloadScript", &(name.as_str(),));
    }
    if std::fs::read_to_string(loaded_file()).is_ok_and(|n| n.trim() == name) {
        let _ = std::fs::remove_file(loaded_file());
    }
}

/// An app seen in front: as profiles are tied to it, and its name.
#[derive(Clone, Debug, PartialEq)]
pub struct App {
    pub id: String,
    pub name: String,
}

/// The apps seen in front, newest first, kept in Ectomatter's own state.
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

    /// An app come to the front, first now, by the id it goes by (a game's
    /// window by its game's: see games.rs), named by `name` if it's new; true
    /// if the list changed.
    pub fn saw_as(&mut self, f: &Front, id: &str, name: impl FnOnce() -> String) -> bool {
        let id = id.to_string();
        if id.is_empty() || f.is_ectomatter() {
            return false;
        }
        if self.list.first().is_some_and(|a| a.id == id) {
            return false;
        }
        let app = self.list.iter().position(|a| a.id == id).map(|i| self.list.remove(i)).unwrap_or_else(|| App { name: name(), id });
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

/// Where desktop files and icons are kept: the person's own, the system's, and Flatpak's.
fn share_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local/share"));
        dirs.push(home.join(".local/share/flatpak/exports/share"));
    }
    let data = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(data.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    dirs
}

/// An app's desktop file, as text, where one is.
fn desktop_file(id: &str) -> Option<String> {
    let file = format!("{id}.desktop");
    share_dirs().into_iter().find_map(|d| std::fs::read_to_string(d.join("applications").join(&file)).ok())
}

/// An app's name, as its desktop file has it (`Name=`, in its Desktop Entry),
/// looked for where desktop files are kept; else its class with a capital.
pub fn name_of(id: &str, class: &str) -> String {
    if let Some(name) = desktop_file(id).and_then(|t| desktop_entry(&t, "Name")) {
        return name;
    }
    let c = if class.is_empty() { id } else { class };
    let mut chars = c.chars();
    chars.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + chars.as_str())
}

/// An app's icon, as its desktop file names it (`Icon=`): a picture's own path,
/// or a name looked for in the icon themes, largest first, then the old pixmaps.
pub fn icon_of(id: &str) -> Option<PathBuf> {
    let icon = desktop_entry(&desktop_file(id)?, "Icon")?;
    let p = PathBuf::from(&icon);
    if p.is_absolute() {
        return p.is_file().then_some(p);
    }
    let dirs = share_dirs();
    for theme in ["hicolor", "breeze", "breeze-dark", "Adwaita"] {
        for size in ["scalable", "256x256", "128x128", "96x96", "64x64", "48x48", "32x32"] {
            for d in &dirs {
                for ext in ["svg", "png"] {
                    let f = d.join("icons").join(theme).join(size).join("apps").join(format!("{icon}.{ext}"));
                    if f.is_file() {
                        return Some(f);
                    }
                }
            }
        }
    }
    dirs.iter().flat_map(|d| ["png", "svg"].map(|e| d.join("pixmaps").join(format!("{icon}.{e}")))).chain(["png", "svg"].map(|e| PathBuf::from(format!("/usr/share/pixmaps/{icon}.{e}")))).find(|f| f.is_file())
}

/// A key's value in a desktop file's Desktop Entry, as plainly written (not a translation's).
fn desktop_entry(text: &str, key: &str) -> Option<String> {
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if in_entry {
            if let Some(n) = line.strip_prefix(key).and_then(|r| r.strip_prefix('=')) {
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
    /// Ectomatter's own windows are no app a profile follows; a desktop file's
    /// name is its Desktop Entry's own.
    #[test]
    fn the_app_in_front_is_named_as_its_desktop_file_names_it() {
        let f = Front { class: "konsole".into(), title: "~ : bash".into(), desktop: "org.kde.konsole".into(), ..Default::default() };
        assert_eq!(f.app(), "org.kde.konsole");
        assert_eq!(Front { class: "Steam".into(), ..Default::default() }.app(), "steam");
        assert!(Front { class: "ectomatter".into(), ..Default::default() }.is_ectomatter());
        let text = "[Desktop Entry]\nName=Konsole\nName[de]=Konsole DE\nExec=konsole\n[Desktop Action new]\nName=New Window\n";
        assert_eq!(desktop_entry(text, "Name").as_deref(), Some("Konsole"));
        assert_eq!(desktop_entry("[Other]\nName=No\n", "Name"), None);
        assert_eq!(desktop_entry("[Desktop Entry]\nIcon=org.kde.konsole\nName=Konsole\n", "Icon").as_deref(), Some("org.kde.konsole"));
        assert!(script().contains("workspace.windowActivated.connect(tell)"));
        assert!(script().contains("w.pid || 0"), "the script says which process a window is from");
        assert!(script().contains("workspace.windowAdded.connect(opened)"), "and tells of each window as it opens");
    }
}
