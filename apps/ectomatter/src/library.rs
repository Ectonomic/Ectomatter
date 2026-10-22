//! Ectomatter's own macros: kept by Ectomatter and played by it (typist.rs) for
//! any device's control given one, as the mouse's buttons can be. A device
//! that keeps macros in its own memory, as the Z10 does, has its own besides.
//! As many as you want, numbered from 1, in one file in Ectomatter's data
//! folder; each is a name and its steps, in the core protocol's shape.

use std::sync::Mutex;

use serde_json::{json, Map, Value};

/// The macros, read from their file the first time they're asked for.
static MACROS: Mutex<Option<Map<String, Value>>> = Mutex::new(None);

fn path() -> Option<std::path::PathBuf> {
    // (a snapshot or a test keeps nothing)
    if cfg!(test) || std::env::args().any(|a| a == "--snapshot") {
        return None;
    }
    Some(crate::data_dir().join("macros.json"))
}

fn with<R>(f: impl FnOnce(&mut Map<String, Value>) -> R) -> R {
    let mut g = MACROS.lock().unwrap_or_else(|p| p.into_inner());
    let m = g.get_or_insert_with(|| {
        path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v["macros"].as_object().cloned())
            .unwrap_or_default()
    });
    f(m)
}

fn keep(m: &Map<String, Value>) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::to_vec_pretty(&json!({ "macros": m })).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
}

/// Every macro, by its number: its name and steps.
pub fn all() -> Value {
    with(|m| Value::Object(m.clone()))
}

/// One macro, by its number.
pub fn get(n: u32) -> Option<Value> {
    with(|m| m.get(&n.to_string()).cloned())
}

/// A macro saved: its name and its steps.
pub fn set(n: u32, name: &str, steps: Vec<Value>) {
    with(|m| {
        m.insert(n.to_string(), json!({ "name": name, "steps": steps }));
        keep(m);
    });
}

/// A macro deleted.
pub fn delete(n: u32) {
    with(|m| {
        m.remove(&n.to_string());
        keep(m);
    });
}

/// The numbers no macro has, from 1, as many as asked for.
pub fn free(count: usize) -> Vec<u32> {
    with(|m| (1u32..).filter(|n| !m.contains_key(&n.to_string())).take(count).collect())
}

/// The macros a snapshot shows, in place of the file's (and kept nowhere).
pub fn seed(v: &Value) {
    with(|m| *m = v["macros"].as_object().cloned().unwrap_or_default());
}

/// The tests that use the macros take turns, as they share them.
#[cfg(test)]
pub static TESTS: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub fn clear() {
    with(|m| m.clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A macro saved is read back, the first free number skips the ones in
    /// use, and one deleted frees its number.
    #[test]
    fn macros_are_saved_read_and_deleted() {
        let _one = TESTS.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        set(1, "Paste", vec![json!({ "key": "LeftCtrl", "down": true, "after": 0 })]);
        set(3, "Mute", vec![]);
        assert_eq!(get(1).unwrap()["name"], "Paste");
        assert_eq!(free(2), [2, 4]);
        delete(1);
        assert_eq!(free(1), [1]);
        assert_eq!(all().as_object().unwrap().len(), 1);
        clear();
    }
}
