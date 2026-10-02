//! Text measured as a browser measures it: shaped with the fonts the page
//! uses, kerning and all, so what the window lays out in Rust (chips that
//! wrap, notes that run to two lines) lands where the web page put it.

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

/// How wide `s` is in Geist at this weight and size, in pixels.
pub fn width(weight: u16, size: f32, s: &str) -> f32 {
    let (_, data) = faces().iter().min_by_key(|(w, _)| w.abs_diff(weight)).expect("a face");
    let Some(face) = rustybuzz::Face::from_slice(data, 0) else { return 0.0 };
    let mut buf = rustybuzz::UnicodeBuffer::new();
    buf.push_str(s);
    let glyphs = rustybuzz::shape(&face, &[], buf);
    let adv: i32 = glyphs.glyph_positions().iter().map(|p| p.x_advance).sum();
    adv as f32 * size / face.units_per_em() as f32
}

/// Lines of `s` broken at spaces to fit `max` pixels.
pub fn wrap(weight: u16, size: f32, s: &str, max: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in s.split(' ') {
        let next = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if !line.is_empty() && width(weight, size, &next) > max {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = next;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// A whole number with its thousands set apart by commas: 200,000.
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
