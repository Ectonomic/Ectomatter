//! Ectomatter's own settings: what it knows that no device stores for it.
//!
//! One JSON file in the app's configuration folder, read once at start and
//! written whole each time something changes, by way of a temporary file so a
//! crash mid-write never leaves half a file. The screens read it all and set
//! one value at a time by its path, such as `devices.z10.control`.

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{Map, Value};

pub struct Settings {
    path: PathBuf,
    data: Mutex<Value>,
    /// a change made while it is still being made (a slider dragged), not yet
    /// written: when it was last changed
    unsaved: Mutex<Option<std::time::Instant>>,
}

impl Settings {
    pub fn load(path: PathBuf) -> Settings {
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| Value::Object(Map::new()));
        Settings { path, data: Mutex::new(data), unsaved: Mutex::new(None) }
    }

    pub fn all(&self) -> Value {
        self.data.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// The value at a dotted path, if there is one.
    pub fn get(&self, path: &str) -> Option<Value> {
        let d = self.data.lock().unwrap_or_else(|p| p.into_inner());
        let mut v = &*d;
        for k in path.split('.') {
            v = v.get(k)?;
        }
        Some(v.clone())
    }

    /// A switch that is on unless it has been turned off.
    pub fn on(&self, path: &str) -> bool {
        self.get(path).and_then(|v| v.as_bool()).unwrap_or(true)
    }

    /// Set the value at a dotted path, making any objects on the way, and
    /// write the file.
    pub fn set(&self, path: &str, value: Value) -> std::io::Result<()> {
        self.put(path, value);
        *self.unsaved.lock().unwrap_or_else(|p| p.into_inner()) = None;
        self.write()
    }

    /// Take a key out of the settings and write them: its value, if there was one.
    pub fn take(&self, key: &str) -> Option<Value> {
        let v = self.data.lock().unwrap_or_else(|p| p.into_inner()).as_object_mut()?.remove(key)?;
        let _ = self.write();
        Some(v)
    }

    /// Set a value that is still being changed, such as a slider being
    /// dragged: it takes effect at once, and is written once it rests.
    pub fn set_soon(&self, path: &str, value: Value) {
        self.put(path, value);
        *self.unsaved.lock().unwrap_or_else(|p| p.into_inner()) = Some(std::time::Instant::now());
    }

    /// Write what was set soon, once it has rested a moment (or now, with `now`).
    pub fn flush(&self, now: bool) {
        let mut u = self.unsaved.lock().unwrap_or_else(|p| p.into_inner());
        if u.is_some_and(|t| now || t.elapsed() >= std::time::Duration::from_millis(400)) {
            *u = None;
            drop(u);
            let _ = self.write();
        }
    }

    fn put(&self, path: &str, value: Value) {
        let mut d = self.data.lock().unwrap_or_else(|p| p.into_inner());
        let mut v = &mut *d;
        let keys: Vec<&str> = path.split('.').collect();
        for k in &keys[..keys.len() - 1] {
            if !v.get(*k).is_some_and(Value::is_object) {
                v[*k] = Value::Object(Map::new());
            }
            v = v.get_mut(*k).expect("just made");
        }
        v[keys[keys.len() - 1]] = value;
    }

    fn write(&self) -> std::io::Result<()> {
        let d = self.data.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&*d)?)?;
        std::fs::rename(&tmp, &self.path)
    }
}
