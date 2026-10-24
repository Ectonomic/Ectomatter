//! Ectomatter's own jobs for a device's control: what Ectomatter does when the
//! control is pressed, where what it should do belongs to another device or
//! to Ectomatter itself. The device sends one of its host slots, a code the
//! computer ignores (the core protocol's section 7); Ectomatter hears it and
//! does the job: switches another device's profile, plays one of Ectomatter's
//! own macros, does one of the keyboard's screen jobs, or switches the scene
//! (scenes.rs). So a mouse button can switch the keyboard's profile, a
//! keyboard key the mouse's, and either one the desk's scene.
//!
//! A mouse's button keeps its job in its profile, `{"ectomatter": id}` (a
//! macro as `{"macro": n}`, as the mouse page always kept it); a keyboard's
//! key, which keeps a host slot in the keyboard's own memory, has its job
//! kept by Ectomatter against the slot's code (see screen_keys::code_for).

use ectomatter_protocol::{Jump, Plays};
use serde_json::{Value, json};

/// The devices a job can be for, by the ids Ectomatter keeps their profiles by.
pub const KEYBOARD: &str = "z10";
pub const MOUSE: &str = "sensei310";

/// What a keyboard's key's slot keeps, before the job's id, so a job is never
/// taken for a playlist (whose ids the same slots keep).
pub const ON_KEY: &str = "ectomatter-job:";

#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    /// the keyboard's profile: by its number, or the next or the one before
    KeyboardProfile(Jump),
    /// the mouse's profile: by its id (profiles::Profile), or the next or the one before
    MouseProfile(To),
    /// one of Ectomatter's macros, played as it says
    Macro { n: u32, plays: Plays },
    /// one of the keyboard's screen jobs, by its code: the next or previous
    /// layout, the next or previous playlist
    Screen(u32),
    /// the desk's scene: one by its id (on, or off again if it's on), the next
    /// or the one before (round the scenes in Home's order), or none
    Scene(SceneTo),
}

/// Which scene.
#[derive(Clone, Debug, PartialEq)]
pub enum SceneTo {
    Id(String),
    Next,
    Previous,
    Off,
}

/// Which of the mouse's profiles.
#[derive(Clone, Debug, PartialEq)]
pub enum To {
    Id(String),
    Next,
    Previous,
}

impl Job {
    /// The job's id, as Ectomatter keeps it.
    pub fn id(&self) -> String {
        let jump = |j: &Jump| match j {
            Jump::To(n) => n.to_string(),
            Jump::Next => "next".into(),
            Jump::Previous => "previous".into(),
        };
        match self {
            Job::KeyboardProfile(j) => format!("keyboard-profile:{}", jump(j)),
            Job::MouseProfile(To::Id(id)) => format!("mouse-profile:{id}"),
            Job::MouseProfile(To::Next) => "mouse-profile:next".into(),
            Job::MouseProfile(To::Previous) => "mouse-profile:previous".into(),
            Job::Macro { n, plays } => format!("macro:{n}:{}", match plays {
                Plays::Once => "once".to_string(),
                Plays::Times(t) => format!("times-{t}"),
                Plays::WhileHeld => "while-held".into(),
                Plays::UntilPressedAgain => "until-pressed-again".into(),
            }),
            Job::Screen(code) => format!("screen:{code}"),
            Job::Scene(SceneTo::Id(id)) => format!("scene:{id}"),
            Job::Scene(SceneTo::Next) => "scene:next".into(),
            Job::Scene(SceneTo::Previous) => "scene:previous".into(),
            Job::Scene(SceneTo::Off) => "scene:none".into(),
        }
    }

    /// The job an id names.
    pub fn of(id: &str) -> Option<Job> {
        let (kind, rest) = id.split_once(':')?;
        Some(match kind {
            "keyboard-profile" => Job::KeyboardProfile(match rest {
                "next" => Jump::Next,
                "previous" => Jump::Previous,
                n => Jump::To(n.parse().ok()?),
            }),
            "mouse-profile" => Job::MouseProfile(match rest {
                "next" => To::Next,
                "previous" => To::Previous,
                id if !id.is_empty() => To::Id(id.to_string()),
                _ => return None,
            }),
            "macro" => {
                let (n, plays) = rest.split_once(':').unwrap_or((rest, "once"));
                let plays = match plays {
                    "once" => Plays::Once,
                    "while-held" => Plays::WhileHeld,
                    "until-pressed-again" => Plays::UntilPressedAgain,
                    t => Plays::Times(t.strip_prefix("times-")?.parse().ok()?),
                };
                Job::Macro { n: n.parse().ok()?, plays }
            }
            "screen" => Job::Screen(rest.parse().ok()?),
            "scene" => Job::Scene(match rest {
                "next" => SceneTo::Next,
                "previous" => SceneTo::Previous,
                "none" => SceneTo::Off,
                id if !id.is_empty() => SceneTo::Id(id.to_string()),
                _ => return None,
            }),
            _ => return None,
        })
    }

    /// The job as a mouse's profile keeps it on a button.
    pub fn write(&self) -> Value {
        match self {
            Job::Macro { n, plays } => ectomatter_protocol::Action::Macro { n: *n, plays: *plays }.write(),
            j => json!({ "ectomatter": j.id() }),
        }
    }

    /// The job a mouse's button keeps, if it keeps one.
    pub fn read(v: &Value) -> Option<Job> {
        if let Some(id) = v.get("ectomatter").and_then(Value::as_str) {
            return Job::of(id);
        }
        match ectomatter_protocol::Action::read(v)? {
            ectomatter_protocol::Action::Macro { n, plays } => Some(Job::Macro { n, plays }),
            _ => None,
        }
    }

}

/// The names the jobs' words use: the keyboard's profiles (by their numbers,
/// 1 to 5), the mouse's (by their ids), Ectomatter's macros (by their
/// numbers) and the scenes (by their ids, in Home's order), as the pages last
/// said them; and the devices as they sit on Home's desk, left to right
/// (KEYBOARD, MOUSE), which their jobs are listed in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Names {
    pub keyboard: Vec<(u8, String)>,
    pub mouse: Vec<(String, String)>,
    pub macros: Vec<(u32, String)>,
    pub order: Vec<&'static str>,
    pub scenes: Vec<(String, String)>,
}

impl Names {
    /// A job in words, as a menu or a key's label says it.
    pub fn of(&self, j: &Job) -> String {
        match j {
            Job::KeyboardProfile(Jump::To(n)) => {
                let name = self.keyboard.iter().find(|(k, _)| *k as u32 == *n).map_or_else(|| format!("Profile {n}"), |(_, s)| s.clone());
                format!("Keyboard: {name}")
            }
            Job::KeyboardProfile(Jump::Next) => "Keyboard: next profile".into(),
            Job::KeyboardProfile(Jump::Previous) => "Keyboard: previous profile".into(),
            Job::MouseProfile(To::Id(id)) => match self.mouse.iter().find(|(k, _)| k == id) {
                Some((_, name)) => format!("Mouse: {name}"),
                None => "Mouse: a deleted profile".into(),
            },
            Job::MouseProfile(To::Next) => "Mouse: next profile".into(),
            Job::MouseProfile(To::Previous) => "Mouse: previous profile".into(),
            Job::Macro { n, .. } => match self.macros.iter().find(|(k, _)| k == n) {
                Some((_, name)) => name.clone(),
                None => "Deleted macro".into(),
            },
            Job::Screen(code) => match *code {
                crate::z10::NEXT_LAYOUT => "Screen: next layout".into(),
                crate::z10::PREV_LAYOUT => "Screen: previous layout".into(),
                crate::z10::NEXT_PLAYLIST => "Screen: next playlist".into(),
                crate::z10::PREV_PLAYLIST => "Screen: previous playlist".into(),
                _ => "Screen".into(),
            },
            Job::Scene(SceneTo::Id(id)) => match self.scenes.iter().find(|(k, _)| k == id) {
                Some((_, name)) => format!("Scene: {name}"),
                None => "Scene: a deleted scene".into(),
            },
            Job::Scene(SceneTo::Next) => "Next scene".into(),
            Job::Scene(SceneTo::Previous) => "Previous scene".into(),
            Job::Scene(SceneTo::Off) => "Scene off".into(),
        }
    }

    /// The scenes' jobs: each scene, the next, the one before, and off.
    pub fn scene_jobs(&self) -> Vec<Job> {
        if self.scenes.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<Job> = self.scenes.iter().map(|(id, _)| Job::Scene(SceneTo::Id(id.clone()))).collect();
        out.extend([Job::Scene(SceneTo::Next), Job::Scene(SceneTo::Previous), Job::Scene(SceneTo::Off)]);
        out
    }

    /// The jobs a control of one device can be given that act on another,
    /// or on Ectomatter itself, each with its words: for the mouse's buttons,
    /// the keyboard's; for the keyboard's keys, the mouse's and Ectomatter's
    /// macros; for either, the scenes'.
    pub fn for_device(&self, device: &str) -> Vec<(String, Job)> {
        let mut out: Vec<Job> = Vec::new();
        // (each other device's, as they sit on the desk)
        let mut order: Vec<&str> = self.order.clone();
        for d in [KEYBOARD, MOUSE] {
            if !order.contains(&d) {
                order.push(d);
            }
        }
        for d in order {
            if d == KEYBOARD && device != KEYBOARD && !self.keyboard.is_empty() {
                out.extend(self.keyboard.iter().map(|(n, _)| Job::KeyboardProfile(Jump::To(*n as u32))));
                out.extend([Job::KeyboardProfile(Jump::Next), Job::KeyboardProfile(Jump::Previous)]);
                out.extend([crate::z10::NEXT_LAYOUT, crate::z10::PREV_LAYOUT, crate::z10::NEXT_PLAYLIST, crate::z10::PREV_PLAYLIST].map(Job::Screen));
            }
            if d == MOUSE && device != MOUSE && !self.mouse.is_empty() {
                out.extend(self.mouse.iter().map(|(id, _)| Job::MouseProfile(To::Id(id.clone()))));
                out.extend([Job::MouseProfile(To::Next), Job::MouseProfile(To::Previous)]);
            }
        }
        out.extend(self.scene_jobs());
        if device == KEYBOARD {
            out.extend(self.macros.iter().map(|(n, _)| Job::Macro { n: *n, plays: Plays::Once }));
        }
        out.into_iter().map(|j| (self.of(&j), j)).collect()
    }
}

/// The step a profile list takes for `next` or the one before: where it
/// goes from `at` among `count`, round at its ends.
pub fn stepped(at: usize, count: usize, next: bool) -> usize {
    if count == 0 {
        return 0;
    }
    if next { (at + 1) % count } else { (at + count - 1) % count }
}

/// What a scene's job does, with the scene on (`on`) among every scene's id
/// in order: nothing (None) for a deleted scene's, else the scene it turns
/// on, or none. The scene on, chosen again, goes off, as in Home's menu; the
/// next and the one before go round, from the first and the last with none on.
pub fn scene_after(to: &SceneTo, on: Option<&str>, ids: &[String]) -> Option<Option<String>> {
    let at = on.and_then(|id| ids.iter().position(|s| s == id));
    match to {
        SceneTo::Id(id) if !ids.contains(id) => None,
        SceneTo::Id(id) => Some((on != Some(id.as_str())).then(|| id.clone())),
        SceneTo::Off => Some(None),
        SceneTo::Next => Some(ids.get(at.map_or(0, |i| stepped(i, ids.len(), true))).cloned()),
        SceneTo::Previous => Some(ids.get(at.map_or(ids.len().saturating_sub(1), |i| stepped(i, ids.len(), false))).cloned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every job reads back from its id, and from how a mouse's button keeps it.
    #[test]
    fn each_job_reads_back_as_itself() {
        let jobs = [
            Job::KeyboardProfile(Jump::To(3)),
            Job::KeyboardProfile(Jump::Next),
            Job::KeyboardProfile(Jump::Previous),
            Job::MouseProfile(To::Id("p18f2a".into())),
            Job::MouseProfile(To::Next),
            Job::MouseProfile(To::Previous),
            Job::Macro { n: 4, plays: Plays::Times(3) },
            Job::Macro { n: 2, plays: Plays::WhileHeld },
            Job::Screen(crate::z10::NEXT_PLAYLIST),
            Job::Scene(SceneTo::Id("scene-3".into())),
            Job::Scene(SceneTo::Next),
            Job::Scene(SceneTo::Previous),
            Job::Scene(SceneTo::Off),
        ];
        for j in jobs {
            assert_eq!(Job::of(&j.id()), Some(j.clone()), "{}", j.id());
            assert_eq!(Job::read(&j.write()), Some(j.clone()), "{}", j.write());
        }
        assert_eq!(Job::of("mouse-profile:"), None);
        assert_eq!(Job::of("scene:"), None);
        assert_eq!(Job::read(&json!({ "key": "A" })), None);
    }

    /// Each device is offered the others' jobs: the mouse the keyboard's,
    /// the keyboard the mouse's and Ectomatter's macros; each in words.
    #[test]
    fn a_device_is_offered_the_others_jobs() {
        let names = Names { keyboard: vec![(1, "Work".into()), (2, "Profile 2".into())], mouse: vec![("pa".into(), "Games".into())], macros: vec![(1, "Copy".into())], order: vec![MOUSE, KEYBOARD], scenes: vec![] };
        let mouse: Vec<String> = names.for_device(MOUSE).into_iter().map(|(w, _)| w).collect();
        assert_eq!(mouse[..3], ["Keyboard: Work", "Keyboard: Profile 2", "Keyboard: next profile"]);
        assert!(mouse.iter().all(|w| !w.starts_with("Mouse")), "none of its own");
        let keys: Vec<String> = names.for_device(KEYBOARD).into_iter().map(|(w, _)| w).collect();
        assert_eq!(keys, ["Mouse: Games", "Mouse: next profile", "Mouse: previous profile", "Copy"]);
        assert_eq!(names.of(&Job::MouseProfile(To::Id("gone".into()))), "Mouse: a deleted profile");
        assert_eq!((stepped(0, 3, false), stepped(2, 3, true)), (2, 0));
    }

    /// Both devices are offered the scenes' jobs, each scene by its name, then
    /// the next, the one before and none; a deleted one says so.
    #[test]
    fn both_devices_are_offered_the_scenes() {
        let names = Names { scenes: vec![("wave".into(), "Wave across the desk".into()), ("scene-1".into(), "DOOM".into())], macros: vec![(1, "Copy".into())], ..Default::default() };
        let words = |d| names.for_device(d).into_iter().map(|(w, _)| w).collect::<Vec<_>>();
        assert_eq!(words(MOUSE), ["Scene: Wave across the desk", "Scene: DOOM", "Next scene", "Previous scene", "Scene off"]);
        assert_eq!(words(KEYBOARD), ["Scene: Wave across the desk", "Scene: DOOM", "Next scene", "Previous scene", "Scene off", "Copy"]);
        assert_eq!(names.of(&Job::Scene(SceneTo::Id("gone".into()))), "Scene: a deleted scene");
    }

    /// A scene's job turns its scene on, or off when it's the one on; the next
    /// and the one before go round the scenes; a deleted scene's does nothing.
    #[test]
    fn a_scenes_job_turns_scenes_on_and_off() {
        let ids: Vec<String> = ["wave", "shift", "scene-1"].map(String::from).to_vec();
        let after = |to: SceneTo, on: Option<&str>| scene_after(&to, on, &ids);
        let on = |id: &str| Some(Some(id.to_string()));
        assert_eq!(after(SceneTo::Id("shift".into()), None), on("shift"));
        assert_eq!(after(SceneTo::Id("shift".into()), Some("wave")), on("shift"));
        assert_eq!(after(SceneTo::Id("shift".into()), Some("shift")), Some(None));
        assert_eq!(after(SceneTo::Id("gone".into()), Some("wave")), None);
        assert_eq!(after(SceneTo::Off, Some("wave")), Some(None));
        assert_eq!(after(SceneTo::Off, None), Some(None));
        assert_eq!([None, Some("wave"), Some("scene-1")].map(|o| after(SceneTo::Next, o)), [on("wave"), on("shift"), on("wave")]);
        assert_eq!([None, Some("wave"), Some("shift")].map(|o| after(SceneTo::Previous, o)), [on("scene-1"), on("scene-1"), on("wave")]);
        // (a scene on that's since gone counts as none)
        assert_eq!(after(SceneTo::Next, Some("gone")), on("wave"));
    }
}
