//! The codes the keys given a screen job send (see [crate::keyboard::screen_job]): codes the
//! USB standard reserves, which Linux never turns into key presses, so the keyboard's core
//! hears them in the keyboard's own reports and tells Ectomatter which host slot it was (see
//! cores/evga-z10). Here, the code each playlist and each of Ectomatter's own jobs keeps.

use std::collections::{BTreeMap, HashSet};

use crate::keyboard::PLAY_CODES;

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
