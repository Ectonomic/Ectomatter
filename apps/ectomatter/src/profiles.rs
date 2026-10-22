//! Profiles Ectomatter keeps for a device that keeps none of its own, one whose
//! core says its profiles are `"stored": "host"` (docs/core-protocol.md,
//! section 5, profiles), as the Sensei 310's does: as many as you want, all in
//! one file for the device in Ectomatter's own data folder. The one in use is
//! put on the device whole (the core's `profiles/apply`), and the device keeps
//! it after Ectomatter closes.

use std::path::PathBuf;

use serde_json::{json, Value};

/// One profile: an id of its own, which stays whatever it is renamed or
/// moved to (another device's key that switches to it names it so), its
/// name, and everything it sets on the device, as the core's
/// `profiles/apply` takes it (`keys`, `lighting`, `settings`).
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub body: Value,
    /// the apps a file kept before the Library kept ties says it was tied to,
    /// read once for the Library to take in (games.rs)
    pub apps: Vec<String>,
}

impl Profile {
    /// A new profile, with an id no other has.
    pub fn new(name: impl Into<String>, body: Value) -> Profile {
        Profile { id: new_id(), name: name.into(), body, apps: Vec::new() }
    }
}

/// An id for a new profile: the moment it was made, and a count, so two
/// made in the same moment differ.
fn new_id() -> String {
    static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_micros());
    format!("p{t:x}{:x}", COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

/// A device's profiles, and which is in use.
#[derive(Debug)]
pub struct Store {
    /// where they are kept; none for a snapshot or a test, which keep nothing
    path: Option<PathBuf>,
    pub list: Vec<Profile>,
    pub active: usize,
}

/// Where a device's profiles are kept: one file for it in Ectomatter's own data
/// folder (none while Ectomatter draws a snapshot or runs its tests).
pub fn path_for(device: &str) -> Option<PathBuf> {
    if cfg!(test) || std::env::args().any(|a| a == "--snapshot") {
        return None;
    }
    Some(crate::data_dir().join("profiles").join(format!("{device}.json")))
}

impl Store {
    /// A device's profiles as they were kept, if any were.
    pub fn load(path: Option<PathBuf>) -> Option<Store> {
        let v: Value = serde_json::from_slice(&std::fs::read(path.as_ref()?).ok()?).ok()?;
        let list: Vec<Profile> = v["profiles"]
            .as_array()?
            .iter()
            .map(|p| {
                let mut body = p.clone();
                let name = body.as_object_mut().and_then(|o| o.remove("name")).and_then(|n| n.as_str().map(String::from)).unwrap_or_default();
                // (a profile kept before profiles had ids takes one now)
                let id = body.as_object_mut().and_then(|o| o.remove("id")).and_then(|n| n.as_str().map(String::from)).unwrap_or_else(new_id);
                let apps = body.as_object_mut().and_then(|o| o.remove("apps")).and_then(|a| a.as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())).unwrap_or_default();
                Profile { id, name, body, apps }
            })
            .collect();
        if list.is_empty() {
            return None;
        }
        let active = (v["active"].as_u64().unwrap_or(0) as usize).min(list.len() - 1);
        Some(Store { path, list, active })
    }

    /// Profiles as a file keeps them, kept nowhere from now (a snapshot's).
    pub fn read_only(path: PathBuf) -> Option<Store> {
        let mut s = Store::load(Some(path))?;
        s.path = None;
        Some(s)
    }

    /// The first time: what the device is set to now, as Profile 1.
    pub fn first(path: Option<PathBuf>, body: Value) -> Store {
        let s = Store { path, list: vec![Profile::new("Profile 1", body)], active: 0 };
        s.save();
        s
    }

    /// Keep them as they are now.
    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        let profiles: Vec<Value> = self
            .list
            .iter()
            .map(|p| {
                let mut v = p.body.clone();
                v["name"] = json!(p.name);
                v["id"] = json!(p.id);
                if !p.apps.is_empty() {
                    v["apps"] = json!(p.apps);
                }
                v
            })
            .collect();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, serde_json::to_vec_pretty(&json!({ "active": self.active, "profiles": profiles })).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    pub fn current(&self) -> &Profile {
        &self.list[self.active]
    }

    /// The name a new profile takes: Profile and the first number no profile has.
    pub fn new_name(&self) -> String {
        (1..).map(|n| format!("Profile {n}")).find(|n| !self.list.iter().any(|p| &p.name == n)).unwrap()
    }

    /// A new profile, last in the list, in use from now.
    pub fn add(&mut self, p: Profile) -> usize {
        self.list.push(p);
        self.active = self.list.len() - 1;
        self.save();
        self.active
    }

    /// A copy of the profile in use, after it, named with "copy" after its
    /// name, in use from now.
    pub fn duplicate(&mut self) -> usize {
        let p = self.current().clone();
        let name = format!("{} copy", p.name);
        self.list.insert(self.active + 1, Profile::new(name, p.body));
        self.active += 1;
        self.save();
        self.active
    }

    /// The profile in use taken out, the one above it in use instead: where
    /// it was and what it was, for Undo. Never the only one.
    pub fn delete(&mut self) -> Option<(usize, Profile)> {
        if self.list.len() < 2 {
            return None;
        }
        let at = self.active;
        let gone = self.list.remove(at);
        self.active = at.saturating_sub(1);
        self.save();
        Some((at, gone))
    }

    /// A profile deleted put back where it was, in use again.
    pub fn restore(&mut self, (at, p): (usize, Profile)) {
        let at = at.min(self.list.len());
        self.list.insert(at, p);
        self.active = at;
        self.save();
    }

    pub fn rename(&mut self, i: usize, name: &str) {
        let name: String = name.trim().chars().take(60).collect();
        if let (Some(p), false) = (self.list.get_mut(i), name.is_empty()) {
            p.name = name;
            self.save();
        }
    }

    /// The profile in use changed as the device was: what was set on it is kept in it.
    pub fn change(&mut self, f: impl FnOnce(&mut Value)) {
        f(&mut self.list[self.active].body);
        self.save();
    }

    /// Choose the profile in use.
    pub fn choose(&mut self, i: usize) {
        if i < self.list.len() {
            self.active = i;
            self.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(dir: &std::path::Path) -> Option<PathBuf> {
        Some(dir.join("mouse.json"))
    }

    /// Profiles are kept as many as there are, the one in use with them; a new
    /// one takes the first number not in use; a copy goes after the one it
    /// copies; the only profile can't be deleted, another can, and Undo puts
    /// it back where it was.
    #[test]
    fn profiles_are_kept_made_copied_deleted_and_put_back() {
        let dir = std::env::temp_dir().join(format!("ectomatter-profiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = Store::first(kept(&dir), json!({ "settings": { "cpi1": 800 } }));
        assert!(s.delete().is_none(), "the only profile stays");
        assert_eq!(s.new_name(), "Profile 2");
        s.add(Profile::new(s.new_name(), json!({ "settings": { "cpi1": 1600 } })));
        s.rename(1, "Games");
        s.duplicate();
        assert_eq!(s.list.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Profile 1", "Games", "Games copy"]);
        assert_eq!(s.active, 2);
        s.change(|b| b["settings"]["cpi1"] = json!(2400));
        assert_eq!(s.new_name(), "Profile 2", "the first number no profile has");
        let back = Store::load(kept(&dir)).unwrap();
        assert_eq!((back.active, back.list.len(), &back.list[2].body["settings"]["cpi1"]), (2, 3, &json!(2400)));
        // (each its own id, kept, and none of it in what goes to the device)
        assert_eq!(back.list.iter().map(|p| &p.id).collect::<Vec<_>>(), s.list.iter().map(|p| &p.id).collect::<Vec<_>>());
        assert!(s.list[1].id != s.list[2].id && back.list[0].body.get("id").is_none());
        // apps a file says a profile was tied to (before the Library kept the
        // ties) are read, for the Library to take in, and never sent to the device
        s.list[1].apps = vec!["steam".into()];
        s.save();
        let back = Store::load(kept(&dir)).unwrap();
        assert_eq!((back.list[1].apps.clone(), back.list[1].body.get("apps")), (vec!["steam".to_string()], None));
        s.choose(1);
        let gone = s.delete().unwrap();
        assert_eq!((gone.0, gone.1.name.as_str(), s.active, s.current().name.as_str()), (1, "Games", 0, "Profile 1"), "the one above in use");
        s.restore(gone);
        assert_eq!((s.active, s.current().name.as_str(), s.list.len()), (1, "Games", 3));
        let _ = std::fs::remove_dir_all(&dir);
        // (a snapshot or a test keeps nothing)
        assert_eq!(path_for("sensei310"), None);
    }
}
