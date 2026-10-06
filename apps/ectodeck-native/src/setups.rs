//! A device's light setups of the person's own, beside the one its core
//! describes: one for the caps they have put on it, say. Each is a file in
//! Ectodeck's own data, light/<core>/<id>.json, holding its name and only
//! what its light changes from the core's, as a JSON merge patch (RFC 7386):
//! a value there takes the place of the core's, an object is merged into
//! the core's, and null takes the core's out. What a setup leaves alone
//! follows the core, as the core's author tunes it. Which setup lights the
//! device is kept in Ectodeck's settings, by its id; none is the core's own.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use ectodeck_draw::jsonc;

/// Where the setup lighting the Z10 is kept in Ectodeck's settings.
pub const IN_USE: &str = "devices.z10.light";

#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    /// its file's name, made from its name
    pub id: String,
    pub name: String,
    /// what its light changes from the core's
    pub changes: Value,
}

/// Where a core's setups are kept.
pub fn dir(core: &str) -> PathBuf {
    crate::data_dir().join("light").join(core)
}

/// A core's setups, by name.
pub fn list(core: &str) -> Vec<Setup> {
    let mut out: Vec<Setup> = std::fs::read_dir(dir(core))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            (path.extension()? == "json").then_some(())?;
            let id = path.file_stem()?.to_str()?.to_string();
            read_file(&path, id)
        })
        .collect();
    out.sort_by_key(|s| s.name.to_lowercase());
    out
}

fn read_file(path: &Path, id: String) -> Option<Setup> {
    let v = jsonc::parse(&std::fs::read_to_string(path).ok()?).ok()?;
    let name = v["name"].as_str().map(str::to_string).unwrap_or_else(|| id.clone());
    Some(Setup { id, name, changes: v.get("light").filter(|l| l.is_object()).cloned().unwrap_or_else(|| json!({})) })
}

/// One of a core's setups, by its id.
pub fn read(core: &str, id: &str) -> Option<Setup> {
    read_file(&dir(core).join(format!("{id}.json")), id.to_string())
}

/// The setup the settings say lights the device, if it is still there.
pub fn in_use(settings: &crate::settings::Settings, core: &str) -> Option<Setup> {
    read(core, settings.get(IN_USE)?.as_str()?)
}

/// Write a setup's file, by way of a temporary one so a crash mid-write
/// never leaves half a file. `device` is what the core calls its device.
pub fn write(core: &str, device: &str, s: &Setup) -> Result<(), String> {
    let folder = dir(core);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let text = format!(
        "// A light setup of your own for the {device}, made in Ectodeck's Light lab:\n\
         // its name, and what its light changes from the {core} core's. What it\n\
         // leaves out follows the core's; null takes out what the core has.\n\
         {{\n  \"name\": {},\n  \"light\": {}\n}}\n",
        json!(s.name),
        light_text(&s.changes)
    );
    let path = folder.join(format!("{}.json", s.id));
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| e.to_string())
}

/// What a setup changes, as its file writes it: each of the light's parts
/// on a line, each key set apart on a line of its own.
fn light_text(changes: &Value) -> String {
    let Some(parts) = changes.as_object().filter(|o| !o.is_empty()) else { return "{}".into() };
    let lines: Vec<String> = parts
        .iter()
        .map(|(k, v)| match v.as_object().filter(|keys| k == "keys" && !keys.is_empty()) {
            Some(keys) => {
                let keys: Vec<String> = keys.iter().map(|(id, own)| format!("      {}: {}", json!(id), jsonc::inline(own))).collect();
                format!("    \"keys\": {{\n{}\n    }}", keys.join(",\n"))
            }
            None => format!("    {}: {}", json!(k), jsonc::inline(v)),
        })
        .collect();
    format!("{{\n{}\n  }}", lines.join(",\n"))
}

/// Take a setup's file away.
pub fn remove(core: &str, id: &str) -> Result<(), String> {
    std::fs::remove_file(dir(core).join(format!("{id}.json"))).map_err(|e| e.to_string())
}

/// A setup's id from its name: its letters and digits, lower case, with a
/// dash between words; numbered on where another has it.
pub fn id_for(name: &str, taken: &[String]) -> String {
    let mut base = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_alphanumeric() {
            base.push(c);
        } else if !base.is_empty() && !base.ends_with('-') {
            base.push('-');
        }
    }
    let base = match base.trim_end_matches('-') {
        "" => "setup",
        b => b,
    };
    (1..).map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") }).find(|id| !taken.contains(id)).expect("a free id")
}

/// A core's light with a setup's changes made to it.
pub fn merged(core: &Value, changes: &Value) -> Value {
    match changes {
        Value::Object(ch) => {
            let mut out = core.as_object().cloned().unwrap_or_default();
            for (k, v) in ch {
                if v.is_null() {
                    out.remove(k);
                } else {
                    let m = merged(out.get(k).unwrap_or(&Value::Null), v);
                    out.insert(k.clone(), m);
                }
            }
            Value::Object(out)
        }
        v => v.clone(),
    }
}

/// What a light changes from a core's: what, merged into the core's, makes it.
pub fn changes(core: &Value, light: &Value) -> Value {
    let (Some(c), Some(l)) = (core.as_object(), light.as_object()) else { return light.clone() };
    let mut out = Map::new();
    for (k, v) in l {
        match c.get(k) {
            Some(cv) if jsonc::same(cv, v) => {}
            Some(cv) if cv.is_object() && v.is_object() => {
                let d = changes(cv, v);
                if d.as_object().is_some_and(|o| !o.is_empty()) {
                    out.insert(k.clone(), d);
                }
            }
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    for k in c.keys().filter(|k| !l.contains_key(*k)) {
        out.insert(k.clone(), Value::Null);
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A setup's changes, merged into the core's light, make the light they
    /// were taken from: values changed, parts added, and what the core has
    /// and the light does not taken out.
    #[test]
    fn a_setup_s_changes_make_its_light() {
        let core = json!({ "scale": 1.38, "caps": { "kind": "shine-through", "top": 14, "milky": 0 }, "keys": { "stop": { "caps": { "top": 6 } }, "keypad-plus": { "place": { "north": 4.5 } } } });
        let light = json!({ "scale": 1.38, "caps": { "kind": "pudding", "top": 14.0, "milky": 0.8 }, "keys": { "stop": { "caps": { "top": 6.0 } }, "space": { "caps": { "kind": "shine-through" } } } });
        let ch = changes(&core, &light);
        assert_eq!(ch, json!({ "caps": { "kind": "pudding", "milky": 0.8 }, "keys": { "space": { "caps": { "kind": "shine-through" } }, "keypad-plus": null } }), "only what differs (14 and 14.0 alike)");
        let back = merged(&core, &ch);
        assert_eq!(changes(&back, &light), json!({}), "the light again");
        assert!(back["keys"].get("keypad-plus").is_none(), "taken out where the light has none");
        assert_eq!(merged(&core, &json!({})), core, "no changes: the core's own");
    }

    #[test]
    fn a_setup_s_id_comes_from_its_name() {
        assert_eq!(id_for("Pudding caps", &[]), "pudding-caps");
        assert_eq!(id_for("  Pudding caps (clear spacebar)! ", &[]), "pudding-caps-clear-spacebar");
        assert_eq!(id_for("Pudding caps", &["pudding-caps".into()]), "pudding-caps-2");
        assert_eq!(id_for("???", &[]), "setup");
    }

    /// Its file reads back as written, comments and all.
    #[test]
    fn a_setup_s_file_reads_back() {
        let ch = json!({ "caps": { "kind": "pudding", "opacity": 0.8 }, "keys": { "space": { "caps": { "kind": "shine-through" } }, "w": null } });
        let text = format!("{{ \"name\": \"Pudding caps\", \"light\": {} }}", light_text(&ch));
        assert_eq!(jsonc::parse(&text).unwrap()["light"], ch);
        assert!(light_text(&ch).contains("\n      \"space\": { \"caps\": { \"kind\": \"shine-through\" } },\n"), "a key a line");
        assert_eq!(light_text(&json!({})), "{}");
    }
}
