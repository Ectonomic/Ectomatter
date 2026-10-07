//! The Z10's Settings tab, as the web app had it (renderDisplay in app.js at
//! cd9ea82): the profile's name, its screen's colour and brightness, the
//! screens the keyboard shows by itself and the keys game mode blocks; what
//! each slider controls; and what the keyboard's firmware adds.

use serde_json::{json, Value};

use crate::lighting::{cell, kind, Col, Ctl, Part};
use crate::z10::Page;
use crate::Cell;

pub const LCD_APPS: [(u64, &str); 5] = [(0x20, "Clock"), (0x04, "Profile number"), (0x08, "Keyboard mode"), (0x01, "Rollover setting"), (0x02, "EVGA Precision X1 stats")];
pub const GAME_KEYS: [(u64, &str); 7] = [(0x40, "Windows key"), (0x01, "Alt+Tab"), (0x08, "Alt+F4"), (0x04, "Ctrl+Esc"), (0x02, "Tab"), (0x10, "Left Shift"), (0x20, "Menu key")];

/// What each slider can control. "own" is what the keyboard does by itself.
pub const BIND_CHOICES: [[(&str, &str); 4]; 2] = [
    [("own", "Keyboard brightness"), ("output", "System volume"), ("microphone", "Microphone volume"), ("none", "Nothing")],
    [("own", "System volume"), ("backlight", "Keyboard brightness"), ("microphone", "Microphone volume"), ("none", "Nothing")],
];

/// What a slider really drives: left on its own job, the dimmer drives the
/// backlight and the volume slider the computer's volume.
pub fn drives(i: usize, v: &str) -> String {
    if v == "own" { if i == 0 { "backlight".into() } else { "output".into() } } else { v.into() }
}

pub fn has(fw: &Value, feature: &str) -> bool {
    fw["features"].as_array().is_some_and(|a| a.iter().any(|f| f["id"] == feature))
}

pub struct Tiles {
    pub cells: Vec<Cell>,
    pub height: f32,
}

/// The tab's tiles, placed from its top.
pub fn tiles(page: &Page, fw: &Value, bind: &[String; 2]) -> Tiles {
    let mut cells = Vec::new();
    let l = &page.draft()["lighting"];
    let (px, pad) = (24.0, 20.0);
    // three columns, 1.3 to 1 to 1, 48 apart
    let unit = (1216.0 - 96.0) / 3.3;
    let (w0, w1) = (unit * 1.3, unit);
    let mut prof = Col::sized(px, w0, "Profile");
    let name = page.draft()["name"].as_str().unwrap_or("").to_string();
    prof.row("Name", vec![Part::Field(Ctl::Name, name), Part::Note("Stored on the keyboard with this profile.".into())]);
    let lc = &l["lcd"]["color"];
    let c = &lc["color"];
    prof.row("Screen color", vec![Part::Swatches(Ctl::LcdColor, vec![slint::Color::from_rgb_u8(c[0].as_u64().unwrap_or(0) as u8, c[1].as_u64().unwrap_or(0) as u8, c[2].as_u64().unwrap_or(0) as u8)])]);
    let b = lc["brightness"].as_f64().unwrap_or(255.0);
    prof.row("Brightness", vec![Part::Slider(Ctl::LcdBrightness, (b / 255.0) as f32, format!("{}%", (b / 2.55).round()))]);
    let apps = l["lcd"]["apps"].as_u64().unwrap_or(0);
    let mut screens = Col::sized(px + w0 + 48.0, w1, "Built-in screens");
    screens.checks(LCD_APPS.iter().enumerate().map(|(i, (bit, n))| (Ctl::LcdApp, i, apps & bit != 0, n.to_string())).collect());
    screens.para("Screens the keyboard shows by itself. The stats service draws over them while it runs.");
    let blocked = !l["game_keys"].as_u64().unwrap_or(0) & 0x7f;
    let mut game = Col::sized(px + w0 + w1 + 96.0, w1, "Blocked in game mode");
    game.checks(GAME_KEYS.iter().enumerate().map(|(i, (bit, n))| (Ctl::GameKey, i, blocked & bit != 0, n.to_string())).collect());
    game.para("These keys do nothing while game mode is on.");
    let h = prof.height().max(screens.height()).max(game.height());
    let mut y = 0.0;
    let mut tile = |cells: &mut Vec<Cell>, cols: Vec<Col>, h: f32, y: &mut f32| {
        cells.push(cell(kind::TILE, 0.0, *y, 1264.0, pad * 2.0 + h));
        for col in cols {
            for mut c in col.cells {
                c.y += *y + pad;
                cells.push(c);
            }
        }
        *y += pad * 2.0 + h + 14.0;
    };
    tile(&mut cells, vec![prof, screens, game], h, &mut y);

    // the sliders, on firmware that can say where they are
    if has(fw, "sliders.read") {
        let mut sl = Col::sized(px, 1216.0, "Sliders");
        for (i, name) in ["Dimmer", "Volume"].iter().enumerate() {
            let cur = BIND_CHOICES[i].iter().find(|(v, _)| *v == bind[i]).unwrap_or(&BIND_CHOICES[i][0]);
            let mut parts = vec![Part::Dropdown(Ctl::SliderBind, i, cur.1.to_string())];
            if i == 1 && bind[1] != "own" && !has(fw, "volume.quiet") {
                parts.push(Part::Note("The keyboard still sends volume keys from this slider as well. Its firmware can be told not to, from build 3.".into()));
            }
            if i == 0 && bind[0] != "own" {
                parts.push(Part::Note(
                    if has(fw, "dimmer.keep") {
                        "The backlight stays where it was, and follows the volume slider if that is on keyboard brightness."
                    } else {
                        "The backlight stays where it was, though it may flicker as this slider moves. Its firmware can hold it steady, from build 3."
                    }
                    .into(),
                ));
            }
            sl.row(name, parts);
        }
        // the note is a column of its own, 8 under the rows
        let mut note = Col::sized(px, 1216.0, "");
        note.y = sl.height() + 8.0 + 16.0;
        note.para("Move a slider and its knob on the keyboard above moves with it. What each one controls is Ectodeck’s, not the keyboard’s, so it is the same on every profile and works while Ectodeck is open.");
        let h = note.height();
        tile(&mut cells, vec![sl, note], h, &mut y);
    }

    // custom firmware, apart from everything EVGA's does
    let mut f = Col::sized(px, 1216.0, "Custom firmware");
    match fw["kind"].as_str().unwrap_or("unknown") {
        "custom" => {
            f.row("Firmware", vec![Part::Value(format!("Ectodeck v{}, build {}", fw["version"].as_str().unwrap_or(""), fw["build"]))]);
            let adds: Vec<String> = fw["features"].as_array().map(|a| a.iter().filter_map(|x| x["name"].as_str().map(String::from)).collect()).unwrap_or_default();
            if adds.is_empty() {
                f.row("Adds", vec![Part::Note("Nothing Ectodeck controls yet. This build fixes how the volume slider steps.".into())]);
            } else {
                f.row("Adds", vec![Part::List(adds)]);
            }
            let later: Vec<String> = fw["later"].as_array().map(|a| a.iter().map(|x| format!("{} (build {})", x["name"].as_str().unwrap_or(""), x["since"])).collect()).unwrap_or_default();
            if !later.is_empty() {
                f.row("In later builds", vec![Part::List(later)]);
            }
        }
        "retail" => {
            f.row("Firmware", vec![Part::Value(format!("EVGA v{}", fw["version"].as_str().unwrap_or("")))]);
            f.para("Features that need Ectodeck’s custom firmware show here when the keyboard runs it.");
        }
        _ => {
            let v = fw["version"].as_str().unwrap_or("");
            f.row("Firmware", vec![Part::Value(if v.is_empty() { "Not known".into() } else { format!("v{v}") })]);
            f.para("This version is neither EVGA’s nor Ectodeck’s, so no custom features are offered.");
        }
    }
    let h = f.height();
    tile(&mut cells, vec![f], h, &mut y);
    Tiles { cells, height: y - 14.0 }
}

/// Use a piece of the tab. `on` is a checkbox's new state; `index` which box.
pub fn act(page: &mut Page, c: Ctl, index: usize, value: f32, text: Option<&str>) {
    let n = page.editing;
    let Some(d) = page.drafts.get_mut(&n) else { return };
    let l = &mut d["lighting"];
    match c {
        Ctl::LcdBrightness => l["lcd"]["color"]["brightness"] = json!((value.clamp(0.0, 1.0) * 255.0).round() as u64),
        Ctl::LcdApp => {
            let bit = LCD_APPS.get(index).map_or(0, |a| a.0);
            let apps = l["lcd"]["apps"].as_u64().unwrap_or(0);
            l["lcd"]["apps"] = json!(if value > 0.5 { apps | bit } else { apps & !bit });
        }
        Ctl::GameKey => {
            let bit = GAME_KEYS.get(index).map_or(0, |a| a.0);
            let blocked = !l["game_keys"].as_u64().unwrap_or(0) & 0x7f;
            let b = if value > 0.5 { blocked | bit } else { blocked & !bit };
            l["game_keys"] = json!((!b & 0x7f) | 0x80);
        }
        Ctl::Name => {
            if let Some(t) = text {
                d["name"] = json!(t.chars().take(60).collect::<String>());
            }
        }
        _ => {}
    }
}
