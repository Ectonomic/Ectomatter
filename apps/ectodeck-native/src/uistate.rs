//! What the window remembers between runs that belongs to no device: the
//! rail's blade, open or closed, and the sliders' ends. One small JSON file
//! next to Ectodeck's settings, as the web app kept these in its storage.

use serde_json::{Map, Value};

fn path() -> std::path::PathBuf {
    crate::config_dir().join("window.json")
}

fn all() -> Map<String, Value> {
    std::fs::read(path()).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

pub fn get(key: &str) -> Option<Value> {
    all().remove(key)
}

pub fn set(key: &str, value: Value) {
    let mut m = all();
    m.insert(key.into(), value);
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::to_vec_pretty(&Value::Object(m)).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
}
