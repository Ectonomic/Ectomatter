//! KDE's own account of each screen, its output device protocol
//! (kde-output-device-v2, Plasma's, as kscreen reads it; the definition is
//! in protocols/): the screen by its name, whether HDR is on, and the SDR
//! white its user set, which an idle dim does not change. The definition's
//! copy marks a mode's `removed` as ending the mode, as KWin treats it (it
//! may give the mode's number to a new one at once), so a connection that
//! lives through a change of modes doesn't fail. And KDE's output management
//! (kde-output-management-v2), with which monitors.rs changes them.

#[allow(non_upper_case_globals, non_camel_case_types, non_snake_case, unused_imports, unused_mut, unused_variables, dead_code, missing_docs, clippy::all)]
pub mod client {
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/kde-output-device-v2.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/kde-output-device-v2.xml");
}

/// How the screens are changed, for the Monitor Configurator's changes (monitors.rs).
#[allow(non_upper_case_globals, non_camel_case_types, non_snake_case, unused_imports, unused_mut, unused_variables, dead_code, missing_docs, clippy::all)]
pub mod management {
    use wayland_client;
    use wayland_client::protocol::*;
    pub use super::client::{kde_output_device_mode_v2, kde_output_device_v2};
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        use super::super::client::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/kde-output-management-v2.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/kde-output-management-v2.xml");
}

use client::kde_output_device_v2::Event;

/// What KDE says of a screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Device {
    pub name: String,
    pub hdr: bool,
    /// the SDR white its user set, in nits (with HDR on)
    pub sdr: f64,
    /// how far it is dimmed while the desktop is idle, 1 not at all (0 unsaid)
    pub dimming: f64,
}

impl Device {
    /// One of its events, into what it says; true at its `done`, when what it says is whole.
    pub fn take(&mut self, e: Event) -> bool {
        match e {
            Event::Name { name } => self.name = name,
            Event::HighDynamicRange { hdr_enabled } => self.hdr = hdr_enabled == 1,
            Event::SdrBrightness { sdr_brightness } => self.sdr = sdr_brightness as f64,
            Event::Dimming { multiplier } => self.dimming = multiplier as f64 / 10000.0,
            Event::Done => return true,
            _ => {}
        }
        false
    }
}

impl Device {
    /// Whether the desktop is idle and the screen dimmed for it.
    pub fn dimmed(&self) -> bool {
        self.dimming > 0.0 && self.dimming < 1.0
    }
}

/// The white Ectomatter describes its picture for on a screen KWin reports
/// `reported` nits of white for. KWin scales a picture by the screen's white
/// over the picture's own, so a picture described for the screen's white
/// before an idle dim dims with everything else on it, and a dim can never
/// leave its light too strong. With HDR on, the white is the SDR white the
/// screen's user set (and so the picture dims with the screen's brightness
/// too). Otherwise it is the white KWin reports, but while the screen is
/// dimmed the one it reported last undimmed (`undimmed`), if that is known
/// (KWin's white falls with the dim, though not in proportion to it); and
/// without KDE's word, as KWin reports it.
pub fn white(reported: f64, d: Option<&Device>, undimmed: Option<f64>) -> f64 {
    match d {
        Some(d) if d.hdr && d.sdr > 0.0 => d.sdr,
        Some(d) if d.dimmed() => undimmed.unwrap_or(reported),
        _ => reported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With HDR on, the white is the one set, however KWin reports it (as it
    /// did while the desktop was idle: 80 nits for the 254 set); an SDR
    /// screen's, the one it reported undimmed while it is dimmed (it reported
    /// 23 for its 64); and with nothing from KDE, as KWin reports it.
    #[test]
    fn the_white_is_the_one_set() {
        let hdr = Device { name: "DP-4".into(), hdr: true, sdr: 254.0, dimming: 0.3 };
        assert_eq!(white(80.0, Some(&hdr), None), 254.0, "dimmed");
        assert_eq!(white(254.0, Some(&Device { dimming: 1.0, ..hdr.clone() }), None), 254.0);
        let sdr = Device { name: "DP-3".into(), hdr: false, sdr: 0.0, dimming: 1.0 };
        assert_eq!(white(64.0, Some(&sdr), None), 64.0, "an SDR screen");
        let dim = Device { dimming: 0.3, ..sdr };
        assert_eq!(white(23.0, Some(&dim), Some(64.0)), 64.0, "an SDR screen dimmed");
        assert_eq!(white(23.0, Some(&dim), None), 23.0, "dimmed before its white was known");
        assert_eq!(white(203.0, None, None), 203.0, "no word from KDE");
    }
}
