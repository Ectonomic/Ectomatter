//! The design tokens (design/ectomatter.tokens.json), as the build writes them
//! out for what Ectomatter lays out and draws in Rust: the Library's pages,
//! Settings', the grounds its pictures are drawn on. They hold the values the
//! window's Slint globals hold (`Space`, `Ink`), from the same file.

include!(concat!(env!("OUT_DIR"), "/tokens.rs"));

/// A token's color, for a piece the window draws where Rust says.
pub fn color(c: [u8; 3]) -> slint::Color {
    slint::Color::from_rgb_u8(c[0], c[1], c[2])
}

/// The design system's own check: nothing the window draws sets a value of its
/// own. Every color, type size, typeface, corner and shadow in ui/ comes from
/// the tokens, but a device's own art: the parts that draw a device's picture
/// (`ART`) and a line saying `// art:`; a corner under 4 px is a mark's (a
/// glyph's, a key's), and a computed one (half a part's height) is fine. In the
/// pages laid out in Rust, every text measured is measured in the type tokens'
/// sizes and every color set is a token's. (docs/design-system.md)
#[cfg(test)]
mod system {
    use std::path::Path;

    /// the parts that draw a device's own picture, in its own colors
    const ART: &[&str] = &["KeyBox", "MouseLayers"];

    fn slint_files() -> Vec<(String, String)> {
        let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        let mut out = vec![("app.slint".to_string(), std::fs::read_to_string(ui.join("app.slint")).unwrap())];
        for e in std::fs::read_dir(ui.join("system")).unwrap().flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "slint") {
                out.push((format!("system/{}", p.file_name().unwrap().to_string_lossy()), std::fs::read_to_string(&p).unwrap()));
            }
        }
        out
    }

    /// Each line of code that isn't art: its file, number, and code (comments and strings out).
    fn code_lines() -> Vec<(String, usize, String)> {
        let mut out = Vec::new();
        for (name, text) in slint_files() {
            let mut part = String::new();
            for (i, line) in text.lines().enumerate() {
                let t = line.trim_start();
                if let Some(rest) = t.strip_prefix("export component ").or_else(|| t.strip_prefix("component ")) {
                    part = rest.split_whitespace().next().unwrap_or("").to_string();
                }
                if ART.contains(&part.as_str()) || line.contains("// art:") {
                    continue;
                }
                let code = line.split("//").next().unwrap_or("");
                // (what's in quotes is words or a glyph's path, not a value)
                let code: String = code.split('"').step_by(2).collect::<Vec<_>>().join("\"\"");
                out.push((name.clone(), i + 1, code));
            }
        }
        out
    }

    fn px_values(s: &str) -> Vec<f32> {
        let mut out = Vec::new();
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i].is_ascii_digit() && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'.' || b[i - 1] == b'-')) {
                let start = i;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                if s[i..].starts_with("px") {
                    out.push(s[start..i].parse().unwrap_or(0.0));
                }
            }
            i += 1;
        }
        out
    }

    fn value_of<'a>(code: &'a str, prop: &str) -> Option<&'a str> {
        let at = code.find(&format!("{prop}:"))?;
        // (the property itself, not one whose name ends in it)
        if at > 0 && code.as_bytes()[at - 1].is_ascii_alphanumeric() || at > 0 && code.as_bytes()[at - 1] == b'-' {
            return None;
        }
        Some(code[at + prop.len() + 1..].split(';').next().unwrap_or(""))
    }

    #[test]
    fn colors_are_tokens() {
        let bad: Vec<String> = code_lines()
            .into_iter()
            .filter(|(_, _, c)| c.split(|ch: char| !(ch == '#' || ch.is_ascii_hexdigit())).any(|w| w.starts_with('#') && matches!(w.len(), 4 | 7 | 9)))
            .map(|(f, n, c)| format!("{f}:{n}: {}", c.trim()))
            .collect();
        assert!(bad.is_empty(), "colors set by hand, not from Ink:\n{}", bad.join("\n"));
    }

    #[test]
    fn type_is_tokens() {
        let mut bad = Vec::new();
        for (f, n, c) in code_lines() {
            if let Some(v) = value_of(&c, "font-size")
                && !px_values(v).is_empty()
            {
                bad.push(format!("{f}:{n}: {}", c.trim()));
            }
            if value_of(&c, "font-family").is_some_and(|v| v.contains("\"\"")) {
                bad.push(format!("{f}:{n}: {}", c.trim()));
            }
        }
        assert!(bad.is_empty(), "type set by hand, not from Font:\n{}", bad.join("\n"));
    }

    #[test]
    fn corners_are_tokens() {
        let mut bad = Vec::new();
        for (f, n, c) in code_lines() {
            for prop in ["border-radius", "border-top-left-radius", "border-top-right-radius", "border-bottom-left-radius", "border-bottom-right-radius"] {
                let Some(v) = value_of(&c, prop) else { continue };
                if v.contains("Radius.") || v.contains("radius") {
                    continue;
                }
                if px_values(v).iter().any(|&p| p >= 4.0) {
                    bad.push(format!("{f}:{n}: {}", c.trim()));
                }
            }
        }
        assert!(bad.is_empty(), "corners set by hand, not from Radius:\n{}", bad.join("\n"));
    }

    #[test]
    fn shadows_are_tokens() {
        let mut bad = Vec::new();
        for (f, n, c) in code_lines() {
            for prop in ["drop-shadow-blur", "drop-shadow-offset-x", "drop-shadow-offset-y", "drop-shadow-color"] {
                let Some(v) = value_of(&c, prop) else { continue };
                if !v.contains("Shadow.") && v.trim() != "0" && v.trim() != "transparent" {
                    bad.push(format!("{f}:{n}: {}", c.trim()));
                }
            }
        }
        assert!(bad.is_empty(), "shadows set by hand, not from Shadow:\n{}", bad.join("\n"));
    }

    /// the pages laid out in Rust, and the parts they're laid out with
    const LAID: &[&str] = &["boxes.rs", "described.rs", "display.rs", "ecto_settings.rs", "form.rs", "games_page.rs", "lighting.rs", "mouse.rs", "screen_edit.rs"];

    #[test]
    fn rust_layouts_measure_and_color_with_tokens() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut bad = Vec::new();
        for f in LAID {
            let text = std::fs::read_to_string(src.join(f)).unwrap();
            for (i, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                let measured = ["text::width(", "text::wrap("].iter().any(|m| code.split(m).skip(1).any(|rest| rest.starts_with(|c: char| c.is_ascii_digit())));
                let colored = code.contains("from_rgb_u8(0x") || code.contains("from_argb_u8(") && !code.contains("from_argb_u8((");
                if measured || colored {
                    bad.push(format!("{f}:{}: {}", i + 1, line.trim()));
                }
            }
        }
        assert!(bad.is_empty(), "measured or colored by hand, not from the tokens:\n{}", bad.join("\n"));
    }
}
