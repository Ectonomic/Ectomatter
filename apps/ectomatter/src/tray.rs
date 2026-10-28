//! Ectomatter in the system tray, as the web app had it: Open Ectomatter, each
//! device (grayed out while it is not connected), the Scene menu (No scene,
//! the scenes that come with Ectomatter, the person's own, the one on chosen),
//! and Quit. A click on the icon opens the window; closing the window leaves
//! Ectomatter in the tray.

#[cfg(target_os = "linux")]
use ksni::blocking::TrayMethods;
use crate::shell::Sender;

#[derive(Clone, Debug)]
pub enum TrayAsk {
    Open,
    Device(String),
    /// a scene chosen in the Scene menu, or No scene
    Scene(Option<String>),
    Quit,
}

#[cfg(target_os = "linux")]
struct Tray {
    tell: Sender<TrayAsk>,
    /// the devices, as they sit on Home's desk: each one's place, its name,
    /// and whether it is plugged in
    devices: Vec<(String, String, bool)>,
    /// the scenes (each one's id and name, the two that come with Ectomatter
    /// first) and the one on
    scenes: Vec<(String, String)>,
    scene: Option<String>,
    icon: Vec<ksni::Icon>,
    /// since when the desktop's tray hasn't had the icon: none while it has
    away: Away,
}

/// Since when the desktop's tray hasn't had Ectomatter's icon, shared with the tray's thread.
#[cfg(target_os = "linux")]
type Away = std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>;

#[cfg(target_os = "linux")]
impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "com.ectonomic.ectomatter".into()
    }
    fn title(&self) -> String {
        "Ectomatter".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icon.clone()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: "Ectomatter".into(), ..Default::default() }
    }
    // the desktop's tray not there (not yet, at login, or gone): the icon goes in when it's back
    fn watcher_offline(&self, _: ksni::OfflineReason) -> bool {
        if let Ok(mut a) = self.away.lock() {
            a.get_or_insert_with(std::time::Instant::now);
        }
        true
    }
    fn watcher_online(&self) {
        if let Ok(mut a) = self.away.lock() {
            *a = None;
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tell.send(TrayAsk::Open);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let mut items: Vec<ksni::MenuItem<Self>> = vec![StandardItem { label: "Open Ectomatter".into(), activate: Box::new(|t: &mut Self| { let _ = t.tell.send(TrayAsk::Open); }), ..Default::default() }.into()];
        for (place, name, on) in &self.devices {
            let place = place.clone();
            items.push(
                StandardItem {
                    label: if *on { name.clone() } else { format!("{name} (not connected)") },
                    enabled: *on,
                    activate: Box::new(move |t: &mut Self| {
                        let _ = t.tell.send(TrayAsk::Device(place.clone()));
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        // the Scene menu: a group of choices for No scene, the built-in scenes and the person's own
        if !self.scenes.is_empty() {
            use ksni::menu::{RadioGroup, RadioItem, SubMenu};
            let group = |ids: Vec<Option<String>>, names: Vec<String>, on: &Option<String>| -> ksni::MenuItem<Self> {
                let selected = ids.iter().position(|i| i == on).unwrap_or(usize::MAX);
                RadioGroup {
                    selected,
                    select: Box::new(move |t: &mut Self, i| {
                        if let Some(id) = ids.get(i) {
                            let _ = t.tell.send(TrayAsk::Scene(id.clone()));
                        }
                    }),
                    options: names.into_iter().map(|n| RadioItem { label: n, ..Default::default() }).collect(),
                }
                .into()
            };
            let (built, own): (Vec<_>, Vec<_>) = self.scenes.iter().cloned().partition(|(id, _)| id == "wave" || id == "shift");
            let mut sub = vec![group(vec![None], vec!["No scene".into()], &self.scene)];
            for g in [built, own] {
                if !g.is_empty() {
                    sub.push(ksni::MenuItem::Separator);
                    let (ids, names): (Vec<Option<String>>, Vec<String>) = g.into_iter().map(|(id, n)| (Some(id), n)).unzip();
                    sub.push(group(ids, names, &self.scene));
                }
            }
            items.push(ksni::MenuItem::Separator);
            items.push(SubMenu { label: "Scene".into(), submenu: sub, ..Default::default() }.into());
            items.push(ksni::MenuItem::Separator);
        }
        items.push(StandardItem { label: "Quit Ectomatter".into(), activate: Box::new(|t: &mut Self| { let _ = t.tell.send(TrayAsk::Quit); }), ..Default::default() }.into());
        items
    }
}

#[cfg(target_os = "linux")]
/// The app's icon as the tray takes it: ARGB, straight alpha, big-endian.
/// Each size a panel asks for is drawn at that size (tools/mark/build_icons.py),
/// since the ring's line gets heavier as the mark gets smaller; shrinking one big
/// picture would leave it too thin.
fn icon() -> Vec<ksni::Icon> {
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
            Some(ksni::Icon { width: pix.width() as i32, height: pix.height() as i32, data })
        })
        .collect()
}

#[cfg(target_os = "linux")]
pub struct Handle(ksni::blocking::Handle<Tray>, Away);

#[cfg(target_os = "linux")]
impl Handle {
    /// The devices, as they sit on Home's desk, each with whether it is there, for their entries.
    pub fn devices(&self, devices: Vec<(String, String, bool)>) {
        self.0.update(|t| t.devices = devices);
    }

    /// How long the desktop's tray has been without the icon: none while it has it.
    pub fn away(&self) -> Option<std::time::Duration> {
        self.1.lock().ok().and_then(|a| a.map(|t| t.elapsed()))
    }

    /// The scenes, for the Scene menu, and the one on.
    pub fn scenes(&self, scenes: Vec<(String, String)>, on: Option<String>) {
        self.0.update(|t| {
            t.scenes = scenes;
            t.scene = on;
        });
    }
}

#[cfg(target_os = "linux")]
pub fn start(tell: Sender<TrayAsk>) -> Option<Handle> {
    let away = Away::default();
    // (Ectomatter can start before the desktop's tray, as at login: its icon goes in once
    // the tray is there, as the tray's library has it for a program that starts before
    // its desktop is ready)
    match (Tray { tell, devices: Vec::new(), scenes: Vec::new(), scene: None, icon: icon(), away: away.clone() }).assume_sni_available(true).spawn() {
        Ok(h) => Some(Handle(h, away)),
        Err(e) => {
            eprintln!("ectomatter: no tray: {e}");
            None
        }
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
        let Ok(conn) = zbus::blocking::Connection::session() else { return };
        let Ok(n) = zbus::blocking::Proxy::new(&conn, "org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications") else { return };
        // the icon by the name the desktop knows it by, and the desktop entry, so
        // the notice shows the mark and the desktop's notification settings list
        // Ectomatter as itself
        let mut hints: std::collections::HashMap<&str, zbus::zvariant::Value> = std::collections::HashMap::new();
        hints.insert("desktop-entry", zbus::zvariant::Value::from("com.ectonomic.ectomatter"));
        let r: zbus::Result<u32> = n.call("Notify", &("Ectomatter", 0u32, "com.ectonomic.ectomatter", title.as_str(), body.as_str(), Vec::<&str>::new(), hints, -1i32));
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
