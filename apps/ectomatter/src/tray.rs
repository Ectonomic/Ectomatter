//! Ectomatter in the system tray, as the web app had it: Open Ectomatter, each
//! device (grayed out while it is not connected), the Scene menu (No scene,
//! the scenes that come with Ectomatter, the person's own, the one on chosen),
//! and Quit. A click on the icon opens the window; closing the window leaves
//! Ectomatter in the tray.
//!
//! On Linux the icon is a StatusNotifierItem, as KDE's and every freedesktop
//! tray takes one (org.kde.StatusNotifierItem at /StatusNotifierItem,
//! registered with the desktop's org.kde.StatusNotifierWatcher, and again
//! each time a tray comes), and its menu a dbusmenu (com.canonical.dbusmenu
//! at /MenuBar), both on a connection of the tray's own, served by a thread
//! of its own. A press hands the activation token the tray gives with it
//! (ProvideXdgActivationToken, KDE's), so the window may come forward.

use crate::shell::Sender;

#[derive(Clone, Debug)]
pub enum TrayAsk {
    /// the window brought to the person, with the activation token the press gave
    Open(Option<String>),
    Device(String),
    /// a scene chosen in the Scene menu, or No scene
    Scene(Option<String>),
    Quit,
}

#[cfg(target_os = "linux")]
mod item {
    use super::TrayAsk;
    use crate::shell::Sender;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use zbus::object_server::SignalEmitter;
    use zbus::wire::{OwnedObjectPath, OwnedVariant, Variant};

    /// How long after a press a token may still come for it: a tray asks the
    /// compositor for one as it's pressed, and hands it over as it comes.
    const LATE_TOKEN: Duration = Duration::from_millis(250);

    /// What the tray shows and what a press on it does, shared by the tray's
    /// thread, which answers the desktop, and the window's, which changes it.
    pub struct State {
        pub tell: Sender<TrayAsk>,
        /// the devices, as they sit on Home's desk: each one's place, its name,
        /// and whether it is plugged in
        pub devices: Vec<(String, String, bool)>,
        /// the scenes (each one's id and name, the two that come with Ectomatter
        /// first) and the one on
        pub scenes: Vec<(String, String)>,
        pub scene: Option<String>,
        /// the icon at each size, as the tray takes it: width, height, ARGB
        pub icon: Vec<(i32, i32, Vec<u8>)>,
        /// the menu's revision, one more each time what it shows changes
        pub revision: u32,
        /// the activation token the tray handed for the press coming, and when a press
        /// came before its token did
        token: Option<String>,
        pressed: Option<Instant>,
        /// since when the desktop's tray hasn't had the icon: none while it has
        pub away: Option<Instant>,
    }

    pub type Shared = Arc<Mutex<State>>;

    impl State {
        pub fn new(tell: Sender<TrayAsk>, icon: Vec<(i32, i32, Vec<u8>)>) -> State {
            State { tell, devices: Vec::new(), scenes: Vec::new(), scene: None, icon, revision: 1, token: None, pressed: None, away: None }
        }

        /// The window asked to come forward, with the press's token if it has come.
        fn open(&mut self) {
            let token = self.token.take();
            self.pressed = token.is_none().then(Instant::now);
            let _ = self.tell.send(TrayAsk::Open(token));
        }
    }

    /// What a menu entry does.
    #[derive(Clone, Debug, PartialEq)]
    pub enum Act {
        None,
        Open,
        Device(String),
        Scene(Option<String>),
        Quit,
    }

    /// A menu entry: its id (its place in the menu as laid out), what it shows and does,
    /// and what's under it.
    #[derive(Clone, Debug)]
    pub struct Entry {
        pub id: i32,
        pub label: String,
        pub separator: bool,
        pub enabled: bool,
        /// a choice among others, and whether it's the one chosen
        pub radio: Option<bool>,
        pub act: Act,
        pub children: Vec<Entry>,
    }

    /// The menu as `s` has it: Open Ectomatter, each device, the Scene menu, Quit.
    pub fn menu(s: &State) -> Entry {
        let mut id = 0;
        let mut next = |label: &str, act: Act| {
            id += 1;
            Entry { id, label: label.into(), separator: label.is_empty() && act == Act::None, enabled: true, radio: None, act, children: Vec::new() }
        };
        let mut items = vec![next("Open Ectomatter", Act::Open)];
        for (place, name, on) in &s.devices {
            let mut e = next(&if *on { name.clone() } else { format!("{name} (not connected)") }, Act::Device(place.clone()));
            e.enabled = *on;
            items.push(e);
        }
        // the Scene menu: a group of choices for No scene, the built-in scenes and the person's own
        if !s.scenes.is_empty() {
            items.push(next("", Act::None));
            let mut scene = next("Scene", Act::None);
            let mut none = next("No scene", Act::Scene(None));
            none.radio = Some(s.scene.is_none());
            scene.children.push(none);
            let (built, own): (Vec<_>, Vec<_>) = s.scenes.iter().cloned().partition(|(id, _)| id == "wave" || id == "shift");
            for g in [built, own] {
                if !g.is_empty() {
                    scene.children.push(next("", Act::None));
                    for (sid, name) in g {
                        let on = s.scene.as_deref() == Some(sid.as_str());
                        let mut e = next(&name, Act::Scene(Some(sid)));
                        e.radio = Some(on);
                        scene.children.push(e);
                    }
                }
            }
            items.push(scene);
            items.push(next("", Act::None));
        }
        items.push(next("Quit Ectomatter", Act::Quit));
        Entry { id: 0, label: String::new(), separator: false, enabled: true, radio: None, act: Act::None, children: items }
    }

    fn find(e: &Entry, id: i32) -> Option<&Entry> {
        if e.id == id {
            return Some(e);
        }
        e.children.iter().find_map(|c| find(c, id))
    }

    /// An entry's properties as dbusmenu names them, those asked for (all, where none are).
    fn props(e: &Entry, names: &[String]) -> HashMap<String, OwnedVariant> {
        let mut p: Vec<(&str, Variant)> = Vec::new();
        if e.separator {
            p.push(("type", Variant::from("separator")));
        } else if e.id != 0 {
            p.push(("label", Variant::from(e.label.clone())));
        }
        if !e.enabled {
            p.push(("enabled", Variant::from(false)));
        }
        if let Some(on) = e.radio {
            p.push(("toggle-type", Variant::from("radio")));
            p.push(("toggle-state", Variant::from(i32::from(on))));
        }
        if !e.children.is_empty() || e.id == 0 {
            p.push(("children-display", Variant::from("submenu")));
        }
        p.into_iter().filter(|(k, _)| names.is_empty() || names.iter().any(|n| n == k)).filter_map(|(k, v)| Some((k.to_string(), OwnedVariant::try_from(v).ok()?))).collect()
    }

    /// An entry laid out as dbusmenu's GetLayout gives it: (ia{sv}av), its children
    /// `depth` deep (all of them at -1).
    #[derive(serde::Serialize, zbus::wire::Type, zbus::wire::Variant)]
    pub struct Layout {
        id: i32,
        props: HashMap<String, OwnedVariant>,
        children: Vec<OwnedVariant>,
    }

    fn layout(e: &Entry, depth: i32, names: &[String]) -> Layout {
        let children = if depth == 0 { Vec::new() } else { e.children.iter().filter_map(|c| OwnedVariant::try_from(Variant::from(layout(c, depth - 1, names))).ok()).collect() };
        Layout { id: e.id, props: props(e, names), children }
    }

    /// The icon, as org.kde.StatusNotifierItem.
    pub struct Item(pub Shared);

    #[zbus::interface(name = "org.kde.StatusNotifierItem")]
    impl Item {
        #[zbus(property)]
        fn category(&self) -> String {
            "ApplicationStatus".into()
        }
        #[zbus(property)]
        fn id(&self) -> String {
            "com.ectonomic.ectomatter".into()
        }
        #[zbus(property)]
        fn title(&self) -> String {
            "Ectomatter".into()
        }
        #[zbus(property)]
        fn status(&self) -> String {
            "Active".into()
        }
        #[zbus(property)]
        fn window_id(&self) -> i32 {
            0
        }
        #[zbus(property)]
        fn icon_theme_path(&self) -> String {
            String::new()
        }
        #[zbus(property)]
        fn icon_name(&self) -> String {
            String::new()
        }
        #[zbus(property)]
        fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
            self.0.lock().map(|s| s.icon.clone()).unwrap_or_default()
        }
        #[zbus(property)]
        fn overlay_icon_name(&self) -> String {
            String::new()
        }
        #[zbus(property)]
        fn overlay_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
            Vec::new()
        }
        #[zbus(property)]
        fn attention_icon_name(&self) -> String {
            String::new()
        }
        #[zbus(property)]
        fn attention_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
            Vec::new()
        }
        #[zbus(property)]
        fn attention_movie_name(&self) -> String {
            String::new()
        }
        #[zbus(property)]
        fn tool_tip(&self) -> (String, Vec<(i32, i32, Vec<u8>)>, String, String) {
            (String::new(), Vec::new(), "Ectomatter".into(), String::new())
        }
        #[zbus(property)]
        fn item_is_menu(&self) -> bool {
            false
        }
        #[zbus(property)]
        fn menu(&self) -> OwnedObjectPath {
            OwnedObjectPath::try_from("/MenuBar").expect("the menu's path")
        }

        /// The icon pressed: the window comes forward.
        fn activate(&self, _x: i32, _y: i32) {
            if let Ok(mut s) = self.0.lock() {
                s.open();
            }
        }
        fn secondary_activate(&self, _x: i32, _y: i32) {}
        // (the desktop's tray shows the menu itself, from Menu)
        fn context_menu(&self, _x: i32, _y: i32) {}
        fn scroll(&self, _delta: i32, _orientation: String) {}

        /// The activation token for a press, which may come before the press or just after
        /// it: one that comes after brings the window forward with it then.
        fn provide_xdg_activation_token(&self, token: String) {
            let Ok(mut s) = self.0.lock() else { return };
            if s.pressed.take().is_some_and(|t| t.elapsed() < LATE_TOKEN) {
                let _ = s.tell.send(TrayAsk::Open(Some(token)));
            } else {
                s.token = Some(token);
            }
        }
    }

    /// The menu, as com.canonical.dbusmenu.
    pub struct Menu(pub Shared);

    #[zbus::interface(name = "com.canonical.dbusmenu")]
    impl Menu {
        #[zbus(property)]
        fn version(&self) -> u32 {
            3
        }
        #[zbus(property)]
        fn text_direction(&self) -> String {
            "ltr".into()
        }
        #[zbus(property)]
        fn status(&self) -> String {
            "normal".into()
        }
        #[zbus(property)]
        fn icon_theme_path(&self) -> Vec<String> {
            Vec::new()
        }

        fn get_layout(&self, parent_id: i32, recursion_depth: i32, property_names: Vec<String>) -> zbus::fdo::Result<(u32, Layout)> {
            let s = self.0.lock().map_err(|_| zbus::fdo::Error::Failed("the tray's state".into()))?;
            let m = menu(&s);
            let e = find(&m, parent_id).ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no menu entry {parent_id}")))?;
            Ok((s.revision, layout(e, recursion_depth, &property_names)))
        }

        fn get_group_properties(&self, ids: Vec<i32>, property_names: Vec<String>) -> Vec<(i32, HashMap<String, OwnedVariant>)> {
            let Ok(s) = self.0.lock() else { return Vec::new() };
            let m = menu(&s);
            ids.iter().filter_map(|&id| find(&m, id).map(|e| (id, props(e, &property_names)))).collect()
        }

        fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedVariant> {
            let s = self.0.lock().map_err(|_| zbus::fdo::Error::Failed("the tray's state".into()))?;
            let m = menu(&s);
            find(&m, id).and_then(|e| props(e, &[name.clone()]).remove(&name)).ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("no {name} on menu entry {id}")))
        }

        /// An entry pressed ("clicked"): what it does, done.
        fn event(&self, id: i32, event_id: String, _data: OwnedVariant, _timestamp: u32) {
            if event_id == "clicked" {
                self.clicked(id);
            }
        }

        fn event_group(&self, events: Vec<(i32, String, OwnedVariant, u32)>) -> Vec<i32> {
            for (id, event_id, ..) in &events {
                if event_id == "clicked" {
                    self.clicked(*id);
                }
            }
            Vec::new()
        }

        fn about_to_show(&self, _id: i32) -> bool {
            false
        }

        fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
            (Vec::new(), Vec::new())
        }

        #[zbus(signal)]
        pub async fn layout_updated(emitter: &SignalEmitter<'_>, revision: u32, parent: i32) -> zbus::Result<()>;
    }

    impl Menu {
        fn clicked(&self, id: i32) {
            let Ok(mut s) = self.0.lock() else { return };
            let act = find(&menu(&s), id).map(|e| e.act.clone()).unwrap_or(Act::None);
            match act {
                Act::Open => s.open(),
                Act::Device(place) => {
                    let _ = s.tell.send(TrayAsk::Device(place));
                }
                Act::Scene(id) => {
                    let _ = s.tell.send(TrayAsk::Scene(id));
                }
                Act::Quit => {
                    let _ = s.tell.send(TrayAsk::Quit);
                }
                Act::None => {}
            }
        }
    }
}

/// The desktop's tray service, which the icon is registered with.
#[cfg(target_os = "linux")]
const WATCHER: &str = "org.kde.StatusNotifierWatcher";

#[cfg(target_os = "linux")]
/// The app's icon as the tray takes it: ARGB, straight alpha, big-endian.
/// Each size a panel asks for is drawn at that size (tools/mark/build_icons.py),
/// since the ring's line gets heavier as the mark gets smaller; shrinking one big
/// picture would leave it too thin.
fn icon() -> Vec<(i32, i32, Vec<u8>)> {
    use resvg::tiny_skia::Pixmap;
    let sizes: [&[u8]; 4] = [
        include_bytes!("../ui/icons/ectomatter-22.png"),
        include_bytes!("../ui/icons/ectomatter-32.png"),
        include_bytes!("../ui/icons/ectomatter-48.png"),
        include_bytes!("../ui/icons/ectomatter-64.png"),
    ];
    sizes
        .iter()
        .filter_map(|png| {
            let pix = Pixmap::decode_png(png).ok()?;
            let data = pix
                .pixels()
                .iter()
                .flat_map(|p| {
                    let c = p.demultiply();
                    [c.alpha(), c.red(), c.green(), c.blue()]
                })
                .collect();
            Some((pix.width() as i32, pix.height() as i32, data))
        })
        .collect()
}

#[cfg(target_os = "linux")]
pub struct Handle {
    state: item::Shared,
    bus: crate::bus::Connection,
}

#[cfg(target_os = "linux")]
impl Handle {
    /// The devices, as they sit on Home's desk, each with whether it is there, for their entries.
    pub fn devices(&self, devices: Vec<(String, String, bool)>) {
        self.change(|s| {
            let changed = s.devices != devices;
            s.devices = devices;
            changed
        });
    }

    /// How long the desktop's tray has been without the icon: none while it has it.
    pub fn away(&self) -> Option<std::time::Duration> {
        self.state.lock().ok().and_then(|s| s.away.map(|t| t.elapsed()))
    }

    /// The scenes, for the Scene menu, and the one on.
    pub fn scenes(&self, scenes: Vec<(String, String)>, on: Option<String>) {
        self.change(|s| {
            let changed = s.scenes != scenes || s.scene != on;
            s.scenes = scenes;
            s.scene = on;
            changed
        });
    }

    /// What the menu shows changed (where `f` says it did): the tray told to lay it out again.
    fn change(&self, f: impl FnOnce(&mut item::State) -> bool) {
        let revision = {
            let Ok(mut s) = self.state.lock() else { return };
            if !f(&mut s) {
                return;
            }
            s.revision += 1;
            s.revision
        };
        let _ = zbus::block_on(self.bus.emit_signal(None::<&str>, "/MenuBar", "com.canonical.dbusmenu", "LayoutUpdated", &(revision, 0i32)));
    }
}

/// The icon registered with the desktop's tray under `name`: whether it took it.
#[cfg(target_os = "linux")]
async fn register(bus: &zbus::Connection, name: &str) -> bool {
    bus.call_method(Some(WATCHER), "/StatusNotifierWatcher", Some(WATCHER), "RegisterStatusNotifierItem", &(name,)).await.is_ok()
}

#[cfg(target_os = "linux")]
pub fn start(tell: Sender<TrayAsk>) -> Option<Handle> {
    let state: item::Shared = std::sync::Arc::new(std::sync::Mutex::new(item::State::new(tell, icon())));
    let (made, conn) = std::sync::mpsc::channel();
    let shared = state.clone();
    let spawned = std::thread::Builder::new().name("tray".into()).spawn(move || {
        zbus::block_on(async move {
            // (the name a tray's item takes on the bus, as KDE's own have it)
            let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
            let built = zbus::connection::Builder::session().name(name.as_str()).serve_at("/StatusNotifierItem", item::Item(shared.clone())).serve_at("/MenuBar", item::Menu(shared.clone())).build().await;
            let bus = match built {
                Ok(b) => b,
                Err(e) => {
                    let _ = made.send(Err(e.to_string()));
                    return;
                }
            };
            let _ = made.send(Ok(bus.clone()));
            // (Ectomatter can start before the desktop's tray, as at login: its icon goes in once
            // the tray is there, and again each time a tray comes)
            let there = register(&bus, &name).await;
            if let Ok(mut s) = shared.lock() {
                s.away = (!there).then(std::time::Instant::now);
            }
            let Ok(names) = zbus::Proxy::new(&bus, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus").await else { return };
            let Ok(mut owners) = names.receive_signal_with_args("NameOwnerChanged", &[(0, WATCHER)]).await else { return };
            while let Some(m) = crate::bus::next(&mut owners).await {
                let Ok((_, _, owner)) = m.body().deserialize::<(String, String, String)>() else { continue };
                let there = !owner.is_empty() && register(&bus, &name).await;
                if let Ok(mut s) = shared.lock() {
                    s.away = match (there, s.away) {
                        (true, _) => None,
                        (false, Some(t)) => Some(t),
                        (false, None) => Some(std::time::Instant::now()),
                    };
                }
            }
        })
    });
    if let Err(e) = spawned {
        eprintln!("ectomatter: no tray: {e}");
        return None;
    }
    match conn.recv() {
        Ok(Ok(bus)) => Some(Handle { state, bus }),
        Ok(Err(e)) => {
            eprintln!("ectomatter: no tray: {e}");
            None
        }
        Err(_) => None,
    }
}

#[cfg(target_os = "linux")]
/// A notice from Ectomatter on the desktop, as other programs give one (the
/// desktop's notification service), sent on a thread of its own so nothing
/// waits for it. For what the person needs to know while the window may be
/// shut: what Ectomatter put back after it stopped short.
pub fn notify(title: &str, body: &str) {
    let (title, body) = (title.to_string(), body.to_string());
    let _ = std::thread::Builder::new().name("notice".into()).spawn(move || {
        let Ok(conn) = crate::bus::session() else { return };
        // the icon by the name the desktop knows it by, and the desktop entry, so
        // the notice shows the mark and the desktop's notification settings list
        // Ectomatter as itself
        let mut hints: std::collections::HashMap<&str, zbus::wire::Variant> = std::collections::HashMap::new();
        hints.insert("desktop-entry", zbus::wire::Variant::from("com.ectonomic.ectomatter"));
        let r: zbus::Result<u32> = crate::bus::call(&conn, "org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications", "Notify", &("Ectomatter", 0u32, "com.ectonomic.ectomatter", title.as_str(), body.as_str(), Vec::<&str>::new(), hints, -1i32));
        match r {
            Ok(id) => eprintln!("ectomatter: told the desktop: {title} (notice {id})"),
            Err(e) => eprintln!("ectomatter: couldn’t tell the desktop {title}: {e}"),
        }
    });
}

// Elsewhere: no tray yet (closing the window quits), and no notice on the
// desktop (docs/deferred.md, stage 5).
#[cfg(not(target_os = "linux"))]
pub struct Handle;

#[cfg(not(target_os = "linux"))]
impl Handle {
    pub fn devices(&self, _: Vec<(String, String, bool)>) {}
    pub fn scenes(&self, _: Vec<(String, String)>, _: Option<String>) {}
    pub fn away(&self) -> Option<std::time::Duration> {
        None
    }
}

#[cfg(not(target_os = "linux"))]
pub fn start(_: Sender<TrayAsk>) -> Option<Handle> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn notify(title: &str, body: &str) {
    eprintln!("ectomatter: {title}: {body}");
}
