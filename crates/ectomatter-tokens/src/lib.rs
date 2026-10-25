//! Ectomatter's design tokens, read from a file in the Design Tokens Community
//! Group's format (<https://www.designtokens.org/tr/2025.10/format/>) and written
//! out as Slint and as Rust, so the window and the pages laid out in Rust draw
//! from one set of values.
//!
//! The file is groups of tokens, one level deep. Each group becomes a Slint
//! global and a Rust module of constants. A token is an object with a `$value`,
//! its `$type` set on it or on its group, and a `$description` if it needs one.
//! The types: `dimension` (in px), `color` (a hex string, or the format's color
//! object), `fontWeight`, `fontFamily` (a name, or a list whose first is used),
//! `number`, `duration` and `shadow` (its color, offsets and blur). A value can be
//! another token's, as `"{group.token}"`.
//!
//! A group can say, under `"$extensions": {"com.ectonomic.ectomatter": {..}}`,
//! the name of its Slint global (`"global"`, otherwise its own name capitalized)
//! and whether the app sets its tokens while it runs (`"settable": true`, as
//! Ink's are for a color mock), which makes them `in-out` and writes a setter.

use serde_json::{Map, Value};

/// The tokens written out: a Slint file of globals, and a Rust module.
pub struct Output {
    pub slint: String,
    pub rust: String,
}

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Px(f64),
    Color([u8; 4], bool),
    Weight(i64),
    Family(String),
    Number(f64),
    Ms(f64),
    // offset across, offset down, blur, and its color
    Shadow(f64, f64, f64, [u8; 4]),
}

struct Token {
    name: String,
    val: Val,
    about: Option<String>,
}

struct Group {
    key: String,
    global: String,
    settable: bool,
    about: Option<String>,
    tokens: Vec<Token>,
}

const EXT: &str = "com.ectonomic.ectomatter";
const HEAD: &str = "Ectomatter's design tokens, written by the build from design/ectomatter.tokens.json: change them there, not here.";

/// Reads the tokens file and writes it out as Slint and as Rust.
pub fn generate(json: &str) -> Result<Output, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| format!("the tokens file isn't JSON: {e}"))?;
    let root = root.as_object().ok_or("the tokens file isn't an object of groups")?;
    let groups = read(root)?;
    Ok(Output { slint: slint(&groups), rust: rust(&groups) })
}

fn read(root: &Map<String, Value>) -> Result<Vec<Group>, String> {
    let mut groups = Vec::new();
    for (key, g) in root {
        if key.starts_with('$') {
            continue;
        }
        check_name(key)?;
        let g = g.as_object().ok_or_else(|| format!("{key} isn't a group"))?;
        if g.contains_key("$value") {
            return Err(format!("{key} is a token outside any group: every token belongs to one"));
        }
        let ext = g.get("$extensions").and_then(|e| e.get(EXT));
        let global = match ext.and_then(|e| e.get("global")) {
            Some(n) => n.as_str().ok_or_else(|| format!("{key}'s global isn't a name"))?.to_string(),
            None => capitalized(key),
        };
        let settable = ext.and_then(|e| e.get("settable")).and_then(Value::as_bool).unwrap_or(false);
        let mut tokens = Vec::new();
        for (name, t) in g {
            if name.starts_with('$') {
                continue;
            }
            check_name(name)?;
            let at = format!("{key}.{name}");
            let val = value(root, &at, &mut Vec::new())?;
            let about = t.get("$description").and_then(Value::as_str).map(str::to_string);
            tokens.push(Token { name: name.clone(), val, about });
        }
        let about = g.get("$description").and_then(Value::as_str).map(str::to_string);
        groups.push(Group { key: key.clone(), global, settable, about, tokens });
    }
    Ok(groups)
}

// A token's value, `at` being "group.token", following references.
fn value(root: &Map<String, Value>, at: &str, seen: &mut Vec<String>) -> Result<Val, String> {
    if seen.iter().any(|s| s == at) {
        return Err(format!("{at} refers to itself, through {}", seen.join(" → ")));
    }
    seen.push(at.to_string());
    let (gk, tk) = at.split_once('.').ok_or_else(|| format!("{at} isn't group.token"))?;
    let group = root.get(gk).and_then(Value::as_object).ok_or_else(|| format!("there's no group {gk} (for {at})"))?;
    let token = group.get(tk).and_then(Value::as_object).ok_or_else(|| format!("there's no token {at}"))?;
    if token.keys().any(|k| !k.starts_with('$')) {
        return Err(format!("{at} is a group inside a group: groups go one level deep"));
    }
    let raw = token.get("$value").ok_or_else(|| format!("{at} has no $value"))?;
    if let Some(s) = raw.as_str()
        && let Some(to) = s.strip_prefix('{').and_then(|s| s.strip_suffix('}'))
    {
        return value(root, to, seen);
    }
    let ty = token
        .get("$type")
        .or_else(|| group.get("$type"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{at} has no $type, nor has its group"))?;
    let bad = |what: &str| format!("{at}'s value isn't {what}: {raw}");
    Ok(match ty {
        "dimension" => Val::Px(length(raw, "px").ok_or_else(|| bad("a length in px"))?),
        "duration" => Val::Ms(
            length(raw, "ms").or_else(|| length(raw, "s").map(|s| s * 1000.0)).ok_or_else(|| bad("a duration in ms or s"))?,
        ),
        "number" => Val::Number(raw.as_f64().ok_or_else(|| bad("a number"))?),
        "fontWeight" => Val::Weight(weight(raw).ok_or_else(|| bad("a font weight"))?),
        "fontFamily" => {
            let name = raw.as_str().or_else(|| raw.as_array().and_then(|a| a.first()).and_then(Value::as_str));
            Val::Family(name.ok_or_else(|| bad("a font's name"))?.to_string())
        }
        "color" => {
            let (c, alpha) = color(raw).ok_or_else(|| bad("a color"))?;
            Val::Color(c, alpha)
        }
        "shadow" => {
            let o = raw.as_object().ok_or_else(|| bad("a shadow"))?;
            let px = |k: &str| match o.get(k) {
                None => Some(0.0),
                Some(v) => length(v, "px"),
            };
            let (c, _) = o.get("color").and_then(color).ok_or_else(|| bad("a shadow with a color"))?;
            if px("spread") != Some(0.0) {
                return Err(format!("{at} has a spread, which Slint's shadows can't draw"));
            }
            match (px("offsetX"), px("offsetY"), px("blur")) {
                (Some(x), Some(y), Some(b)) => Val::Shadow(x, y, b, c),
                _ => return Err(bad("a shadow's offsets and blur in px")),
            }
        }
        other => return Err(format!("{at} is of type {other}, which Ectomatter doesn't draw with")),
    })
}

// a length as the format writes it, {"value": 24, "unit": "px"}, or as "24px"
fn length(raw: &Value, unit: &str) -> Option<f64> {
    if let Some(o) = raw.as_object() {
        return (o.get("unit")?.as_str()? == unit).then(|| o.get("value")?.as_f64()).flatten();
    }
    raw.as_str()?.strip_suffix(unit)?.trim().parse().ok()
}

fn weight(raw: &Value) -> Option<i64> {
    if let Some(n) = raw.as_i64() {
        return (1..=1000).contains(&n).then_some(n);
    }
    Some(match raw.as_str()? {
        "thin" | "hairline" => 100,
        "extra-light" | "ultra-light" => 200,
        "light" => 300,
        "normal" | "regular" | "book" => 400,
        "medium" => 500,
        "semi-bold" | "demi-bold" => 600,
        "bold" => 700,
        "extra-bold" | "ultra-bold" => 800,
        "black" | "heavy" => 900,
        "extra-black" | "ultra-black" => 950,
        _ => return None,
    })
}

// "#rgb", "#rrggbb" or "#rrggbbaa", or the format's object: its hex if it has
// one, otherwise its sRGB components and alpha
fn color(raw: &Value) -> Option<([u8; 4], bool)> {
    if let Some(o) = raw.as_object() {
        if let Some(h) = o.get("hex").and_then(Value::as_str) {
            let (c, _) = hex(h)?;
            let a = o.get("alpha").and_then(Value::as_f64);
            return Some(match a {
                Some(a) if a < 1.0 => ([c[0], c[1], c[2], (a * 255.0).round() as u8], true),
                _ => (c, false),
            });
        }
        if o.get("colorSpace")?.as_str()? != "srgb" {
            return None;
        }
        let parts = o.get("components")?.as_array()?;
        if parts.len() != 3 {
            return None;
        }
        let mut c = [0u8; 4];
        for (i, p) in parts.iter().enumerate() {
            c[i] = (p.as_f64()?.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        let a = o.get("alpha").and_then(Value::as_f64).unwrap_or(1.0);
        c[3] = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
        return Some((c, a < 1.0));
    }
    hex(raw.as_str()?)
}

fn hex(s: &str) -> Option<([u8; 4], bool)> {
    let h = s.strip_prefix('#')?;
    let digit = |i: usize| u8::from_str_radix(h.get(i..i + 1)?, 16).ok();
    let pair = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    Some(match h.len() {
        3 => ([digit(0)? * 17, digit(1)? * 17, digit(2)? * 17, 255], false),
        6 => ([pair(0)?, pair(2)?, pair(4)?, 255], false),
        8 => ([pair(0)?, pair(2)?, pair(4)?, pair(6)?], true),
        _ => return None,
    })
}

// a name both Slint and Rust can carry: lowercase letters, digits and hyphens,
// starting with a letter
fn check_name(n: &str) -> Result<(), String> {
    let ok = n.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !n.ends_with('-');
    if ok { Ok(()) } else { Err(format!("{n} isn't a token's name: lowercase letters, digits and hyphens")) }
}

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
}

fn number(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}

// a description as comment lines of at most 88 columns, each line begun with `lead`
fn comment(out: &mut String, lead: &str, text: &str) {
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && lead.len() + 1 + line.len() + 1 + word.len() > 88 {
            out.push_str(&format!("{lead} {line}\n"));
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push_str(&format!("{lead} {line}\n"));
    }
}

fn shadows(groups: &[Group]) -> bool {
    groups.iter().flat_map(|g| &g.tokens).any(|t| matches!(t.val, Val::Shadow(..)))
}

fn slint(groups: &[Group]) -> String {
    let mut out = String::new();
    comment(&mut out, "//", HEAD);
    if shadows(groups) {
        out.push_str("\n// a shadow as its tokens give it: its offsets, its blur and its color\n");
        out.push_str("export struct ShadowStyle {\n    x: length,\n    y: length,\n    blur: length,\n    color: color,\n}\n");
    }
    for g in groups {
        out.push('\n');
        if let Some(a) = &g.about {
            comment(&mut out, "//", a);
        }
        out.push_str(&format!("export global {} {{\n", g.global));
        let access = if g.settable { "in-out" } else { "out" };
        for t in &g.tokens {
            if let Some(a) = &t.about {
                comment(&mut out, "    //", a);
            }
            let (ty, v) = match &t.val {
                Val::Px(v) => ("length", format!("{}px", number(*v))),
                Val::Ms(v) => ("duration", format!("{}ms", number(*v))),
                Val::Number(v) => ("float", number(*v)),
                Val::Weight(w) => ("int", w.to_string()),
                Val::Family(f) => ("string", format!("{f:?}")),
                Val::Color(c, alpha) => ("color", hex_text(c, *alpha)),
                Val::Shadow(x, y, b, c) => (
                    "ShadowStyle",
                    format!("{{ x: {}px, y: {}px, blur: {}px, color: {} }}", number(*x), number(*y), number(*b), hex_text(c, c[3] != 255)),
                ),
            };
            out.push_str(&format!("    {access} property <{ty}> {}: {v};\n", t.name));
        }
        out.push_str("}\n");
    }
    out
}

fn hex_text(c: &[u8; 4], alpha: bool) -> String {
    if alpha {
        format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
    } else {
        format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
    }
}

fn rust(groups: &[Group]) -> String {
    let mut out = String::new();
    comment(&mut out, "//", HEAD);
    if shadows(groups) {
        out.push_str("\n/// a shadow as its tokens give it: its offsets, its blur and its color\n");
        out.push_str("#[allow(dead_code)]\n#[derive(Clone, Copy, Debug, PartialEq)]\npub struct ShadowStyle {\n    pub x: f32,\n    pub y: f32,\n    pub blur: f32,\n    pub color: [u8; 4],\n}\n");
    }
    for g in groups {
        out.push('\n');
        if let Some(a) = &g.about {
            comment(&mut out, "///", a);
        }
        out.push_str("#[allow(dead_code)]\n");
        out.push_str(&format!("pub mod {} {{\n", g.key.replace('-', "_")));
        for t in &g.tokens {
            if let Some(a) = &t.about {
                comment(&mut out, "    ///", a);
            }
            let name = t.name.to_ascii_uppercase().replace('-', "_");
            let (ty, v) = match &t.val {
                Val::Px(v) | Val::Number(v) | Val::Ms(v) => ("f32".to_string(), format!("{v:?}")),
                Val::Weight(w) => ("u16".to_string(), w.to_string()),
                Val::Family(f) => ("&str".to_string(), format!("{f:?}")),
                Val::Color(c, false) => ("[u8; 3]".to_string(), format!("[{:#04x}, {:#04x}, {:#04x}]", c[0], c[1], c[2])),
                Val::Color(c, true) => {
                    ("[u8; 4]".to_string(), format!("[{:#04x}, {:#04x}, {:#04x}, {:#04x}]", c[0], c[1], c[2], c[3]))
                }
                Val::Shadow(x, y, b, c) => (
                    "super::ShadowStyle".to_string(),
                    format!(
                        "super::ShadowStyle {{ x: {x:?}, y: {y:?}, blur: {b:?}, color: [{:#04x}, {:#04x}, {:#04x}, {:#04x}] }}",
                        c[0], c[1], c[2], c[3]
                    ),
                ),
            };
            out.push_str(&format!("    pub const {name}: {ty} = {v};\n"));
        }
        if g.settable {
            setter(&mut out, g);
        }
        out.push_str("}\n");
    }
    out
}

// for a group the app sets while it runs: its tokens' names, and a setter by name
fn setter(out: &mut String, g: &Group) {
    out.push_str("\n    /// every token's name, as the window's global has it\n");
    out.push_str("    pub const NAMES: &[&str] = &[");
    out.push_str(&g.tokens.iter().map(|t| format!("{:?}", t.name)).collect::<Vec<_>>().join(", "));
    out.push_str("];\n");
    let colors = g.tokens.iter().all(|t| matches!(t.val, Val::Color(..)));
    if !colors {
        return;
    }
    out.push_str(&format!(
        "\n    /// Sets the window's token by its name, saying whether there's one by that name.\n    pub fn set(global: &crate::{}<'_>, name: &str, c: slint::Color) -> bool {{\n        match name {{\n",
        g.global
    ));
    for t in &g.tokens {
        out.push_str(&format!("            {:?} => global.set_{}(c),\n", t.name, t.name.replace('-', "_")));
    }
    out.push_str("            _ => return false,\n        }\n        true\n    }\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r##"{
        "$description": "a test",
        "space": {
            "$type": "dimension",
            "$description": "the gutters",
            "margin": { "$value": { "value": 24, "unit": "px" }, "$description": "a page's margin" },
            "label": { "$value": "8px" },
            "half": { "$value": { "value": 13.5, "unit": "px" } },
            "card-gap": { "$value": "{space.margin}" }
        },
        "ink": {
            "$type": "color",
            "$extensions": { "com.ectonomic.ectomatter": { "global": "Ink", "settable": true } },
            "base": { "$value": "#18181b" },
            "hover-box": { "$value": { "colorSpace": "srgb", "components": [1, 0, 0] } },
            "scrim": { "$value": "#000000cc" }
        },
        "font": {
            "ui": { "$type": "fontFamily", "$value": ["Geist", "sans-serif"] },
            "strong": { "$type": "fontWeight", "$value": "semi-bold" },
            "glide": { "$type": "duration", "$value": { "value": 0.15, "unit": "s" } }
        },
        "shadow": {
            "$type": "shadow",
            "float": { "$value": { "color": "#00000080", "offsetX": "0px", "offsetY": { "value": 14, "unit": "px" }, "blur": "34px" } }
        }
    }"##;

    #[test]
    fn slint_has_a_global_per_group() {
        let out = generate(FILE).unwrap();
        assert!(out.slint.contains("export global Space {"), "{}", out.slint);
        assert!(out.slint.contains("    out property <length> margin: 24px;"));
        assert!(out.slint.contains("    out property <length> label: 8px;"));
        assert!(out.slint.contains("    out property <length> half: 13.5px;"));
        assert!(out.slint.contains("    out property <length> card-gap: 24px;"));
        assert!(out.slint.contains("export global Ink {"));
        assert!(out.slint.contains("    in-out property <color> base: #18181b;"));
        assert!(out.slint.contains("    in-out property <color> hover-box: #ff0000;"));
        assert!(out.slint.contains("    in-out property <color> scrim: #000000cc;"));
        assert!(out.slint.contains("export global Font {"));
        assert!(out.slint.contains("    out property <string> ui: \"Geist\";"));
        assert!(out.slint.contains("    out property <int> strong: 600;"));
        assert!(out.slint.contains("    out property <duration> glide: 150ms;"));
        assert!(out.slint.contains("    // a page's margin\n    out property <length> margin"));
        assert!(out.slint.contains("export struct ShadowStyle {"));
        assert!(out.slint.contains("    out property <ShadowStyle> float: { x: 0px, y: 14px, blur: 34px, color: #00000080 };"));
    }

    #[test]
    fn rust_has_a_module_per_group() {
        let out = generate(FILE).unwrap();
        assert!(out.rust.contains("pub mod space {"), "{}", out.rust);
        assert!(out.rust.contains("    pub const MARGIN: f32 = 24.0;"));
        assert!(out.rust.contains("    pub const HALF: f32 = 13.5;"));
        assert!(out.rust.contains("    pub const CARD_GAP: f32 = 24.0;"));
        assert!(out.rust.contains("    pub const BASE: [u8; 3] = [0x18, 0x18, 0x1b];"));
        assert!(out.rust.contains("    pub const SCRIM: [u8; 4] = [0x00, 0x00, 0x00, 0xcc];"));
        assert!(out.rust.contains("    pub const STRONG: u16 = 600;"));
        assert!(out.rust.contains("    pub const UI: &str = \"Geist\";"));
        assert!(out.rust.contains("pub const NAMES: &[&str] = &[\"base\", \"hover-box\", \"scrim\"];"));
        assert!(out.rust.contains("pub fn set(global: &crate::Ink<'_>, name: &str, c: slint::Color) -> bool"));
        assert!(out.rust.contains("\"hover-box\" => global.set_hover_box(c),"));
        assert!(out.rust.contains("pub const FLOAT: super::ShadowStyle = super::ShadowStyle { x: 0.0, y: 14.0, blur: 34.0, color: [0x00, 0x00, 0x00, 0x80] };"));
    }

    #[test]
    fn a_bad_file_says_what_is_wrong() {
        let loop_file = r#"{"space": {"$type": "dimension", "a": {"$value": "{space.b}"}, "b": {"$value": "{space.a}"}}}"#;
        assert!(generate(loop_file).err().unwrap().contains("refers to itself"));
        let no_type = r#"{"space": {"a": {"$value": "4px"}}}"#;
        assert!(generate(no_type).err().unwrap().contains("no $type"));
        let bad_name = r#"{"space": {"$type": "dimension", "Margin": {"$value": "4px"}}}"#;
        assert!(generate(bad_name).err().unwrap().contains("isn't a token's name"));
        let deep = r#"{"space": {"$type": "dimension", "inner": {"a": {"$value": "4px"}}}}"#;
        assert!(generate(deep).err().unwrap().contains("one level deep"));
        let wrong_unit = r#"{"space": {"$type": "dimension", "a": {"$value": {"value": 1, "unit": "rem"}}}}"#;
        assert!(generate(wrong_unit).err().unwrap().contains("a length in px"));
    }
}
