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
    const ART: &[&str] = &["KeyBox", "MouseLayers", "WayGlyph", "BlockGlyph", "MacroIcon"];

    /// Every Slint file under ui/, by its path there (app.slint first).
    pub(crate) fn slint_files() -> Vec<(String, String)> {
        fn walk(dir: &Path, rel: &str, out: &mut Vec<(String, String)>) {
            let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).collect();
            entries.sort();
            for p in entries {
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                let at = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
                if p.is_dir() {
                    walk(&p, &at, out);
                } else if p.extension().is_some_and(|x| x == "slint") {
                    out.push((at, std::fs::read_to_string(&p).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("ui"), "", &mut out);
        out.sort_by_key(|(name, _)| name != "app.slint");
        out
    }

    /// Whether a line is art: in a part that draws a device's picture, marked `// art:`,
    /// or between `// art: begin` and `// art: end` (a drawing of a part's own, drawn once:
    /// a glyph, an instrument's marks); `within` carries whether a region is open.
    fn is_art(part: &str, line: &str, within: &mut bool) -> bool {
        if line.contains("// art: begin") {
            *within = true;
        } else if line.contains("// art: end") {
            *within = false;
            return true;
        }
        ART.contains(&part) || line.contains("// art:") || *within
    }

    /// Each line of code that isn't art: its file, number, and code (comments and strings out).
    fn code_lines() -> Vec<(String, usize, String)> {
        let mut out = Vec::new();
        for (name, text) in slint_files() {
            let mut part = String::new();
            let mut within = false;
            for (i, line) in text.lines().enumerate() {
                let t = line.trim_start();
                if let Some(rest) = t.strip_prefix("export component ").or_else(|| t.strip_prefix("component ")) {
                    part = rest.split_whitespace().next().unwrap_or("").to_string();
                }
                if is_art(&part, line, &mut within) {
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

    /// The most lines the checks may skip for being marked art (`// art:` and
    /// the regions it opens; the parts in `ART` aside). A mark is for a
    /// device's own drawing, not a way around the system: a page's row, a tab
    /// or a card drawn by hand is a part to make. Lower it as marks go; it is
    /// raised only with Grant's word.
    const ART_LINES_AT_MOST: usize = 501;

    /// How many lines are skipped for being marked art, and where the most are.
    fn marked_art() -> (usize, Vec<String>) {
        let mut total = 0;
        let mut by_file = Vec::new();
        for (name, text) in slint_files() {
            let mut within = false;
            let n = text.lines().filter(|line| is_art("", line, &mut within)).count();
            total += n;
            if n > 0 {
                by_file.push(format!("{name}: {n}"));
            }
        }
        (total, by_file)
    }

    #[test]
    fn art_marks_are_capped() {
        let (total, by_file) = marked_art();
        assert!(total <= ART_LINES_AT_MOST, "{total} lines are marked art, more than the {ART_LINES_AT_MOST} allowed:\n  {}", by_file.join("\n  "));
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
    fn sizes_are_tokens() {
        // (every size, place and line's width on the pages and in the parts is a token's,
        // a part's or worked out from them; a part's own drawing is marked art. A 1 px
        // that only turns a number into a length, or back, isn't a size.)
        // (an instrument's own marks, drawn once, sit between `// drawn: begin` and
        // `// drawn: end`: the playlist's strip, the mouse's picture, Home's grips)
        let mut drawn = std::collections::HashSet::new();
        for (name, text) in slint_files() {
            let mut within = false;
            for (i, line) in text.lines().enumerate() {
                if line.contains("// drawn: begin") {
                    within = true;
                }
                if within {
                    drawn.insert((name.clone(), i + 1));
                }
                if line.contains("// drawn: end") {
                    within = false;
                }
            }
        }
        let mut bad = Vec::new();
        for (f, n, c) in code_lines() {
            if drawn.contains(&(f.clone(), n)) {
                continue;
            }
            let unit = c.replace("/ 1px", "").replace("* 1px", "").replace("(1px,", "(").replace("max(1px", "max(").replace("(0px", "(");
            if px_values(&unit).iter().any(|&v| v != 0.0) {
                bad.push(format!("{f}:{n}: {}", c.trim()));
            }
        }
        assert!(bad.is_empty(), "{} sizes set by hand, not from the tokens:\n{}", bad.len(), bad.join("\n"));
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

    /// The pages' own lines (every Slint file but the system's parts, under
    /// ui/system/): their file, number, the line as written, and its code
    /// (comments out), art and lines marked `// art:` left out.
    fn page_lines() -> Vec<(String, usize, String, String)> {
        let mut out = Vec::new();
        for (name, text) in slint_files().into_iter().filter(|(name, _)| !name.starts_with("system/")) {
            let mut part = String::new();
            let mut within = false;
            for (i, line) in text.lines().enumerate() {
                let t = line.trim_start();
                if let Some(rest) = t.strip_prefix("export component ").or_else(|| t.strip_prefix("component ")) {
                    part = rest.split_whitespace().next().unwrap_or("").to_string();
                }
                if is_art(&part, line, &mut within) {
                    continue;
                }
                out.push((name.clone(), i + 1, line.to_string(), line.split("//").next().unwrap_or("").to_string()));
            }
        }
        out
    }

    #[test]
    fn page_gaps_are_tokens() {
        let mut bad = Vec::new();
        for (f, n, line, code) in page_lines() {
            for prop in ["padding", "padding-left", "padding-right", "padding-top", "padding-bottom", "spacing"] {
                let Some(v) = value_of(&code, prop) else { continue };
                if px_values(v).iter().any(|&p| p > 0.0) {
                    bad.push(format!("{f}:{n}: {}", line.trim()));
                }
            }
        }
        assert!(bad.is_empty(), "gaps set by hand on a page, not from Space (or a part's):\n{}", bad.join("\n"));
    }

    #[test]
    fn pages_use_the_parts() {
        // (a line to type in is a FieldBox, a RenameField or a cell's; a dropdown's or a
        // menu's chevron is the DropField's or the WordButton's)
        const CHEVRONS: &[&str] = &["M3 4.5l3 3 3-3", "M1 1l4 4 4-4", "M2 3.5l3 3 3-3", "M6 9l6 6 6-6"];
        let mut bad = Vec::new();
        for (f, n, line, code) in page_lines() {
            if code.contains("TextInput {") || CHEVRONS.iter().any(|c| line.contains(c)) {
                bad.push(format!("{f}:{n}: {}", line.trim()));
            }
        }
        assert!(bad.is_empty(), "a field or a dropdown drawn on a page, not the system's part:\n{}", bad.join("\n"));
    }

    /// the pages laid out in Rust, and the parts they're laid out with
    const LAID: &[&str] = &["boxes.rs", "described.rs", "display.rs", "ecto_settings.rs", "form.rs", "games_page.rs", "lab.rs", "lighting.rs", "menu_size.rs", "mouse.rs", "screen_edit.rs"];

    /// The floats written on a line of code, as numbers.
    fn floats(code: &str) -> Vec<f32> {
        let mut out = Vec::new();
        let b = code.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i].is_ascii_digit() && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b'.')) {
                let start = i;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                let lit = &code[start..i];
                if lit.contains('.') && !code[i..].starts_with(|c: char| c.is_ascii_alphabetic() && c != 'f') {
                    out.push(lit.parse().unwrap_or(0.0));
                }
            }
            i += 1;
        }
        out
    }

    #[test]
    fn rust_layouts_place_with_tokens() {
        // (where a piece is placed or a layout steps on, no number of its own: a token, a
        // part's measure or a named measure of the page's, said once)
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let places = |code: &str| code.contains("cell(") || code.contains("text_cell(") || code.contains("gap(") || ["y +=", "y -=", "x +=", "x -=", ".y =", ".x ="].iter().any(|p| code.contains(p));
        let mut bad = Vec::new();
        for f in LAID {
            let text = std::fs::read_to_string(src.join(f)).unwrap();
            for (i, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("#[cfg(test)]") {
                    break;
                }
                let code = line.split("//").next().unwrap_or("");
                let code: String = code.split('"').step_by(2).collect::<Vec<_>>().join("\"\"");
                if line.contains("// art:") || code.trim_start().starts_with("const ") || !places(&code) {
                    continue;
                }
                if floats(&code).iter().any(|&v| v >= 3.0) {
                    bad.push(format!("{f}:{}: {}", i + 1, line.trim()));
                }
            }
        }
        assert!(bad.is_empty(), "placed with a number of its own, not a token:\n{}", bad.join("\n"));
    }

    #[test]
    fn menus_open_by_one_rule() {
        // (a menu's width is set where each window opens its menus, by src/menu_size.rs's
        // rule, never typed: no opener hands a menu a number for its width or its field's)
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let openers = ["fn hang_menu_most(", "fn open_menu_on(", "fn place_menu(", "fn fit_menu_width(", "fn show_rows(", "fn show_menus("];
        let mut bad = Vec::new();
        for entry in std::fs::read_dir(&src).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let text = std::fs::read_to_string(&path).unwrap();
            let mut within = "";
            for (i, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("#[cfg(test)]") {
                    break;
                }
                let code = line.split("//").next().unwrap_or("");
                if let Some(f) = openers.iter().find(|f| code.contains(**f)) {
                    within = f;
                } else if code.trim_start().starts_with("fn ") || code.trim_start().starts_with("pub fn ") {
                    within = "";
                }
                let typed = |call: &str| code.split(call).skip(1).any(|rest| rest.split(',').skip(1).take(2).chain(rest.split(')').take(1)).any(|a| a.trim().parse::<f32>().is_ok_and(|v| v != 0.0)));
                if code.contains("set_menu_w(") && (within.is_empty() || typed("set_menu_w(")) {
                    bad.push(format!("{name}:{}: {}", i + 1, line.trim()));
                }
                if ["hang_menu(", "hang_menu_most(", "place_menu(", "open_menu("].iter().any(|c| !code.contains(&format!("fn {c}")) && typed(c)) {
                    bad.push(format!("{name}:{}: {}", i + 1, line.trim()));
                }
            }
        }
        assert!(bad.is_empty(), "a menu's width typed, not the menus' rule:\n{}", bad.join("\n"));
    }

    #[test]
    fn cells_placed_are_drawn() {
        // (every kind of cell placed, by name from Rust or by number from a page, is one
        // ui/system/cells.slint draws, and lighting.rs names each kind it draws and no
        // other: the lab's sliders went blank, with nothing failing, when their kind was
        // taken out of the renderer as placed by no page, 2026-10-25)
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let number = |s: &str| s.trim_start().split(|c: char| !c.is_ascii_digit()).next().and_then(|n| n.parse::<i32>().ok());
        // what the renderer draws: each `root.c.kind == N`, each run `root.c.kind >= A && root.c.kind <= B`
        let mut drawn = std::collections::BTreeSet::new();
        for line in std::fs::read_to_string(root.join("ui/system/cells.slint")).unwrap().lines() {
            let code = line.split("//").next().unwrap_or("");
            drawn.extend(code.split("root.c.kind == ").skip(1).filter_map(number));
            for rest in code.split("root.c.kind >= ").skip(1) {
                if let (Some(lo), Some(hi)) = (number(rest), rest.split("root.c.kind <= ").nth(1).and_then(number)) {
                    drawn.extend(lo..=hi);
                }
            }
        }
        // the kinds by name, as lighting.rs's `mod kind` has them
        let lighting = std::fs::read_to_string(root.join("src/lighting.rs")).unwrap();
        let block = lighting.split("pub mod kind {").nth(1).and_then(|b| b.split("\n}").next()).unwrap();
        let named: Vec<(&str, i32)> = block.lines().filter_map(|l| {
            let (name, n) = l.trim().strip_prefix("pub const ")?.split_once(": i32 = ")?;
            Some((name, n.trim_end_matches(';').parse().ok()?))
        }).collect();
        let mut bad = Vec::new();
        for (name, n) in &named {
            if !drawn.contains(n) {
                bad.push(format!("lighting.rs: kind::{name} ({n}), which cells.slint doesn't draw"));
            }
        }
        for n in &drawn {
            if !named.iter().any(|(_, m)| m == n) {
                bad.push(format!("cells.slint: kind {n}, which lighting.rs doesn't name"));
            }
        }
        // what a page places itself: a cell's `c: { kind: N, ... }`
        for (name, text) in slint_files() {
            for (i, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                for rest in code.split("c: {").skip(1) {
                    if let Some(n) = rest.trim_start().strip_prefix("kind:").and_then(number) {
                        if !drawn.contains(&n) {
                            bad.push(format!("{name}:{}: kind {n}, which cells.slint doesn't draw", i + 1));
                        }
                    }
                }
            }
        }
        assert!(!drawn.is_empty() && !named.is_empty(), "the renderer's kinds and lighting.rs's names read");
        assert!(bad.is_empty(), "a cell's kind placed or named that the renderer doesn't draw:\n{}", bad.join("\n"));
    }

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
