//! Text measured as a browser measures it: shaped with the fonts the page
//! uses, kerning and all, so what the window lays out in Rust (chips that
//! wrap, notes that run to two lines) lands where the web page put it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

static FACES: OnceLock<Vec<(u16, Vec<u8>)>> = OnceLock::new();

fn faces() -> &'static [(u16, Vec<u8>)] {
    FACES.get_or_init(|| {
        vec![
            (400, include_bytes!("../ui/fonts/Geist-400.ttf").to_vec()),
            (500, include_bytes!("../ui/fonts/Geist-500.ttf").to_vec()),
            (600, include_bytes!("../ui/fonts/Geist-600.ttf").to_vec()),
        ]
    })
}

/// The face nearest a weight, read once.
fn face(weight: u16) -> Option<&'static rustybuzz::Face<'static>> {
    static READ: OnceLock<Vec<(u16, Option<rustybuzz::Face<'static>>)>> = OnceLock::new();
    let read = READ.get_or_init(|| faces().iter().map(|(w, data)| (*w, rustybuzz::Face::from_slice(data, 0))).collect());
    read.iter().min_by_key(|(w, _)| w.abs_diff(weight)).and_then(|(_, f)| f.as_ref())
}

thread_local! {
    /// what has been measured and broken into lines, kept, since a page lays
    /// out the same text again on every change (a slider dragged); let go
    /// all at once when there is much of it
    static WIDTHS: RefCell<HashMap<(u16, u32, String), f32>> = RefCell::new(HashMap::new());
    static LINES: RefCell<HashMap<(u16, u32, u32, String), Vec<String>>> = RefCell::new(HashMap::new());
}
const KEPT: usize = 4096;

fn shaped(weight: u16, size: f32, s: &str) -> f32 {
    let Some(face) = face(weight) else { return 0.0 };
    let mut buf = rustybuzz::UnicodeBuffer::new();
    buf.push_str(s);
    let glyphs = rustybuzz::shape(face, &[], buf);
    let adv: i32 = glyphs.glyph_positions().iter().map(|p| p.x_advance).sum();
    adv as f32 * size / face.units_per_em() as f32
}

/// Words cut to `most` wide, set at `weight` and `size`, an ellipsis where they were cut.
pub fn elide(s: &str, most: f32, weight: u16, size: f32) -> String {
    if width(weight, size, s) <= most {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while !out.is_empty() && width(weight, size, &format!("{}…", out.trim_end())) > most {
        out.pop();
    }
    format!("{}…", out.trim_end())
}

/// How wide `s` is in Geist at this weight and size, in pixels.
pub fn width(weight: u16, size: f32, s: &str) -> f32 {
    let key = (weight, size.to_bits(), s.to_string());
    if let Some(w) = WIDTHS.with(|k| k.borrow().get(&key).copied()) {
        return w;
    }
    let w = shaped(weight, size, s);
    WIDTHS.with(|k| {
        let mut k = k.borrow_mut();
        if k.len() >= KEPT {
            k.clear();
        }
        k.insert(key, w);
    });
    w
}

/// Lines of `s` broken at spaces to fit `max` pixels.
pub fn wrap(weight: u16, size: f32, s: &str, max: f32) -> Vec<String> {
    let key = (weight, size.to_bits(), max.to_bits(), s.to_string());
    if let Some(lines) = LINES.with(|k| k.borrow().get(&key).cloned()) {
        return lines;
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in s.split(' ') {
        let next = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if !line.is_empty() && shaped(weight, size, &next) > max {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = next;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    LINES.with(|k| {
        let mut k = k.borrow_mut();
        if k.len() >= KEPT {
            k.clear();
        }
        k.insert(key, lines.clone());
    });
    lines
}

/// A whole number with its thousands set apart by commas: 200,000.
/// Things in a sentence: "a", "a and b", "a, b and c".
pub fn list(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        n => format!("{} and {}", items[..n - 1].join(", "), items[n - 1]),
    }
}

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn measures_as_the_web_page_does() {
        // WebKit's widths of these, from the reference page (chip text = chip width - 24)
        for (s, web) in [("Whole keyboard", 95.92), ("F-keys", 40.52), ("Number row", 73.33), ("With the profile", 90.59)] {
            let w = super::width(400, 13.0, s);
            assert!((w - web).abs() < 0.6, "{s}: {w} against {web}");
        }
    }
}
