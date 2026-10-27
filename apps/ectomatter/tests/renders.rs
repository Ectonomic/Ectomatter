//! Ectomatter's pages drawn offscreen on a made-up desk, each compared pixel
//! for pixel with the picture approved for it (`tests/renders/approved/`).
//!
//! The desk is the same on any computer and holds nothing of Grant's: the
//! keyboard's and the mouse's states in `tests/renders/desk/`, a computer
//! described in a folder (`ECTOMATTER_COMPUTER`: the sensors crate's made-up
//! computer, two made-up monitors, the weather's answers), a Steam library of
//! made-up games, fresh settings folders, no Wayland or D-Bus session, the
//! time of day fixed (`ECTOMATTER_NOW`) and the time zone UTC, and the
//! lights' effects and the screen's playlist each at a moment fixed. Two desks are
//! drawn: one set up (scenes, a place for the weather) and one as a stranger
//! first opens Ectomatter.
//!
//! A page that comes out otherwise fails the test, its picture and where it
//! differs written to `target/renders/`. When the change is meant, look at
//! those pictures, then write them over the approved ones:
//! `ECTOMATTER_APPROVE=1 cargo test --release -p ectomatter --test renders`.
//!
//! The packages are the ones built in this source folder
//! (`tools/build-cores.sh`, `tools/build-plugins.sh`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn here() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn repo() -> PathBuf {
    here().join("../..")
}

/// The time of day every page is drawn at: October 22, 2026, 14:00 UTC.
const NOW: u64 = 1_792_677_600;

/// A page to draw: its name, the window's size, and what's set for it (the
/// snapshot's own settings, main.rs `snap`). A name ending `.window` is a
/// window of its own (the lab, the editors), which writes its own picture.
struct Case {
    name: &'static str,
    size: (u32, u32),
    set: &'static [(&'static str, &'static str)],
}

const WIDE: (u32, u32) = (1440, 1028);
const NARROW: (u32, u32) = (1000, 700);

/// Every place of the main window, and the windows of their own, on the desk
/// that's set up.
const SET_UP: &[Case] = &[
    Case { name: "home", size: WIDE, set: &[("ECTOMATTER_PLACE", "home")] },
    Case { name: "home-edit", size: WIDE, set: &[("ECTOMATTER_PLACE", "home"), ("ECTOMATTER_DESK", "edit")] },
    Case { name: "kb-keys", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard")] },
    Case { name: "kb-light", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "358,63")] },
    Case { name: "kb-screen", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "440,63")] },
    Case { name: "kb-settings", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "522,63")] },
    Case { name: "kb-key", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "420,416")] },
    Case { name: "kb-menu", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "420,416;1174,875")] },
    Case { name: "kb-onboard", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "440,63;484,164")] },
    Case { name: "kb-gamelaunch", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "440,63;612,164")] },
    Case { name: "m-buttons", size: WIDE, set: &[("ECTOMATTER_PLACE", "mouse"), ("ECTOMATTER_MOUSE_TAB", "0")] },
    Case { name: "m-light", size: WIDE, set: &[("ECTOMATTER_PLACE", "mouse"), ("ECTOMATTER_MOUSE_TAB", "1")] },
    Case { name: "m-perf", size: WIDE, set: &[("ECTOMATTER_PLACE", "mouse"), ("ECTOMATTER_MOUSE_TAB", "2")] },
    Case { name: "m-settings", size: WIDE, set: &[("ECTOMATTER_PLACE", "mouse"), ("ECTOMATTER_MOUSE_TAB", "3")] },
    Case { name: "settings", size: WIDE, set: &[("ECTOMATTER_PLACE", "settings")] },
    Case { name: "settings-tall", size: (1440, 3600), set: &[("ECTOMATTER_PLACE", "settings")] },
    Case { name: "library", size: WIDE, set: &[("ECTOMATTER_PLACE", "library")] },
    Case { name: "library-games", size: WIDE, set: &[("ECTOMATTER_PLACE", "library"), ("ECTOMATTER_CLICKS", "296,63")] },
    Case { name: "library-apps", size: WIDE, set: &[("ECTOMATTER_PLACE", "library"), ("ECTOMATTER_CLICKS", "368,63")] },
    Case { name: "game", size: (1440, 1600), set: &[("ECTOMATTER_PLACE", "library"), ("ECTOMATTER_LIBRARY_PAGE", "steam_app_4000010"), ("ECTOMATTER_WAIT", "1500")] },
    Case { name: "scene-form", size: WIDE, set: &[("ECTOMATTER_PLACE", "home"), ("ECTOMATTER_SCENE_FORM", "new")] },
    Case { name: "n-kb-keys", size: NARROW, set: &[("ECTOMATTER_PLACE", "keyboard")] },
    Case { name: "n-kb-light", size: NARROW, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "358,63")] },
    Case { name: "n-m-buttons", size: NARROW, set: &[("ECTOMATTER_PLACE", "mouse"), ("ECTOMATTER_MOUSE_TAB", "0")] },
    Case { name: "n-m-light", size: NARROW, set: &[("ECTOMATTER_PLACE", "mouse"), ("ECTOMATTER_MOUSE_TAB", "1")] },
    Case { name: "n-settings", size: NARROW, set: &[("ECTOMATTER_PLACE", "settings")] },
    Case { name: "rail-home", size: WIDE, set: &[("ECTOMATTER_PLACE", "home"), ("ECTOMATTER_CLICKS", "30,66"), ("ECTOMATTER_WAIT", "900")] },
    Case { name: "rail-keyboard-to-mouse", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "30,166"), ("ECTOMATTER_WAIT", "900")] },
    Case { name: "rail-mouse-to-keyboard", size: WIDE, set: &[("ECTOMATTER_PLACE", "mouse"), ("ECTOMATTER_MOUSE_TAB", "0"), ("ECTOMATTER_CLICKS", "30,122"), ("ECTOMATTER_WAIT", "900")] },
    Case { name: "rail-library", size: WIDE, set: &[("ECTOMATTER_PLACE", "library"), ("ECTOMATTER_CLICKS", "30,954"), ("ECTOMATTER_WAIT", "900")] },
    Case { name: "rail-settings", size: WIDE, set: &[("ECTOMATTER_PLACE", "settings"), ("ECTOMATTER_CLICKS", "30,998"), ("ECTOMATTER_WAIT", "900")] },
    Case { name: "macro-editor.window", size: (1080, 620), set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_MACROS", "1"), ("ECTOMATTER_MACRO_LIB", "ectomatter")] },
    Case { name: "screen-editor.window", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_LAYOUTS", "0")] },
    Case { name: "lab.window", size: (1280, 800), set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_LAB", "1")] },
    Case { name: "lab-legends.window", size: (1280, 800), set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_LAB", "1"), ("ECTOMATTER_CLICKS", "958,70")] },
    Case { name: "lab-leds.window", size: (1280, 800), set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_LAB", "1"), ("ECTOMATTER_CLICKS", "1098,70")] },
    Case { name: "lab-wells.window", size: (1280, 800), set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_LAB", "1"), ("ECTOMATTER_CLICKS", "1155,70")] },
    Case { name: "lab-display.window", size: (1280, 800), set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_LAB", "1"), ("ECTOMATTER_CLICKS", "1210,70")] },
    Case { name: "lab-mouse.window", size: (1280, 800), set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_LAB", "mouse")] },
    Case { name: "lab-slid.window", size: (1280, 800), set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_LAB", "1"), ("ECTOMATTER_CLICKS", "100,416"), ("ECTOMATTER_HOVER", "600,700")] },
];

/// The main window's places on the desk of a stranger opening Ectomatter
/// for the first time.
const FIRST_RUN: &[Case] = &[
    Case { name: "first-home", size: WIDE, set: &[("ECTOMATTER_PLACE", "home")] },
    Case { name: "first-keyboard", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard")] },
    Case { name: "first-mouse", size: WIDE, set: &[("ECTOMATTER_PLACE", "mouse"), ("ECTOMATTER_MOUSE_TAB", "0")] },
    Case { name: "first-settings", size: (1440, 3600), set: &[("ECTOMATTER_PLACE", "settings")] },
    Case { name: "first-library", size: WIDE, set: &[("ECTOMATTER_PLACE", "library")] },
];

/// A folder of the test's own, empty, gone after it. Its path is the same on
/// every computer and every run (a page can show it: a game's prefix), so
/// two runs of these tests at once on one computer would share it.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let d = Path::new("/tmp/ectomatter-renders").join(name);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn put(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A picture of one color with a band of another across its middle, as a
/// made-up game's cover.
fn cover(path: &Path, (w, h): (u32, u32), back: [u8; 3], band: [u8; 3]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut px = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for _ in 0..w {
            px.extend_from_slice(if (h * 2 / 5..h * 3 / 5).contains(&y) { &band } else { &back });
        }
    }
    let mut e = png::Encoder::new(std::fs::File::create(path).unwrap(), w, h);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&px).unwrap();
}

/// The made-up games, by Steam number: their names and their covers' colors.
const GAMES: &[(u32, &str, [u8; 3], [u8; 3])] = &[
    (4000010, "Lantern Road", [38, 52, 84], [232, 176, 74]),
    (4000020, "Tidewater", [22, 86, 92], [210, 236, 228]),
    (4000030, "Copper Sky", [112, 48, 30], [246, 140, 70]),
];

/// A Steam library as Steam lays its folder out, with the made-up games
/// installed and their covers chosen in Steam's grid.
fn steam(root: &Path) {
    put(&root.join("steamapps/libraryfolders.vdf"), &format!("\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n", root.display()));
    // (the account 76561198000000001, which Steam keeps its grid under as 39734273)
    put(&root.join("config/loginusers.vdf"), "\"users\"\n{\n\t\"76561198000000001\"\n\t{\n\t\t\"AccountName\"\t\t\"stranger\"\n\t\t\"MostRecent\"\t\t\"1\"\n\t\t\"Timestamp\"\t\t\"1792600000\"\n\t}\n}\n");
    for (id, name, back, band) in GAMES {
        put(&root.join(format!("steamapps/appmanifest_{id}.acf")), &format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"{name}\"\n}}\n"));
        std::fs::create_dir_all(root.join(format!("steamapps/common/{name}"))).unwrap();
        cover(&root.join(format!("userdata/39734273/config/grid/{id}p.png")), (300, 450), *back, *band);
    }
}

/// The computer the packages and Ectomatter reach (computer::Described): the
/// sensors crate's made-up computer, two made-up monitors, its NVIDIA card's
/// readings, the weather's answers for a clear morning in Reykjavík, and the
/// cores' devices plugged in. (Its files don't change, so the processor's
/// load, which takes two different readings of /proc/stat, is unavailable.)
fn computer(dir: &Path) {
    ectomatter_sensors::fake::computer(&dir.join("files"));
    let nv = serde_json::json!({ "0000:01:00.0": {
        "probe": { "has": ["temp", "load", "clock", "vram", "mem-clock", "power", "fan"], "vram": 31.8 },
        "temp": 43.0, "load": 6.0, "clock": 1882.0, "mem-clock": 7001.0, "vram": [4.8, 31.8], "power": 73.4, "fan": 0.0 } });
    put(&dir.join("nvidia.json"), &nv.to_string());
    let monitors = serde_json::json!([
        { "id": "edid:AAA-0001-1", "name": "Wide Monitor", "maker": "Acme", "port": "DP-1", "enabled": true, "main": true,
          "modes": [{ "width": 3440, "height": 1440, "refresh": 144000 }, { "width": 2560, "height": 1440, "refresh": 60000 }],
          "mode": { "width": 3440, "height": 1440, "refresh": 144000 }, "hdr": true, "vrr": "automatic", "brightness": 8000, "ddc": true },
        { "id": "edid:BBB-0002-2", "name": "Side Monitor", "maker": "Acme", "port": "HDMI-A-1", "enabled": true, "main": false,
          "modes": [{ "width": 1920, "height": 1080, "refresh": 60000 }],
          "mode": { "width": 1920, "height": 1080, "refresh": 60000 }, "brightness": 5000, "ddc": false }
    ]);
    put(&dir.join("monitors.json"), &monitors.to_string());
    let web = serde_json::json!([
        { "starts": "https://api.open-meteo.com/v1/forecast", "answer": { "utc_offset_seconds": 0,
            "current": { "weather_code": 1, "cloud_cover": 20, "rain": 0, "showers": 0, "snowfall": 0, "wind_speed_10m": 14.0, "wind_direction_10m": 250, "wind_gusts_10m": 22.0, "visibility": 30000, "temperature_2m": 4.0 },
            "daily": { "sunrise": ["2026-10-22T09:03"], "sunset": ["2026-10-22T17:21"] } } },
        { "starts": "https://air-quality-api.open-meteo.com/", "answer": { "current": { "pm2_5": 3.0, "dust": 0.0, "aerosol_optical_depth": 0.05 } } },
        { "starts": "https://api.weather.gov/", "answer": { "features": [] } },
        { "starts": "https://geocoding-api.open-meteo.com/v1/search", "answer": { "results": [{ "id": 3413829, "name": "Reykjavík", "admin1": "Capital Region", "country": "Iceland", "country_code": "IS", "latitude": 64.13548, "longitude": -21.89541 }] } }
    ]);
    put(&dir.join("web.json"), &web.to_string());
    // every device a core that comes with Ectomatter drives, plugged in and
    // Ectomatter's to open, no other program holding it (their ids from the
    // cores' manifests)
    let mut devices = serde_json::Map::new();
    for e in std::fs::read_dir(repo().join("cores")).unwrap().flatten() {
        let Ok(text) = std::fs::read_to_string(e.path().join("core.json")) else { continue };
        let m = ectomatter_draw::jsonc::parse(&text).unwrap();
        for usb in m["matches"].as_array().into_iter().flatten().filter_map(|x| x.get("usb")) {
            let (Some(v), Some(p)) = (usb["vendor"].as_str(), usb["product"].as_str()) else { continue };
            devices.insert(format!("{}:{}", v.to_lowercase(), p.to_lowercase()), serde_json::json!({ "present": true, "reachable": true, "holders": [] }));
        }
    }
    assert!(!devices.is_empty(), "no core names a USB device");
    put(&dir.join("devices.json"), &serde_json::Value::Object(devices).to_string());
}

/// The settings of a desk that's set up: three scenes, and Reykjavík for
/// the weather.
fn set_up(config: &Path) {
    let settings = serde_json::json!({
        "scenes": [
            { "id": "scene-1", "name": "Late night", "lights": { "desk": { "effect": "static", "colors": ["#C46A1A"], "speed": 0.5, "reverse": false } }, "plugins": {} },
            { "id": "scene-2", "name": "Movie night", "lights": { "desk": { "effect": "breathing", "colors": ["#3A5BD9"], "speed": 0.3, "reverse": false } }, "plugins": {} },
            { "id": "scene-3", "name": "Focus", "lights": { "desk": { "effect": "wave", "colors": ["#F2F2F2", "#7FB2FF"], "speed": 0.5, "reverse": false } }, "plugins": {} }
        ],
        "plugins": { "weather-feed": { "place": { "id": 3413829, "name": "Reykjavík", "latin": "Reykjavik", "region": "Capital Region", "country": "Iceland", "country_code": "IS", "lat": 64.13548, "lon": -21.89541 } } }
    });
    put(&config.join("com.ectonomic.ectomatter/settings.json"), &serde_json::to_string_pretty(&settings).unwrap());
}

/// A picture as the eye sees it: 8-bit sRGB, red, green, blue and alpha.
struct Picture {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

impl Picture {
    fn read_png(path: &Path) -> Option<Picture> {
        let mut d = png::Decoder::new(std::fs::File::open(path).ok()?);
        d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA);
        let mut r = d.read_info().ok()?;
        let mut buf = vec![0; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).ok()?;
        buf.truncate(info.buffer_size());
        let rgba = match info.color_type {
            png::ColorType::Rgba => buf,
            png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
            png::ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
            _ => return None,
        };
        // (a window's picture is opaque; one read is held to it as one)
        let rgba = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        Some(Picture { w: info.width, h: info.height, rgba })
    }

    /// The main window's frame as the snapshot saves it (four 16-bit values
    /// a pixel, PQ-encoded, alpha premultiplied), shown at `white` nits as
    /// white, as the screen shows it.
    fn from_frame(bytes: &[u8], w: u32, h: u32, white: f64) -> Picture {
        let (m1, m2, c1, c2, c3) = (2610.0 / 16384.0, 2523.0 / 4096.0 * 128.0, 3424.0 / 4096.0, 2413.0 / 4096.0 * 32.0, 2392.0 / 4096.0 * 32.0);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for px in bytes.chunks_exact(8) {
            let v = |i: usize| u16::from_le_bytes([px[i * 2], px[i * 2 + 1]]) as f64 / 65535.0;
            let a = v(3);
            for i in 0..3 {
                let e = if a > 0.0 { v(i) / a } else { 0.0 };
                let p = e.powf(1.0 / m2);
                let nits = 10000.0 * ((p - c1).max(0.0) / (c2 - c3 * p)).powf(1.0 / m1);
                rgba.push(((nits / white).clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8);
            }
            rgba.push(255);
        }
        Picture { w, h, rgba }
    }

    fn write_png(&self, path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut e = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(path).unwrap()), self.w, self.h);
        // (opaque, as every window's picture is, and packed tight: they're kept in the repository)
        let rgb: Vec<u8> = self.rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.set_compression(png::Compression::Best);
        e.set_adaptive_filter(png::AdaptiveFilterType::Adaptive);
        e.write_header().unwrap().write_image_data(&rgb).unwrap();
    }

    /// How many pixels differ from another picture's, and a picture of
    /// where: this one dimmed, the pixels that differ in magenta.
    fn against(&self, other: &Picture) -> (usize, Picture) {
        let mut n = 0;
        let mut rgba = Vec::with_capacity(self.rgba.len());
        for (a, b) in self.rgba.chunks_exact(4).zip(other.rgba.chunks_exact(4)) {
            if a != b {
                n += 1;
                rgba.extend_from_slice(&[255, 0, 255, 255]);
            } else {
                rgba.extend_from_slice(&[a[0] / 4, a[1] / 4, a[2] / 4, 255]);
            }
        }
        (n, Picture { w: self.w, h: self.h, rgba })
    }
}

/// A desk's folders: a home, its settings, its computer and its Steam.
struct Desk {
    scratch: Scratch,
}

impl Desk {
    fn new(name: &str, settings: bool) -> Desk {
        let scratch = Scratch::new(name);
        let d = &scratch.0;
        for f in ["home", "config", "data", "state", "cache", "run"] {
            std::fs::create_dir_all(d.join(f)).unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(d.join("run"), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        computer(&d.join("computer"));
        steam(&d.join("steam"));
        if settings {
            set_up(&d.join("config"));
        }
        Desk { scratch }
    }

    /// One page drawn (to a file named for it and `tag`): its picture and
    /// what the program said, or why it wasn't drawn.
    fn draw(&self, case: &Case, tag: &str) -> Result<(Picture, String), String> {
        let d = &self.scratch.0;
        let window = case.name.ends_with(".window");
        let out = d.join(format!("{}{tag}.{}", case.name, if window { "png" } else { "raw" }));
        let mut c = Command::new(env!("CARGO_BIN_EXE_ectomatter"));
        // (nothing of this computer's session: only what's set here)
        c.env_clear()
            .arg("--snapshot")
            .arg(&out)
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", d.join("home"))
            .env("XDG_CONFIG_HOME", d.join("config"))
            .env("XDG_DATA_HOME", d.join("data"))
            .env("XDG_STATE_HOME", d.join("state"))
            .env("XDG_CACHE_HOME", d.join("cache"))
            .env("XDG_RUNTIME_DIR", d.join("run"))
            .env("TZ", "UTC")
            .env("LANG", "C.UTF-8")
            .env("ECTOMATTER_PACKAGES", repo())
            .env("ECTOMATTER_COMPUTER", d.join("computer"))
            .env("ECTOMATTER_STEAM", d.join("steam"))
            .env("ECTOMATTER_NOW", NOW.to_string())
            .env("ECTOMATTER_STATE", here().join("tests/renders/desk/keyboard.json"))
            .env("ECTOMATTER_MOUSE", here().join("tests/renders/desk/mouse.json"))
            .env("ECTOMATTER_SIZE", format!("{}x{}", case.size.0, case.size.1))
            // (the packages' first answers in and the animations run out
            // before the picture is taken, unless the case says otherwise)
            .env("ECTOMATTER_WAIT", "1500")
            // (the lights' effects and the screen's playlist at a moment of their own)
            .env("ECTOMATTER_LIGHT_AT", "2000")
            .env("ECTOMATTER_LCD_AT", "3")
            .current_dir(d);
        for (k, v) in case.set {
            c.env(k, v);
        }
        let run = c.output().map_err(|e| e.to_string())?;
        let said = String::from_utf8_lossy(&run.stderr).into_owned();
        if !run.status.success() {
            return Err(format!("it ended {}:\n{said}", run.status));
        }
        if window {
            return Picture::read_png(&out).map(|p| (p, said.clone())).ok_or_else(|| format!("no picture was written:\n{said}"));
        }
        let bytes = std::fs::read(&out).map_err(|e| format!("no frame was written ({e}):\n{said}"))?;
        let white: f64 = std::fs::read_to_string(out.with_extension("raw.reference")).ok().and_then(|t| t.trim().parse().ok()).unwrap_or(203.0);
        let (w, h) = case.size;
        if bytes.len() != (w * h * 8) as usize {
            return Err(format!("the frame is {} bytes, not {w}x{h}'s:\n{said}", bytes.len()));
        }
        Ok((Picture::from_frame(&bytes, w, h, white), said))
    }
}

/// Each case drawn (four at a time) and held to its approved picture; the
/// pages that came out otherwise, and why.
fn check(desk: &Desk, cases: &[Case]) -> Vec<String> {
    let approved = here().join("tests/renders/approved");
    let changed = repo().join("target/renders");
    let approve = std::env::var_os("ECTOMATTER_APPROVE").is_some();
    // (ECTOMATTER_RENDERS=name,name: only those pages)
    let only: Vec<String> = std::env::var("ECTOMATTER_RENDERS").unwrap_or_default().split(',').filter(|n| !n.is_empty()).map(String::from).collect();
    let cases: Vec<&Case> = cases.iter().filter(|c| only.is_empty() || only.iter().any(|o| o == c.name.trim_end_matches(".window"))).collect();
    let drawn: Vec<(&Case, Result<(Picture, String), String>)> = std::thread::scope(|s| {
        let mut out = Vec::new();
        for group in cases.chunks(4) {
            let handles: Vec<_> = group.iter().map(|case| (*case, s.spawn(|| desk.draw(case, "")))).collect();
            out.extend(handles.into_iter().map(|(case, h)| (case, h.join().unwrap_or_else(|_| Err("its thread stopped".into())))));
        }
        out
    });
    let mut wrong = Vec::new();
    for (case, picture) in drawn {
        let name = case.name.trim_end_matches(".window");
        let (picture, said) = match picture {
            Ok(p) => p,
            Err(e) => {
                put(&changed.join(format!("{name}.log")), &e);
                wrong.push(format!("{name}: not drawn (target/renders/{name}.log): {}", e.lines().next().unwrap_or("")));
                continue;
            }
        };
        // (what it said as it drew, beside its picture, for a page that comes out otherwise)
        let tell = |why: String, wrong: &mut Vec<String>| {
            put(&changed.join(format!("{name}.log")), &said);
            wrong.push(why);
        };
        let file = approved.join(format!("{name}.png"));
        if approve {
            picture.write_png(&file);
            continue;
        }
        let Some(before) = Picture::read_png(&file) else {
            picture.write_png(&changed.join(format!("{name}.png")));
            tell(format!("{name}: no approved picture (drawn to target/renders/{name}.png)"), &mut wrong);
            continue;
        };
        if (before.w, before.h) != (picture.w, picture.h) {
            picture.write_png(&changed.join(format!("{name}.png")));
            tell(format!("{name}: drawn at {}x{}, approved at {}x{}", picture.w, picture.h, before.w, before.h), &mut wrong);
            continue;
        }
        let (n, diff) = picture.against(&before);
        if n > 0 {
            picture.write_png(&changed.join(format!("{name}.png")));
            diff.write_png(&changed.join(format!("{name}.diff.png")));
            tell(format!("{name}: {n} pixels differ (target/renders/{name}.png, and where in {name}.diff.png)"), &mut wrong);
        }
    }
    wrong
}

fn built() {
    for folder in ["cores", "plugins"] {
        for e in std::fs::read_dir(repo().join(folder)).unwrap().flatten() {
            if e.path().join("core.json").exists() {
                assert!(e.path().join("core.wasm").exists(), "{folder}/{} isn't built: run tools/build-{folder}.sh", e.file_name().to_string_lossy());
            }
        }
    }
}

/// Every page of a desk that's set up looks as approved.
#[test]
fn every_page_looks_as_approved() {
    built();
    let desk = Desk::new("set-up", true);
    let wrong = check(&desk, SET_UP);
    assert!(wrong.is_empty(), "pages that came out otherwise:\n  {}", wrong.join("\n  "));
}

/// Ectomatter opened for the first time looks as approved.
#[test]
fn the_first_run_looks_as_approved() {
    built();
    let desk = Desk::new("first-run", false);
    let wrong = check(&desk, FIRST_RUN);
    assert!(wrong.is_empty(), "pages that came out otherwise:\n  {}", wrong.join("\n  "));
}

/// Two drawings of the same page are the same: nothing on the desk moves
/// between runs (no clock, no animation caught midway, nothing read from the
/// computer running the test).
#[test]
fn a_page_drawn_twice_is_the_same() {
    built();
    let desk = Desk::new("twice", true);
    let mut wrong = Vec::new();
    let only: Vec<String> = std::env::var("ECTOMATTER_RENDERS").unwrap_or_default().split(',').filter(|n| !n.is_empty()).map(String::from).collect();
    for case in SET_UP.iter().filter(|c| only.is_empty() || only.iter().any(|o| o == c.name.trim_end_matches(".window"))) {
        let (a, b) = std::thread::scope(|s| {
            let a = s.spawn(|| desk.draw(case, "-a"));
            let b = s.spawn(|| desk.draw(case, "-b"));
            (a.join().unwrap(), b.join().unwrap())
        });
        match (a, b) {
            (Ok((a, _)), Ok((b, _))) => {
                let (n, diff) = a.against(&b);
                if n > 0 {
                    let name = case.name.trim_end_matches(".window");
                    let at = repo().join("target/renders/twice");
                    a.write_png(&at.join(format!("{name}.a.png")));
                    b.write_png(&at.join(format!("{name}.b.png")));
                    diff.write_png(&at.join(format!("{name}.diff.png")));
                    wrong.push(format!("{name}: {n} pixels differ between two drawings (target/renders/twice/)"));
                }
            }
            (Err(e), _) | (_, Err(e)) => wrong.push(format!("{}: not drawn: {e}", case.name)),
        }
    }
    assert!(wrong.is_empty(), "pages that move between drawings:\n  {}", wrong.join("\n  "));
}
