//! What the window remembers between runs that belongs to no device: the
//! rail's blade, open or closed, and the sliders' ends. One small JSON file
//! next to Ectomatter's settings, as the web app kept these in its storage.

use serde_json::{Map, Value};

fn path() -> std::path::PathBuf {
    crate::config_dir().join("window.json")
}

/// What the window once kept under British spellings, and its names now (Grant: "Never
/// Colour - Use Color", 2026-10-14).
const RENAMED: [(&str, &str); 2] = [("recent_colours", "recent_colors"), ("recent_cap_colours", "recent_cap_colors")];

/// What the window once kept and no longer reads, let go: Settings' tab, since
/// Settings opens at its first tab each time (first_page.rs, 2026-10-29).
const RETIRED: [&str; 1] = ["settings_tab"];

/// All of it, each thing kept under an old name read under its name now and nothing it no
/// longer reads (and saved so the next time anything is).
fn all() -> Map<String, Value> {
    renamed(crate::files::read_json(&path()).and_then(|v| v.as_object().cloned()).unwrap_or_default())
}

/// What the window kept, each thing under an old name moved to its name now (where nothing
/// is kept under that yet), and what it no longer reads let go.
fn renamed(mut m: Map<String, Value>) -> Map<String, Value> {
    for (old, new) in RENAMED {
        if let Some(v) = m.remove(old) {
            m.entry(new).or_insert(v);
        }
    }
    for gone in RETIRED {
        m.remove(gone);
    }
    m
}

pub fn get(key: &str) -> Option<Value> {
    all().remove(key)
}

pub fn set(key: &str, value: Value) {
    // offscreen checks, and the tests, leave what the window remembers alone
    if cfg!(test) || std::env::args().any(|a| a == "--snapshot") {
        return;
    }
    let mut m = all();
    m.insert(key.into(), value);
    let p = path();
    if let Err(e) = crate::files::write_json(&p, &Value::Object(m)) {
        eprintln!("ectomatter: the window's state wasn’t saved: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_kept_under_a_british_spelling_is_read_under_the_american_one() {
        let kept = serde_json::json!({ "recent_colours": [[255, 0, 0]], "recent_cap_colors": [[1, 2, 3]], "recent_cap_colours": [[9, 9, 9]], "rail_open": true });
        let m = renamed(kept.as_object().unwrap().clone());
        assert_eq!(Value::Object(m), serde_json::json!({ "recent_colors": [[255, 0, 0]], "recent_cap_colors": [[1, 2, 3]], "rail_open": true }), "the name now wins where both are kept");
    }

    /// A key the window no longer reads is let go, and nothing else with it.
    #[test]
    fn what_the_window_no_longer_reads_is_let_go() {
        let kept = serde_json::json!({ "settings_tab": 2, "rail_open": true });
        assert_eq!(Value::Object(renamed(kept.as_object().unwrap().clone())), serde_json::json!({ "rail_open": true }));
        // (and nothing reads it: no `get` of a retired key anywhere in the app)
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") && p.file_name().is_some_and(|n| n != "uistate.rs") {
                    let text = std::fs::read_to_string(&p).unwrap();
                    for gone in RETIRED {
                        assert!(!text.contains(&format!("uistate::get(\"{gone}\")")), "{} reads {gone}, which the window lets go", p.display());
                    }
                }
            }
        }
    }
}
