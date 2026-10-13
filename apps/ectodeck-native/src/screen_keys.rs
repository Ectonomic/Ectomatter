//! The keys given a screen job (see [crate::z10::screen_job]) send key codes the USB standard
//! reserves, which Linux never turns into key presses. So Ectodeck hears them in the keyboard's
//! own reports, read raw beside the computer, which still gets every key as before: the six-key
//! report on the keyboard's first interface and the ten-key report (number 0x10) on its third.
//! Nothing is ever sent to the keyboard here.

use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use crate::z10::{NEXT_LAYOUT, PLAY_CODES};

/// Hear the keyboard's screen keys, on a thread for each of its two key reports, for as long
/// as Ectodeck runs: `tell` hears each code as its key goes down (held down, once).
pub fn start(tell: impl Fn(u32) + Send + Clone + 'static) {
    for iface in [0u8, 2] {
        let tell = tell.clone();
        std::thread::spawn(move || loop {
            if let Some(mut f) = node(iface).and_then(|p| std::fs::File::open(p).ok()) {
                let mut down: Vec<u32> = Vec::new();
                let mut buf = [0u8; 64];
                while let Ok(n) = f.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let now = codes(iface, &buf[..n]);
                    for c in pressed(&down, &now) {
                        tell(c);
                    }
                    down = now;
                }
            }
            // (unplugged, or not there yet: looked for again)
            std::thread::sleep(Duration::from_secs(2));
        });
    }
}

/// The screen keys down in one report from the keyboard's interface `iface`: its boot report
/// (the modifiers, a byte kept empty, then six keys) or its report 0x10 (the same after the
/// report's number, with ten keys).
fn codes(iface: u8, r: &[u8]) -> Vec<u32> {
    let keys = match iface {
        0 if r.len() >= 8 => &r[2..8],
        2 if r.len() >= 13 && r[0] == 0x10 => &r[3..13],
        _ => return Vec::new(),
    };
    keys.iter().map(|k| *k as u32).filter(|k| (NEXT_LAYOUT..=*PLAY_CODES.end()).contains(k)).collect()
}

/// The keys in `now` that weren't down in the report before, `down`.
fn pressed(down: &[u32], now: &[u32]) -> Vec<u32> {
    now.iter().copied().filter(|c| !down.contains(c)).collect()
}

/// The keyboard's raw device for its interface `iface`, found by the Z10's ids.
fn node(iface: u8) -> Option<PathBuf> {
    let end = format!("/input{iface}");
    std::fs::read_dir("/sys/class/hidraw").ok()?.flatten().find_map(|e| {
        let ev = std::fs::read_to_string(e.path().join("device/uevent")).ok()?;
        let ours = ev.lines().any(|l| l == "HID_ID=0003:00003842:00002611") && ev.lines().any(|l| l.starts_with("HID_PHYS=") && l.ends_with(&end));
        ours.then(|| PathBuf::from("/dev").join(e.file_name()))
    })
}

/// The code a key plays playlist `id` with: the playlist's own, if it has one; else one no key
/// sends (`sent`), first one no playlist has, then one a deleted playlist had, then one whose
/// playlist no key plays; None while every code is some key's. `keyed` is the playlist each
/// code plays, by its id, and `exists` says whether a playlist still does.
pub fn code_for(id: &str, keyed: &BTreeMap<u32, String>, sent: &HashSet<u32>, exists: impl Fn(&str) -> bool) -> Option<u32> {
    if let Some((c, _)) = keyed.iter().find(|(_, p)| *p == id) {
        return Some(*c);
    }
    let free: Vec<u32> = PLAY_CODES.filter(|c| !sent.contains(c)).collect();
    free.iter()
        .find(|c| !keyed.contains_key(c))
        .or_else(|| free.iter().find(|c| keyed.get(c).is_some_and(|p| !exists(p))))
        .or(free.first())
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screen_key_is_heard_in_either_report_once_as_it_goes_down() {
        // the boot report: Ctrl held, A and the next layout's code down
        assert_eq!(codes(0, &[0x01, 0, 0x04, 0xA5, 0, 0, 0, 0]), vec![0xA5]);
        // report 0x10, ten keys: a playlist's code among letters
        let mut r = vec![0x10, 0, 0, 0x04, 0x05];
        r.extend([0xAB, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(codes(2, &r), vec![0xAB]);
        // another report on that interface, or one too short, has no keys
        assert!(codes(2, &[0x11, 0xA5, 0xA5, 0xA5, 0xA5, 0xA5, 0xA5, 0xA5, 0xA5, 0xA5, 0xA5, 0xA5, 0xA5]).is_empty());
        assert!(codes(0, &[0, 0, 0xA5]).is_empty());
        // codes past the reserved run are keys for the computer, not the screen
        assert!(codes(0, &[0, 0, 0xB0, 0xA4, 0, 0, 0, 0]).is_empty());
        // held down, a key is heard once; let go and pressed again, again
        assert_eq!(pressed(&[], &[0xA5]), vec![0xA5]);
        assert!(pressed(&[0xA5], &[0xA5]).is_empty());
        assert_eq!(pressed(&[0xA5], &[0xA5, 0xA7]), vec![0xA7]);
        assert_eq!(pressed(&[], &[0xA5]), vec![0xA5]);
    }

    #[test]
    fn each_playlist_keeps_its_code_and_a_new_one_takes_a_spare() {
        let keyed: BTreeMap<u32, String> = [(0xA9, "games".to_string()), (0xAA, "gone".to_string()), (0xAB, "quiet".to_string())].into();
        let exists = |p: &str| p != "gone";
        let sent: HashSet<u32> = [0xA9, 0xAA].into();
        // a playlist with a code keeps it
        assert_eq!(code_for("quiet", &keyed, &sent, exists), Some(0xAB));
        // a new one: the first code no playlist has and no key sends
        assert_eq!(code_for("night", &keyed, &sent, exists), Some(0xAC));
        // every code had: one a deleted playlist had (no key sending it), then one whose
        // playlist no key plays
        let full: BTreeMap<u32, String> = PLAY_CODES.map(|c| (c, if c == 0xAD { "gone".into() } else { format!("p{c}") })).collect();
        assert_eq!(code_for("night", &full, &sent, exists), Some(0xAD));
        let living: BTreeMap<u32, String> = PLAY_CODES.map(|c| (c, format!("p{c}"))).collect();
        assert_eq!(code_for("night", &living, &sent, |_| true), Some(0xAB));
        // and none while every code is some key's
        let all: HashSet<u32> = PLAY_CODES.collect();
        assert_eq!(code_for("night", &living, &all, |_| true), None);
    }
}
