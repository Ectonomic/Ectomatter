//! Ectomatter in the system tray, as the web app had it: Open Ectomatter, each
//! device (grayed out while it is not connected), and Quit. A click on the
//! icon opens the window; closing the window leaves Ectomatter in the tray.

use ksni::blocking::TrayMethods;
use smithay_client_toolkit::reexports::calloop::channel::Sender;

#[derive(Clone, Debug)]
pub enum TrayAsk {
    Open,
    Device(String),
    Quit,
}

struct Tray {
    tell: Sender<TrayAsk>,
    /// the devices, as they sit on Home's desk: each one's place, its name,
    /// and whether it is plugged in
    devices: Vec<(String, String, bool)>,
    icon: Vec<ksni::Icon>,
}

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
        items.push(StandardItem { label: "Quit Ectomatter".into(), activate: Box::new(|t: &mut Self| { let _ = t.tell.send(TrayAsk::Quit); }), ..Default::default() }.into());
        items
    }
}

/// The app's icon as the tray takes it: ARGB, straight alpha, big-endian.
fn icon() -> Vec<ksni::Icon> {
    use resvg::tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};
    let Ok(big) = Pixmap::decode_png(include_bytes!("../ui/icon.png")) else { return Vec::new() };
    // the sizes a panel asks for, not the 512 the file holds
    [22u32, 32, 48, 64]
        .iter()
        .filter_map(|&n| {
            let mut pix = Pixmap::new(n, n)?;
            let k = n as f32 / big.width() as f32;
            let paint = PixmapPaint { quality: FilterQuality::Bicubic, ..Default::default() };
            pix.draw_pixmap(0, 0, big.as_ref(), &paint, Transform::from_scale(k, k), None);
            let data = pix
                .pixels()
                .iter()
                .flat_map(|p| {
                    let c = p.demultiply();
                    [c.alpha(), c.red(), c.green(), c.blue()]
                })
                .collect();
            Some(ksni::Icon { width: n as i32, height: n as i32, data })
        })
        .collect()
}

pub struct Handle(ksni::blocking::Handle<Tray>);

impl Handle {
    /// The devices, as they sit on Home's desk, each with whether it is there, for their entries.
    pub fn devices(&self, devices: Vec<(String, String, bool)>) {
        self.0.update(|t| t.devices = devices);
    }
}

pub fn start(tell: Sender<TrayAsk>) -> Option<Handle> {
    match (Tray { tell, devices: Vec::new(), icon: icon() }).spawn() {
        Ok(h) => Some(Handle(h)),
        Err(e) => {
            eprintln!("ectomatter: no tray: {e}");
            None
        }
    }
}

/// A notice from Ectomatter on the desktop, as other programs give one (the
/// desktop's notification service), sent on a thread of its own so nothing
/// waits for it. For what the person needs to know while the window may be
/// shut: what Ectomatter put back after it stopped short.
pub fn notify(title: &str, body: &str) {
    let (title, body) = (title.to_string(), body.to_string());
    let _ = std::thread::Builder::new().name("notice".into()).spawn(move || {
        let Ok(conn) = zbus::blocking::Connection::session() else { return };
        let Ok(n) = zbus::blocking::Proxy::new(&conn, "org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications") else { return };
        let hints: std::collections::HashMap<&str, zbus::zvariant::Value> = std::collections::HashMap::new();
        let r: zbus::Result<u32> = n.call("Notify", &("Ectomatter", 0u32, "ectomatter", title.as_str(), body.as_str(), Vec::<&str>::new(), hints, -1i32));
        match r {
            Ok(id) => eprintln!("ectomatter: told the desktop: {title} (notice {id})"),
            Err(e) => eprintln!("ectomatter: couldn’t tell the desktop {title}: {e}"),
        }
    });
}
