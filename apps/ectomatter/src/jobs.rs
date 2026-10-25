//! Ectomatter's own jobs for a device's control: what Ectomatter does when the
//! control is pressed, where what it should do belongs to another device or
//! to Ectomatter itself. The device sends one of its host slots, a code the
//! computer ignores (the core protocol's section 7); Ectomatter hears it and
//! does the job: switches another device's profile, plays one of Ectomatter's
//! own macros, does one of the keyboard's screen jobs, or switches the scene
//! (scenes.rs). So a mouse button can switch the keyboard's profile, a
//! keyboard key the mouse's, and either one the desk's scene. The devices a
//! job can switch are whatever devices Ectomatter has with profiles (Names),
//! each by the key the Library keeps its choices by, so a device brought in
//! later has its jobs everywhere at once.
//!
//! A mouse's button keeps its job in its profile, `{"ectomatter": id}` (a
//! macro as `{"macro": n}`, as the mouse page always kept it); a keyboard's
//! key, which keeps a host slot in the keyboard's own memory, has its job
//! kept by Ectomatter against the slot's code (see screen_keys::code_for).

use ectomatter_protocol::Plays;
use serde_json::{Value, json};

/// The keys of the devices whose pages give their controls jobs: the
/// keyboard's and the mouse's (each lists every other device's jobs).
pub const KEYBOARD: &str = "z10";
pub const MOUSE: &str = "sensei310";

/// What a keyboard's key's slot keeps, before the job's id, so a job is never
/// taken for a playlist (whose ids the same slots keep).
pub const ON_KEY: &str = "ectomatter-job:";

#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    /// a device's profile, the device by its key ("z10", "sensei310", a plugin's
    /// device's id): one by its id (the keyboard's by its number), the next or the one before
    Profile { device: String, to: To },
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

/// Which of a device's profiles.
#[derive(Clone, Debug, PartialEq)]
pub enum To {
    Id(String),
    Next,
    Previous,
}

impl Job {
    /// The job's id, as Ectomatter keeps it.
    pub fn id(&self) -> String {
        match self {
            // (a profile's id may hold anything, OpenRGB's being its name: it goes last)
            Job::Profile { device, to: To::Id(id) } => format!("profile:{device}:{id}"),
            Job::Profile { device, to: To::Next } => format!("next-profile:{device}"),
            Job::Profile { device, to: To::Previous } => format!("previous-profile:{device}"),
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
            "profile" => {
                let (device, id) = rest.split_once(':')?;
                if device.is_empty() || id.is_empty() {
                    return None;
                }
                Job::Profile { device: device.into(), to: To::Id(id.into()) }
            }
            "next-profile" | "previous-profile" if !rest.is_empty() => Job::Profile { device: rest.into(), to: if kind == "next-profile" { To::Next } else { To::Previous } },
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

    /// An id as Ectomatter kept it while a job named the keyboard or the mouse
    /// in place of a device ("keyboard-profile:2", "mouse-profile:p6…"), as it's
    /// kept now; none for an id that's as it should be.
    pub fn renamed(id: &str) -> Option<String> {
        let (kind, rest) = id.split_once(':')?;
        let device = match kind {
            "keyboard-profile" => KEYBOARD,
            "mouse-profile" => MOUSE,
            _ => return None,
        };
        Some(match rest {
            "next" => format!("next-profile:{device}"),
            "previous" => format!("previous-profile:{device}"),
            id => format!("profile:{device}:{id}"),
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

/// A device whose profiles a control can switch: its key, its own name, its
/// profiles (each by its id and its name), whether it steps to its next and
/// its previous, and whether the keyboard's screen jobs are its.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profiled {
    pub key: String,
    pub name: String,
    pub profiles: Vec<(String, String)>,
    pub steps: bool,
    pub screen: bool,
}

impl Profiled {
    /// Its jobs: each profile, the next and the one before, its screen's.
    pub fn jobs(&self) -> Vec<Job> {
        let profile = |to: To| Job::Profile { device: self.key.clone(), to };
        let mut out: Vec<Job> = self.profiles.iter().map(|(id, _)| profile(To::Id(id.clone()))).collect();
        if self.steps {
            out.extend([profile(To::Next), profile(To::Previous)]);
        }
        if self.screen {
            out.extend([crate::z10::NEXT_LAYOUT, crate::z10::PREV_LAYOUT, crate::z10::NEXT_PLAYLIST, crate::z10::PREV_PLAYLIST].map(Job::Screen));
        }
        out
    }
}

/// The names the jobs' words use, as the pages last said them: the devices
/// with profiles, as they sit on Home's desk, left to right; Ectomatter's
/// macros (by their numbers); and the scenes (by their ids, in Home's order).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Names {
    pub devices: Vec<Profiled>,
    pub macros: Vec<(u32, String)>,
    pub scenes: Vec<(String, String)>,
}

impl Names {
    /// A job in words, as a menu or a key's label says it.
    pub fn of(&self, j: &Job) -> String {
        match j {
            Job::Profile { device, to } => match (self.device(device), to) {
                (Some(d), To::Id(id)) => match d.profiles.iter().find(|(k, _)| k == id) {
                    Some((_, name)) => format!("{}: {name}", d.name),
                    None => format!("{}: a deleted profile", d.name),
                },
                (Some(d), To::Next) => format!("{}: next profile", d.name),
                (Some(d), To::Previous) => format!("{}: previous profile", d.name),
                (None, _) => "A profile of a device that isn’t here".into(),
            },
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

    /// A job in a list under its device's heading, or the scenes': a profile
    /// or a scene by its own name, the rest in their own words ("Next
    /// profile", "Next scene"), as the Macro Editor lists them.
    pub fn short(&self, j: &Job) -> String {
        match j {
            Job::Profile { device, to: To::Id(id) } => match self.device(device) {
                Some(d) => d.profiles.iter().find(|(k, _)| k == id).map_or_else(|| "A deleted profile".into(), |(_, name)| name.clone()),
                None => "A profile of a device that isn’t here".into(),
            },
            Job::Profile { to: To::Next, .. } => "Next profile".into(),
            Job::Profile { to: To::Previous, .. } => "Previous profile".into(),
            Job::Scene(SceneTo::Id(id)) => self.scenes.iter().find(|(k, _)| k == id).map_or_else(|| "A deleted scene".into(), |(_, name)| name.clone()),
            _ => self.of(j),
        }
    }

    /// A device by its key.
    pub fn device(&self, key: &str) -> Option<&Profiled> {
        self.devices.iter().find(|d| d.key == key)
    }

    /// The heading a job is listed under: its device's name (the screen's jobs,
    /// the device the screen is), or Scenes; a macro, none.
    pub fn group(&self, j: &Job) -> Option<String> {
        match j {
            Job::Profile { device, .. } => Some(self.device(device).map_or_else(|| "Other devices".to_string(), |d| d.name.clone())),
            Job::Screen(_) => self.devices.iter().find(|d| d.screen).map(|d| d.name.clone()),
            Job::Scene(_) => Some("Scenes".into()),
            Job::Macro { .. } => None,
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

    /// The jobs a control of one device (by its key) can be given that act on
    /// another, or on the desk, each with its words: every other device's, as
    /// they sit on the desk, then the scenes'. (Ectomatter's macros are each
    /// page's own to list.)
    pub fn for_device(&self, device: &str) -> Vec<(String, Job)> {
        let mut out: Vec<Job> = self.devices.iter().filter(|d| d.key != device).flat_map(Profiled::jobs).collect();
        out.extend(self.scene_jobs());
        out.into_iter().map(|j| (self.of(&j), j)).collect()
    }
}

/// Every job a kept value names by an old id (Job::renamed), as a mouse's
/// button keeps one (`{"ectomatter": id}`) or a macro's step (`{"job": id}`),
/// named as it's kept now, wherever it is in the value; whether any was.
pub fn rename_in(v: &mut Value) -> bool {
    match v {
        Value::Object(m) => {
            let mut changed = false;
            for key in ["ectomatter", "job"] {
                if let Some(new) = m.get(key).and_then(Value::as_str).and_then(Job::renamed) {
                    m.insert(key.into(), Value::String(new));
                    changed = true;
                }
            }
            for x in m.values_mut() {
                changed |= rename_in(x);
            }
            changed
        }
        Value::Array(a) => a.iter_mut().fold(false, |changed, x| rename_in(x) | changed),
        _ => false,
    }
}

/// Ectomatter's macros whose steps name a job by an old id, kept again with
/// it named as it's kept now.
pub fn rename_in_macros() {
    let all = crate::library::all();
    for (n, m) in all.as_object().into_iter().flatten() {
        let mut steps = m["steps"].clone();
        if let (true, Ok(n)) = (rename_in(&mut steps), n.parse::<u32>()) {
            crate::library::set(n, m["name"].as_str().unwrap_or(""), steps.as_array().cloned().unwrap_or_default());
        }
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

    /// The desk in these tests: the keyboard, the mouse, and a device a
    /// plugin brings in, each by its own name.
    fn desk() -> Names {
        let device = |key: &str, name: &str, profiles: &[(&str, &str)], steps: bool, screen: bool| Profiled {
            key: key.into(),
            name: name.into(),
            profiles: profiles.iter().map(|(id, n)| (id.to_string(), n.to_string())).collect(),
            steps,
            screen,
        };
        Names {
            devices: vec![
                device(KEYBOARD, "EVGA Z10 RGB", &[("1", "Work"), ("2", "Profile 2")], true, true),
                device(MOUSE, "SteelSeries Sensei 310", &[("pa", "Games")], true, false),
                device("openrgb", "OpenRGB", &[("Studio Low", "Studio Low")], false, false),
            ],
            macros: vec![(1, "Copy".into())],
            scenes: vec![],
        }
    }

    /// Every job reads back from its id, and from how a mouse's button keeps it.
    #[test]
    fn each_job_reads_back_as_itself() {
        let profile = |device: &str, to: To| Job::Profile { device: device.into(), to };
        let jobs = [
            profile(KEYBOARD, To::Id("3".into())),
            profile(KEYBOARD, To::Next),
            profile(MOUSE, To::Id("p18f2a".into())),
            profile(MOUSE, To::Previous),
            // (an id with what it likes in it: OpenRGB's are its profiles' names)
            profile("openrgb", To::Id("Studio: Low".into())),
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
        for bad in ["profile:", "profile:z10", "profile::2", "next-profile:", "scene:"] {
            assert_eq!(Job::of(bad), None, "{bad}");
        }
        assert_eq!(Job::read(&json!({ "key": "A" })), None);
    }

    /// The ids kept while a job named the keyboard or the mouse are kept by
    /// the device's key now; the rest are as they were.
    #[test]
    fn the_old_ids_name_their_devices() {
        assert_eq!(Job::renamed("keyboard-profile:2").as_deref(), Some("profile:z10:2"));
        assert_eq!(Job::renamed("keyboard-profile:next").as_deref(), Some("next-profile:z10"));
        assert_eq!(Job::renamed("mouse-profile:p65e5e7d567f370").as_deref(), Some("profile:sensei310:p65e5e7d567f370"));
        assert_eq!(Job::renamed("mouse-profile:previous").as_deref(), Some("previous-profile:sensei310"));
        for kept in ["profile:z10:2", "scene:none", "macro:3:once", "screen:167"] {
            assert_eq!(Job::renamed(kept), None, "{kept}");
        }
    }

    /// Each device is offered every other device's jobs, as they sit on the
    /// desk, each in words naming the device by its own name; a device a plugin
    /// brings in among them, and none of a device's own.
    #[test]
    fn a_device_is_offered_the_others_jobs() {
        let names = desk();
        let words = |d| names.for_device(d).into_iter().map(|(w, _)| w).collect::<Vec<_>>();
        assert_eq!(words(MOUSE)[..4], ["EVGA Z10 RGB: Work", "EVGA Z10 RGB: Profile 2", "EVGA Z10 RGB: next profile", "EVGA Z10 RGB: previous profile"]);
        assert!(words(MOUSE).contains(&"Screen: next layout".to_string()));
        assert!(words(MOUSE).iter().all(|w| !w.starts_with("SteelSeries")), "none of its own");
        assert_eq!(words(KEYBOARD), ["SteelSeries Sensei 310: Games", "SteelSeries Sensei 310: next profile", "SteelSeries Sensei 310: previous profile", "OpenRGB: Studio Low"]);
        let profile = |device: &str, id: &str| Job::Profile { device: device.into(), to: To::Id(id.into()) };
        assert_eq!(names.of(&profile(MOUSE, "gone")), "SteelSeries Sensei 310: a deleted profile");
        assert_eq!(names.of(&profile("deck", "1")), "A profile of a device that isn’t here");
        assert_eq!((stepped(0, 3, false), stepped(2, 3, true)), (2, 0));
    }

    /// A button's job and a macro's step kept by an old id are kept by the new,
    /// wherever they are; a value with none is left as it was.
    #[test]
    fn old_ids_are_renamed_where_they_are_kept() {
        let mut body = json!({ "keys": { "base": { "left-side-back": { "ectomatter": "keyboard-profile:2" }, "right": { "mouse": 2 } } } });
        assert!(rename_in(&mut body));
        assert_eq!(body["keys"]["base"]["left-side-back"], json!({ "ectomatter": "profile:z10:2" }));
        let mut steps = json!([{ "job": "mouse-profile:pa", "after": 0 }, { "key": "A", "down": true, "after": 0 }]);
        assert!(rename_in(&mut steps));
        assert_eq!(steps[0]["job"], "profile:sensei310:pa");
        let mut kept = json!([{ "job": "scene:none", "after": 0 }]);
        assert!(!rename_in(&mut kept));
    }

    /// A device brought in later is offered with the rest, under its own name.
    #[test]
    fn a_new_device_is_offered_as_it_comes() {
        let mut names = desk();
        names.devices.push(Profiled { key: "m3".into(), name: "OpenDeck M3".into(), profiles: vec![("a".into(), "Streaming".into())], steps: true, screen: false });
        let jobs = names.for_device(KEYBOARD);
        assert!(jobs.iter().any(|(w, _)| w == "OpenDeck M3: Streaming"));
        assert_eq!(names.group(&jobs.last().unwrap().1).as_deref(), Some("OpenDeck M3"));
    }

    /// Both devices are offered the scenes' jobs, each scene by its name, then
    /// the next, the one before and none; a deleted one says so.
    #[test]
    fn both_devices_are_offered_the_scenes() {
        let names = Names { scenes: vec![("wave".into(), "Wave across the desk".into()), ("scene-1".into(), "DOOM".into())], macros: vec![(1, "Copy".into())], ..Default::default() };
        let words = |d| names.for_device(d).into_iter().map(|(w, _)| w).collect::<Vec<_>>();
        assert_eq!(words(MOUSE), ["Scene: Wave across the desk", "Scene: DOOM", "Next scene", "Previous scene", "Scene off"]);
        assert_eq!(words(KEYBOARD), words(MOUSE));
        assert_eq!(names.of(&Job::Scene(SceneTo::Id("gone".into()))), "Scene: a deleted scene");
    }

    /// Under its heading, its device's name, a job goes by its own name: a
    /// profile or a scene by its name, the rest in their own words.
    #[test]
    fn under_its_heading_a_job_goes_by_its_own_name() {
        let mut names = desk();
        names.scenes = vec![("scene-1".into(), "DOOM".into())];
        let profile = |device: &str, to: To| Job::Profile { device: device.into(), to };
        let short = |j: Job| names.short(&j);
        assert_eq!(short(profile(KEYBOARD, To::Id("1".into()))), "Work");
        assert_eq!(short(profile(MOUSE, To::Id("pa".into()))), "Games");
        assert_eq!([short(profile(MOUSE, To::Next)), short(profile(KEYBOARD, To::Previous))], ["Next profile", "Previous profile"]);
        assert_eq!(short(Job::Scene(SceneTo::Id("scene-1".into()))), "DOOM");
        assert_eq!([short(Job::Scene(SceneTo::Next)), short(Job::Scene(SceneTo::Off))], ["Next scene", "Scene off"]);
        assert_eq!(short(Job::Screen(crate::z10::NEXT_LAYOUT)), "Screen: next layout");
        let group = |j: Job| names.group(&j);
        assert_eq!(group(profile(MOUSE, To::Next)).as_deref(), Some("SteelSeries Sensei 310"));
        assert_eq!(group(Job::Screen(crate::z10::NEXT_LAYOUT)).as_deref(), Some("EVGA Z10 RGB"));
        assert_eq!(group(Job::Scene(SceneTo::Off)).as_deref(), Some("Scenes"));
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
