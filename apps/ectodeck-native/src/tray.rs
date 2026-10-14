//! Ectodeck in the system tray, as the web app had it: Open Ectodeck, each
//! device (grayed out while it is not connected), and Quit. A click on the
//! icon opens the window; closing the window leaves Ectodeck in the tray.

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
    connected: bool,
    icon: Vec<ksni::Icon>,
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "com.ectonomic.ectodeck".into()
    }
    fn title(&self) -> String {
        "Ectodeck".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icon.clone()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: "Ectodeck".into(), ..Default::default() }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tell.send(TrayAsk::Open);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let label = if self.connected { "EVGA Z10 RGB".to_string() } else { "EVGA Z10 RGB (not connected)".to_string() };
        vec![
            StandardItem { label: "Open Ectodeck".into(), activate: Box::new(|t: &mut Self| { let _ = t.tell.send(TrayAsk::Open); }), ..Default::default() }.into(),
            StandardItem {
                label,
                enabled: self.connected,
                activate: Box::new(|t: &mut Self| { let _ = t.tell.send(TrayAsk::Device("z10".into())); }),
                ..Default::default()
            }
            .into(),
            StandardItem { label: "Quit Ectodeck".into(), activate: Box::new(|t: &mut Self| { let _ = t.tell.send(TrayAsk::Quit); }), ..Default::default() }.into(),
        ]
    }
}

/// The app's icon as the tray takes it: ARGB, straight alpha, big-endian.
fn icon() -> Vec<ksni::Icon> {
    use resvg::tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};
    let Ok(big) = Pixmap::decode_png(include_bytes!("../../ectodeck/src-tauri/icons/icon.png")) else { return Vec::new() };
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
    /// Whether the keyboard is there, for its entry.
    pub fn connected(&self, on: bool) {
        self.0.update(|t| t.connected = on);
    }
}

pub fn start(tell: Sender<TrayAsk>) -> Option<Handle> {
    match (Tray { tell, connected: false, icon: icon() }).spawn() {
        Ok(h) => Some(Handle(h)),
        Err(e) => {
            eprintln!("ectodeck-native: no tray: {e}");
            None
        }
    }
}
