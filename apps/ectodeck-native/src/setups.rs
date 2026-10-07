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

/// The words beside a value in a setup's file, by the value's path from
/// the light down: its name and its value as the Light lab shows them.
pub type Note<'a> = &'a dyn Fn(&[String], &Value) -> Option<String>;

/// Write a setup's file, by way of a temporary one so a crash mid-write
/// never leaves half a file. `device` is what the core calls its device.
pub fn write(core: &str, device: &str, s: &Setup, note: Note) -> Result<(), String> {
    let folder = dir(core);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = folder.join(format!("{}.json", s.id));
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, file_text(device, s, note)).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| e.to_string())
}

/// A setup's file as a person reads it: what it is and how it works, at
/// its head; then its name, and each value it changes on a line of its
/// own with its name and its value in the lab beside it.
fn file_text(device: &str, s: &Setup, note: Note) -> String {
    let name = &s.name;
    let mut out = vec![format!("// {name}: a custom light setup for the {device}."), "//".to_string()];
    out.extend(comment(&format!("The Light lab writes this file each time you save {name}. The file holds only the settings {name} changes from Core; every setting not listed here uses Core's value. The note beside each value names the setting as the Light lab does, for example \"Keycaps › Haze: 80%\"."), ""));
    out.push("//".into());
    out.extend(comment(&format!("You can edit values here by hand; the changes apply the next time Ectodeck starts. Percentages are written from 0 to 1 (80% is 0.8), and lengths in millimetres. To switch back to Core, or to rename or delete {name}, use the setup menu at the top of the Light lab."), ""));
    out.push("{".into());
    match s.changes.as_object().filter(|o| !o.is_empty()) {
        None => {
            out.push(format!("  \"name\": {},  // the name shown in the setup menu", json!(name)));
            out.push(String::new());
            out.extend(comment(&format!("{name} has no changes from Core yet, so the keyboard looks the same as with Core. Change a setting in the Light lab and press Save to {name}; the change is written here."), "  "));
            out.push("  \"light\": {}".into());
        }
        Some(_) => {
            let file = json!({ "name": name, "light": s.changes });
            let top = |path: &[String], v: &Value| match path {
                [n] if n == "name" => Some("the name shown in the setup menu".to_string()),
                [l] if l == "light" => Some(format!("the settings {name} changes from Core")),
                [_, rest @ ..] => note(rest, v),
                [] => None,
            };
            object_lines(file.as_object().expect("an object"), &mut Vec::new(), 1, &top, &mut out);
        }
    }
    out.push("}".into());
    out.join("\n") + "\n"
}

/// Words as comment lines `indent` in, wrapped to keep within 80 columns.
fn comment(words: &str, indent: &str) -> Vec<String> {
    let most = 80 - indent.len() - 3;
    let mut lines = Vec::new();
    let mut line = String::new();
    for w in words.split(' ') {
        if !line.is_empty() && line.chars().count() + 1 + w.chars().count() > most {
            lines.push(format!("{indent}// {line}"));
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(w);
    }
    if !line.is_empty() {
        lines.push(format!("{indent}// {line}"));
    }
    lines
}

/// An object's members, `depth` in: each value on a line of its own, an
/// object's members on the lines after its own, and the notes of the
/// members of one object in one column.
fn object_lines(o: &Map<String, Value>, path: &mut Vec<String>, depth: usize, note: Note, out: &mut Vec<String>) {
    let pad = "  ".repeat(depth);
    let mut rows: Vec<(String, Option<String>, Vec<String>)> = Vec::new();
    for (k, (key, v)) in o.iter().enumerate() {
        let comma = if k + 1 < o.len() { "," } else { "" };
        path.push(key.clone());
        let said = note(path, v);
        match v.as_object().filter(|inner| !inner.is_empty()) {
            Some(inner) => {
                let mut lines = Vec::new();
                object_lines(inner, path, depth + 1, note, &mut lines);
                lines.push(format!("{pad}}}{comma}"));
                rows.push((format!("{pad}{}: {{", json!(key)), said, lines));
            }
            None => rows.push((format!("{pad}{}: {}{comma}", json!(key), jsonc::inline(v)), said, Vec::new())),
        }
        path.pop();
    }
    let column = rows.iter().filter(|r| r.1.is_some()).map(|r| r.0.chars().count()).max().unwrap_or(0) + 2;
    for (head, said, lines) in rows {
        out.push(match said {
            Some(said) => format!("{head:<column$}// {said}"),
            None => head,
        });
        out.extend(lines);
    }
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

    /// Its file reads back as written, each value on a line of its own
    /// with its note beside it, the notes of one object in one column; one
    /// that changes nothing says so.
    #[test]
    fn a_setup_s_file_reads_back() {
        let ch = json!({ "caps": { "kind": "pudding", "opacity": 0.8 }, "keys": { "space": { "caps": { "kind": "shine-through" } }, "w": null } });
        let note = |path: &[String], v: &Value| Some(format!("{} is {}", path.join("."), v));
        let s = Setup { id: "pudding-caps".into(), name: "Pudding caps".into(), changes: ch.clone() };
        let text = file_text("EVGA Z10 RGB", &s, &note);
        let read = jsonc::parse(&text).unwrap();
        assert_eq!((read["name"].as_str(), &read["light"]), (Some("Pudding caps"), &ch));
        assert!(text.contains("\n      \"kind\": \"pudding\",  // caps.kind is \"pudding\"\n      \"opacity\": 0.8      // caps.opacity is 0.8\n"), "a value a line, the notes in one column:\n{text}");
        assert!(text.contains("\n      \"w\": null   // keys.w is null\n"), "{text}");
        assert!(text.lines().all(|l| l.chars().count() <= 80 || !l.trim_start().starts_with("//")), "comments within 80 columns");
        let empty = file_text("EVGA Z10 RGB", &Setup { id: "studio".into(), name: "Studio".into(), changes: json!({}) }, &note);
        assert_eq!(jsonc::parse(&empty).unwrap()["light"], json!({}));
        assert!(empty.starts_with("// Studio: a custom light setup for the EVGA Z10 RGB.\n") && empty.contains("Studio has no changes from Core yet"), "{empty}");
    }
}
