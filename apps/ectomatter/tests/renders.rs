//! Ectomatter's pages drawn offscreen on a made-up desk, each compared pixel
//! for pixel with the picture approved for it (`tests/renders/approved/`).
//!
//! The desk is made up and holds nothing of Grant's: the keyboard's and the
//! mouse's states in `tests/renders/desk/`, a computer described in a folder
//! (`ECTOMATTER_COMPUTER`: the sensors crate's made-up computer with made-up
//! names, two made-up monitors, the weather's answers), a Steam library of
//! made-up games, fresh settings folders, the time of day fixed
//! (`ECTOMATTER_NOW`) and the time zone UTC, and the lights' effects and the
//! screen's playlist each at a moment fixed. Each page is drawn in a sandbox
//! (bubblewrap, `bwrap`) that hides the computer running the test: no
//! network, no session, an empty /sys, a /proc and /dev of the sandbox's own,
//! no home folder, the source read-only at /opt/ectomatter-source. Two desks
//! are drawn: one set up (scenes, a place for the weather), and one as a
//! stranger first opens Ectomatter, with no settings and no device plugged in.
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
    // (Arrange at the narrowest window, a pad added from Add pad's menu and
    // chosen: the bar at its widest, and Revert there to press)
    Case { name: "n-home-edit-pad", size: NARROW, set: &[("ECTOMATTER_PLACE", "home"), ("ECTOMATTER_DESK", "edit"), ("ECTOMATTER_CLICKS", "596,67;600,171")] },
    // (and Revert pressed: the pad gone, nothing chosen, Revert at rest)
    Case { name: "n-home-edit-revert", size: NARROW, set: &[("ECTOMATTER_PLACE", "home"), ("ECTOMATTER_DESK", "edit"), ("ECTOMATTER_CLICKS", "596,67;600,171;726,66")] },
    Case { name: "kb-keys", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard")] },
    // (read, plugged in, and then not answering a read)
    Case { name: "kb-stopped", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_UNREAD", "no reply")] },
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
    Case { name: "settings-devices", size: WIDE, set: &[("ECTOMATTER_PLACE", "settings"), ("ECTOMATTER_SETTINGS_TAB", "devices")] },
    Case { name: "settings-plugins", size: WIDE, set: &[("ECTOMATTER_PLACE", "settings"), ("ECTOMATTER_SETTINGS_TAB", "plugins")] },
    Case { name: "settings-about", size: WIDE, set: &[("ECTOMATTER_PLACE", "settings"), ("ECTOMATTER_SETTINGS_TAB", "about")] },
    Case { name: "settings-credits", size: WIDE, set: &[("ECTOMATTER_PLACE", "settings"), ("ECTOMATTER_SETTINGS_TAB", "about"), ("ECTOMATTER_CLICKS", "204,958")] },
    Case { name: "n-about", size: NARROW, set: &[("ECTOMATTER_PLACE", "settings"), ("ECTOMATTER_SETTINGS_TAB", "about")] },
    // (Add a core: the list of reviewed packages, desk/packages.json, a made-up one chosen)
    Case { name: "add-core", size: WIDE, set: &[("ECTOMATTER_PLACE", "settings"), ("ECTOMATTER_SETTINGS_TAB", "devices"), ("ECTOMATTER_REGISTRY", "/opt/ectomatter-source/apps/ectomatter/tests/renders/desk/packages.json"), ("ECTOMATTER_CLICKS", "160,767;520,524")] },
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
];

/// The main window's places on the desk of a stranger opening Ectomatter
/// for the first time.
const FIRST_RUN: &[Case] = &[
    Case { name: "first-home", size: WIDE, set: &[("ECTOMATTER_PLACE", "home")] },
    Case { name: "first-keyboard", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard")] },
    Case { name: "first-mouse", size: WIDE, set: &[("ECTOMATTER_PLACE", "mouse"), ("ECTOMATTER_MOUSE_TAB", "0")] },
    Case { name: "first-settings", size: (1440, 3600), set: &[("ECTOMATTER_PLACE", "settings")] },
    // (About with no device plugged in: nothing on it comes from one)
    Case { name: "first-about", size: WIDE, set: &[("ECTOMATTER_PLACE", "settings"), ("ECTOMATTER_SETTINGS_TAB", "about")] },
    Case { name: "first-library", size: WIDE, set: &[("ECTOMATTER_PLACE", "library")] },
];

/// The keyboard's page on a desk where Ectomatter has met the keyboard
/// (desk/window.json keeps its place) and it isn't plugged in now: its read
/// failed, as its core says when nothing is there.
const UNPLUGGED: &[Case] = &[Case { name: "kb-unplugged", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_UNREAD", "the keyboard isn't connected")] }];

/// The README's pictures (`docs/pictures/`), on a desk made up for showing
/// Ectomatter (`Desk::showcase`): the same pages, lit and filled in.
const SHOWCASE: &[Case] = &[
    Case { name: "readme-home", size: WIDE, set: &[("ECTOMATTER_PLACE", "home")] },
    Case { name: "readme-scene", size: WIDE, set: &[("ECTOMATTER_PLACE", "home"), ("ECTOMATTER_SCENE_FORM", "new")] },
    Case { name: "readme-lighting", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "358,63")] },
    Case { name: "readme-keys", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "420,416")] },
    Case { name: "readme-screen", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "440,63")] },
    Case { name: "readme-screen-editor.window", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_LAYOUTS", "0")] },
    Case { name: "readme-macros.window", size: (1080, 620), set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_MACROS", "1"), ("ECTOMATTER_CLICKS", "120,175")] },
    Case { name: "readme-mouse", size: WIDE, set: &[("ECTOMATTER_PLACE", "mouse"), ("ECTOMATTER_MOUSE_TAB", "1")] },
    Case { name: "readme-library", size: WIDE, set: &[("ECTOMATTER_PLACE", "library"), ("ECTOMATTER_CLICKS", "296,63"), ("ECTOMATTER_LIBRARY", SHOWN_LIBRARY)] },
    Case { name: "readme-game", size: WIDE, set: &[("ECTOMATTER_PLACE", "library"), ("ECTOMATTER_LIBRARY_PAGE", "steam_app_1091500"), ("ECTOMATTER_LIBRARY", SHOWN_LIBRARY), ("ECTOMATTER_WAIT", "1500")] },
    Case { name: "readme-plugins", size: WIDE, set: &[("ECTOMATTER_PLACE", "settings"), ("ECTOMATTER_SETTINGS_TAB", "plugins")] },
];

/// Where the showcase's Library is in the sandbox (`Desk::showcase` writes it;
/// a snapshot reads it, and writes nothing back).
const SHOWN_LIBRARY: &str = "/tmp/ectomatter-renders/showcase/library.json";

/// A folder of the test's own, empty, gone after it.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let d = std::env::temp_dir().join(format!("ectomatter-renders-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
}

/// Where a desk's folder is in the sandbox, the same on every computer and
/// every run (a page can show it: a game's prefix).
fn inside(name: &str) -> PathBuf {
    Path::new("/tmp/ectomatter-renders").join(name)
}

/// Where the source folder is in the sandbox.
const SOURCE: &str = "/opt/ectomatter-source";

/// Where Ectomatter's program is in the sandbox.
const PROGRAM: &str = "/opt/ectomatter/bin/ectomatter";

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
fn steam(root: &Path, seen_at: &Path) {
    put(&root.join("steamapps/libraryfolders.vdf"), &format!("\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n", seen_at.display()));
    // (the account 76561198000000001, which Steam keeps its grid under as 39734273)
    put(&root.join("config/loginusers.vdf"), "\"users\"\n{\n\t\"76561198000000001\"\n\t{\n\t\t\"AccountName\"\t\t\"stranger\"\n\t\t\"MostRecent\"\t\t\"1\"\n\t\t\"Timestamp\"\t\t\"1792600000\"\n\t}\n}\n");
    for (id, name, back, band) in GAMES {
        put(&root.join(format!("steamapps/appmanifest_{id}.acf")), &format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"{name}\"\n}}\n"));
        std::fs::create_dir_all(root.join(format!("steamapps/common/{name}"))).unwrap();
        cover(&root.join(format!("userdata/39734273/config/grid/{id}p.png")), (300, 450), *back, *band);
    }
}

/// The computer the packages and Ectomatter reach (computer::Described): the
/// sensors crate's made-up computer, its processor, board, cards and drives
/// given made-up names; two made-up monitors; its NVIDIA card's readings;
/// the weather's answers for a clear morning in Reykjavík; udev's rule for
/// the cores that come with Ectomatter, where an install puts it; and, if
/// `plugged`, the cores' devices plugged in. (Its files don't change, so the
/// processor's load, the change between two readings of /proc/stat, is 0.)
fn computer(dir: &Path, plugged: bool) {
    let files = dir.join("files");
    let rules = Command::new(env!("CARGO_BIN_EXE_ectomatter")).arg("--udev-rules").env("ECTOMATTER_PACKAGES", repo()).output().unwrap();
    put(&files.join("usr/lib/udev/rules.d/70-ectomatter.rules"), &String::from_utf8_lossy(&rules.stdout));
    ectomatter_sensors::fake::computer(&files);
    put(&files.join("proc/cpuinfo"), "processor\t: 0\nmodel name\t: Example 8-Core Processor\ncpu cores\t: 4\n");
    put(&files.join("proc/stat"), "cpu  4000 0 2000 32000 400 0 100 0 0 0\ncpu0 1000 0 500 8000 100 0 25 0 0 0\ncpu1 1000 0 500 8000 100 0 25 0 0 0\ncpu2 1000 0 500 8000 100 0 25 0 0 0\ncpu3 1000 0 500 8000 100 0 25 0 0 0\nbtime 1792670000\n");
    put(&files.join("sys/class/dmi/id/board_vendor"), "Example Boards\n");
    put(&files.join("sys/class/dmi/id/board_name"), "EX-1000\n");
    put(&files.join("usr/share/hwdata/pci.ids"), "1002  Example Graphics\n\t13c0  Built-in Graphics\n10de  Example Cards\n\t2b85  Example Card 9000\n14c3  Example Radios\n\t0616  Example Wi-Fi\n1d6a  Example Networks\n\t94c0  Example 10G Ethernet\n8086  Example Silicon\n\te212  Example Card 500\n");
    for (drive, model, serial) in [("0000:00:01.2/0000:02:00.0/nvme/nvme0", "Example SSD 4TB", "EXAMPLE0001"), ("0000:00:02.1/0000:0f:00.0/nvme/nvme3", "Example SSD 2TB", "EXAMPLE0002")] {
        put(&files.join(format!("sys/devices/pci0000:00/{drive}/model")), &format!("{model}\n"));
        put(&files.join(format!("sys/devices/pci0000:00/{drive}/serial")), &format!("{serial}\n"));
    }
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
    if !plugged {
        devices.clear();
    }
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

/// The showcase's Steam: real games (docs/pictures/games.json), installed in
/// the person's own Steam folder, each with its cover, banner and logo as
/// Steam's library cache keeps them (fetched into target/readme-art/ by
/// tools/readme-art.py: the art is the games' publishers', shown under fair
/// use, and isn't kept in the repository), the minutes played and when, and
/// a Proton build of its own.
fn shown_steam(root: &Path, seen_at: &Path) {
    let list: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(repo().join("docs/pictures/games.json")).unwrap()).unwrap();
    put(&root.join("steamapps/libraryfolders.vdf"), &format!("\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n", seen_at.display()));
    put(&root.join("config/loginusers.vdf"), "\"users\"\n{\n\t\"76561198000000001\"\n\t{\n\t\t\"AccountName\"\t\t\"stranger\"\n\t\t\"MostRecent\"\t\t\"1\"\n\t\t\"Timestamp\"\t\t\"1792600000\"\n\t}\n}\n");
    let mut played = String::new();
    for g in list["games"].as_array().unwrap() {
        let (id, name) = (g["id"].as_u64().unwrap(), g["name"].as_str().unwrap());
        put(&root.join(format!("steamapps/appmanifest_{id}.acf")), &format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"{id}\"\n}}\n"));
        std::fs::create_dir_all(root.join(format!("steamapps/common/{id}"))).unwrap();
        for file in ["library_600x900_2x.jpg", "library_hero.jpg", "logo.png"] {
            let art = repo().join(format!("target/readme-art/{id}/{file}"));
            assert!(art.is_file(), "{name}'s art isn't here ({}): run tools/readme-art.py", art.display());
            std::fs::create_dir_all(root.join(format!("appcache/librarycache/{id}"))).unwrap();
            std::fs::copy(&art, root.join(format!("appcache/librarycache/{id}/{file}"))).unwrap();
        }
        let (minutes, hours) = (g["minutes"].as_u64().unwrap(), g["hours_since"].as_u64().unwrap());
        played += &format!("\t\t\t\t\t\"{id}\"\n\t\t\t\t\t{{\n\t\t\t\t\t\t\"Playtime\"\t\t\"{minutes}\"\n\t\t\t\t\t\t\"LastPlayed\"\t\t\"{}\"\n\t\t\t\t\t}}\n", NOW - hours * 3600);
    }
    put(&root.join("userdata/39734273/config/localconfig.vdf"), &format!("\"UserLocalConfigStore\"\n{{\n\t\"Software\"\n\t{{\n\t\t\"Valve\"\n\t\t{{\n\t\t\t\"Steam\"\n\t\t\t{{\n\t\t\t\t\"apps\"\n\t\t\t\t{{\n{played}\t\t\t\t}}\n\t\t\t}}\n\t\t}}\n\t}}\n}}\n"));
    let ge = root.join("compatibilitytools.d/GE-Proton10-25");
    put(&ge.join("proton"), "");
    put(&ge.join("compatibilitytool.vdf"), "\"compatibilitytools\"\n{\n\t\"compat_tools\"\n\t{\n\t\t\"GE-Proton10-25\"\n\t\t{\n\t\t\t\"install_path\"\t\".\"\n\t\t\t\"display_name\"\t\"GE-Proton10-25\"\n\t\t\t\"from_oslist\"\t\"windows\"\n\t\t\t\"to_oslist\"\t\"linux\"\n\t\t}\n\t}\n}\n");
}

/// A color as the keyboard's state writes it.
fn rgb(c: u32) -> serde_json::Value {
    serde_json::json!([(c >> 16) & 255, (c >> 8) & 255, c & 255])
}

/// The showcase's keyboard: the made-up desk's, its profiles named and lit
/// for showing, a wave of color on the one in use and the screen's light to
/// go with it, and its macros named for what they'd do.
fn shown_keyboard() -> serde_json::Value {
    let mut k: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(here().join("tests/renders/desk/keyboard.json")).unwrap()).unwrap();
    let tones = |cs: &[u32]| serde_json::Value::Array(cs.iter().map(|c| serde_json::json!({ "brightness": 255, "color": rgb(*c) })).collect());
    let lit: [(&str, &str, u32, u32); 5] = [("Everyday", "wave", 0x00D4B4, 255), ("Games", "static", 0xFF5A00, 255), ("Late night", "breathing", 0xFF8A1E, 255), ("Movies", "static", 0x2E4BFF, 110), ("Focus", "static", 0xFFFFFF, 140)];
    for (n, (name, effect, color, brightness)) in lit.iter().enumerate() {
        let p = &mut k["profiles"][(n + 1).to_string()];
        p["name"] = serde_json::json!(name);
        let keys = &mut p["lighting"]["keys"];
        keys["effect"] = serde_json::json!(effect);
        keys["effects"]["static"]["color"] = rgb(*color);
        keys["effects"]["static"]["brightness"] = serde_json::json!(brightness);
        keys["effects"]["breathing"]["colors"] = tones(&[*color, 0x7A2400]);
        keys["effects"]["breathing"]["cycle"] = serde_json::json!(4000);
    }
    // (warm to cool and back: the colors the keys' LEDs keep whole, as
    // keyboard.rs's bleed draws them; violet and magenta wash out to white)
    let wave = &mut k["profiles"]["1"]["lighting"]["keys"]["effects"]["wave"];
    wave["colors"] = tones(&[0xFF2A00, 0xFF8A00, 0x00D4B4, 0x00A8FF, 0x00D4B4, 0xFF8A00]);
    wave["cycle"] = serde_json::json!(4000);
    k["profiles"]["1"]["lighting"]["screen"]["color"]["color"] = rgb(0x00D4B4);
    let names = ["Screenshot", "Mute the mic", "Copy the line", "Lock the screen", "New tab", "Paste as text", "Push to talk", "Sign-off", "Build and run"];
    for (n, name) in names.iter().enumerate() {
        if let Some(m) = k["macros"].get_mut((n + 2).to_string()) {
            m["name"] = serde_json::json!(name);
        }
    }
    k
}

/// The showcase's mouse: the made-up desk's, both lights set by Ectomatter
/// (so known) in the keyboard's colors.
fn shown_mouse() -> serde_json::Value {
    let mut m: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(here().join("tests/renders/desk/mouse.json")).unwrap()).unwrap();
    m["lighting"]["logo"] = serde_json::json!({ "effect": "static", "params": { "color": "#00D4B4" } });
    m["lighting"]["wheel"] = serde_json::json!({ "effect": "static", "params": { "color": "#FF5A00" } });
    m["lighting"]["known"] = serde_json::json!({ "logo": true, "wheel": true });
    m["settings"]["known"]["polling"] = serde_json::json!(true);
    m
}

/// A picture as the eye sees it: 8-bit sRGB, red, green, blue and alpha.
struct Picture {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

impl Picture {
    fn read_png(path: &Path) -> Option<Picture> {
        let mut d = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
        d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA);
        let mut r = d.read_info().ok()?;
        let mut buf = vec![0; r.output_buffer_size()?];
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
        e.set_compression(png::Compression::High);
        e.set_filter(png::Filter::Adaptive);
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

/// A desk's folders: a home, its settings, its computer and its Steam; where
/// they are in the sandbox; and whether its devices are plugged in.
struct Desk {
    scratch: Scratch,
    at: PathBuf,
    plugged: bool,
    /// Its Steam's folder in the sandbox.
    steam: PathBuf,
    /// The keyboard's and the mouse's states in the sandbox, when they aren't
    /// the fixtures' (`desk/keyboard.json`, `desk/mouse.json`).
    states: Option<(PathBuf, PathBuf)>,
    /// The pads on Home's desk, as window.json keeps them, in the sandbox.
    pads: Option<PathBuf>,
}

impl Desk {
    fn new(name: &str, settings: bool, plugged: bool) -> Desk {
        let scratch = Scratch::new(name);
        let at = inside(name);
        let d = &scratch.0;
        for f in ["home", "config", "data", "state", "cache", "run"] {
            std::fs::create_dir_all(d.join(f)).unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(d.join("run"), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        computer(&d.join("computer"), plugged);
        steam(&d.join("steam"), &at.join("steam"));
        if settings {
            set_up(&d.join("config"));
        }
        let steam = at.join("steam");
        Desk { scratch, at, plugged, steam, states: None, pads: None }
    }

    /// The desk the README's pictures are drawn on: one that's set up, its
    /// Steam in the person's own folder (`~/.local/share/Steam`) with real
    /// games and their art, its keyboard and mouse lit for showing, and a mat
    /// under them.
    fn showcase() -> Desk {
        let mut desk = Desk::new("showcase", true, true);
        let (d, at) = (desk.scratch.0.clone(), desk.at.clone());
        shown_steam(&d.join("home/.local/share/Steam"), &at.join("home/.local/share/Steam"));
        put(&d.join("states/keyboard.json"), &shown_keyboard().to_string());
        // (the first game set up: its scene and the keyboard's profile while it
        // plays, and a Proton build of its own)
        assert_eq!(at.join("library.json"), Path::new(SHOWN_LIBRARY));
        let library = serde_json::json!({
            "choices": { "steam_app_1091500": { "scene": "scene-1", "z10": "2" } },
            "plugins": { "game-configurator": { "steam_app_1091500": { "version": "GE-Proton10-25" } } }
        });
        put(&d.join("library.json"), &library.to_string());
        // (the mouse's wheel lit its own color, so the logo's page doesn't say
        // the wheel copies it)
        put(&d.join("config/com.ectonomic.ectomatter/window.json"), &serde_json::json!({ "mouse_same_wheel": false }).to_string());
        put(&d.join("states/mouse.json"), &shown_mouse().to_string());
        desk.steam = at.join("home/.local/share/Steam");
        desk.states = Some((at.join("states/keyboard.json"), at.join("states/mouse.json")));
        // (a desk mat under the keyboard and the mouse, as most desks have)
        put(&d.join("pads.json"), &serde_json::json!([{ "x": -72.0, "y": -48.0, "w": 1200.0, "h": 456.0, "color": "#272A31" }]).to_string());
        desk.pads = Some(at.join("pads.json"));
        desk
    }

    /// One page drawn (to a file named for it and `tag`): its picture and
    /// what the program said, or why it wasn't drawn.
    fn draw(&self, case: &Case, tag: &str) -> Result<(Picture, String), String> {
        let (host, d) = (&self.scratch.0, &self.at);
        let window = case.name.ends_with(".window");
        let file = format!("{}{tag}.{}", case.name, if window { "png" } else { "raw" });
        let out = host.join(&file);
        let fixture = |f: &str| Path::new(SOURCE).join("apps/ectomatter/tests/renders/desk").join(f);
        // the sandbox: the system's programs, libraries and typefaces read-only,
        // and nothing else of this computer's but the desk, the source and the
        // program
        let mut c = Command::new("bwrap");
        c.args(["--die-with-parent", "--unshare-all", "--new-session", "--hostname", "ectomatter-renders"])
            .args(["--ro-bind", "/usr", "/usr", "--symlink", "usr/lib64", "/lib64", "--symlink", "usr/lib", "/lib", "--symlink", "usr/bin", "/bin", "--symlink", "usr/sbin", "/sbin"])
            .args(["--ro-bind-try", "/etc/fonts", "/etc/fonts", "--ro-bind-try", "/etc/ld.so.cache", "/etc/ld.so.cache"])
            .args(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp", "--dir", "/sys"])
            .arg("--bind").arg(host).arg(d)
            .arg("--ro-bind").arg(repo()).arg(SOURCE)
            .arg("--ro-bind").arg(env!("CARGO_BIN_EXE_ectomatter")).arg(PROGRAM)
            .arg("--chdir").arg(d)
            .arg(PROGRAM)
            .arg("--snapshot")
            .arg(d.join(&file));
        // (nothing of this computer's session: only what's set here)
        c.env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", d.join("home"))
            .env("XDG_CONFIG_HOME", d.join("config"))
            .env("XDG_DATA_HOME", d.join("data"))
            .env("XDG_STATE_HOME", d.join("state"))
            .env("XDG_CACHE_HOME", d.join("cache"))
            .env("XDG_RUNTIME_DIR", d.join("run"))
            .env("TZ", "UTC")
            .env("LANG", "C.UTF-8")
            .env("ECTOMATTER_PACKAGES", SOURCE)
            .env("ECTOMATTER_COMPUTER", d.join("computer"))
            .env("ECTOMATTER_STEAM", &self.steam)
            .env("ECTOMATTER_NOW", NOW.to_string())
            .env("ECTOMATTER_SIZE", format!("{}x{}", case.size.0, case.size.1))
            // (the packages' first answers in and the animations run out
            // before the picture is taken, unless the case says otherwise)
            .env("ECTOMATTER_WAIT", "1500")
            // (the lights' effects and the screen's playlist at a moment of their own)
            .env("ECTOMATTER_LIGHT_AT", "2000")
            .env("ECTOMATTER_LCD_AT", "3")
            // (About's credits made up, as a history the desk has)
            .env("ECTOMATTER_CREDITS", fixture("credits.json"));
        // (the devices as their cores last read them, where they're plugged in)
        match (&self.states, self.plugged) {
            (Some((keyboard, mouse)), true) => c.env("ECTOMATTER_STATE", keyboard).env("ECTOMATTER_MOUSE", mouse),
            (None, true) => c.env("ECTOMATTER_STATE", fixture("keyboard.json")).env("ECTOMATTER_MOUSE", fixture("mouse.json")),
            _ => &mut c,
        };
        if let Some(pads) = &self.pads {
            c.env("ECTOMATTER_PADS", pads);
        }
        for (k, v) in case.set {
            c.env(k, v);
        }
        let run = c.output().map_err(|e| format!("bubblewrap (bwrap) didn't run, which the render tests need: {e}"))?;
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
    check_in(desk, cases, &here().join("tests/renders/approved"))
}

/// `check`, held to the pictures in `approved`.
fn check_in(desk: &Desk, cases: &[Case], approved: &Path) -> Vec<String> {
    let changed = repo().join("target/renders");
    let approve = std::env::var_os("ECTOMATTER_APPROVE").is_some();
    let cases = chosen(cases);
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
        // (ECTOMATTER_RENDERS_LOG: every page's log, whatever came out)
        if std::env::var_os("ECTOMATTER_RENDERS_LOG").is_some() {
            put(&changed.join(format!("{name}.log")), &said);
        }
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

/// The cases ECTOMATTER_RENDERS names (name,name), or every one: each name
/// asked for one of the cases somewhere.
fn chosen(cases: &[Case]) -> Vec<&Case> {
    let only: Vec<String> = std::env::var("ECTOMATTER_RENDERS").unwrap_or_default().split(',').filter(|n| !n.is_empty()).map(String::from).collect();
    for o in &only {
        assert!(SET_UP.iter().chain(FIRST_RUN).chain(UNPLUGGED).chain(SHOWCASE).any(|c| c.name.trim_end_matches(".window") == o), "ECTOMATTER_RENDERS names {o}, which is no page here");
    }
    cases.iter().filter(|c| only.is_empty() || only.iter().any(|o| o == c.name.trim_end_matches(".window"))).collect()
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

/// The README's pictures (docs/pictures/) are the app as it draws now, on a
/// desk made up for showing it: a change to a page they show fails here
/// until they're drawn again (ECTOMATTER_APPROVE=1, as the others).
#[test]
fn the_readmes_pictures_are_the_app_as_it_is() {
    built();
    let desk = Desk::showcase();
    let wrong = check_in(&desk, SHOWCASE, &repo().join("docs/pictures"));
    assert!(wrong.is_empty(), "pictures that came out otherwise:\n  {}", wrong.join("\n  "));
}

/// The frames of the README's moving picture (`tools/readme-hero.sh` makes
/// it from them): the showcase's desk at Home, the keyboard's wave at each
/// moment of one turn (ECTOMATTER_LIGHT_AT), into target/readme-hero/.
#[test]
#[ignore = "makes the README's moving picture's frames: tools/readme-hero.sh runs it"]
fn the_readmes_moving_picture() {
    built();
    let desk = Desk::showcase();
    let frames: u32 = std::env::var("ECTOMATTER_HERO_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
    let turn = 4000;
    let out = repo().join("target/readme-hero");
    let _ = std::fs::remove_dir_all(&out);
    let times: Vec<String> = (0..frames).map(|n| (n * turn / frames).to_string()).collect();
    std::thread::scope(|s| {
        for group in times.chunks(4) {
            let handles: Vec<_> = group
                .iter()
                .map(|ms| {
                    let (desk, out) = (&desk, &out);
                    s.spawn(move || {
                        let set: &'static [(&'static str, &'static str)] = Box::leak(vec![("ECTOMATTER_PLACE", "home"), ("ECTOMATTER_LIGHT_AT", Box::leak(ms.clone().into_boxed_str()) as &str)].into_boxed_slice());
                        let case = Case { name: "hero", size: WIDE, set };
                        let (picture, _) = desk.draw(&case, &format!("-{ms}")).unwrap();
                        picture.write_png(&out.join(format!("frame-{:05}.png", ms.parse::<u32>().unwrap())));
                    })
                })
                .collect();
            for h in handles {
                h.join().unwrap();
            }
        }
    });
}

/// Every page of a desk that's set up looks as approved.
#[test]
fn every_page_looks_as_approved() {
    built();
    let desk = Desk::new("set-up", true, true);
    let wrong = check(&desk, SET_UP);
    assert!(wrong.is_empty(), "pages that came out otherwise:\n  {}", wrong.join("\n  "));
}

/// Ectomatter opened for the first time looks as approved.
#[test]
fn the_first_run_looks_as_approved() {
    built();
    let desk = Desk::new("first-run", false, false);
    let wrong = check(&desk, FIRST_RUN);
    assert!(wrong.is_empty(), "pages that came out otherwise:\n  {}", wrong.join("\n  "));
}

/// A keyboard Ectomatter has met, not plugged in, says so on its page.
#[test]
fn a_met_keyboard_unplugged_says_so() {
    built();
    let desk = Desk::new("unplugged", false, false);
    let kept = desk.scratch.0.join("config/com.ectonomic.ectomatter/window.json");
    std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
    std::fs::copy(here().join("tests/renders/desk/window.json"), &kept).unwrap();
    let wrong = check(&desk, UNPLUGGED);
    assert!(wrong.is_empty(), "pages that came out otherwise:\n  {}", wrong.join("\n  "));
}

/// Each place left away from its first page (another tab, something chosen, the desk
/// being arranged), and the main menu's item for it pressed, from that place or from
/// another; and the page it should come out as, its first.
const RESETS: &[(Case, &str)] = &[
    (Case { name: "reset-settings", size: WIDE, set: &[("ECTOMATTER_PLACE", "settings"), ("ECTOMATTER_SETTINGS_TAB", "about"), ("ECTOMATTER_CLICKS", "30,998"), ("ECTOMATTER_POINTER_AWAY", "1"), ("ECTOMATTER_WAIT", "900")] }, "settings"),
    (Case { name: "reset-settings-from-home", size: WIDE, set: &[("ECTOMATTER_PLACE", "settings"), ("ECTOMATTER_SETTINGS_TAB", "about"), ("ECTOMATTER_CLICKS", "30,66;30,998"), ("ECTOMATTER_POINTER_AWAY", "1"), ("ECTOMATTER_WAIT", "900")] }, "settings"),
    (Case { name: "reset-keyboard", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "358,63;30,122"), ("ECTOMATTER_POINTER_AWAY", "1"), ("ECTOMATTER_WAIT", "900")] }, "kb-keys"),
    (Case { name: "reset-keyboard-key", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "420,416;30,122"), ("ECTOMATTER_POINTER_AWAY", "1"), ("ECTOMATTER_WAIT", "900")] }, "kb-keys"),
    (Case { name: "reset-mouse", size: WIDE, set: &[("ECTOMATTER_PLACE", "mouse"), ("ECTOMATTER_MOUSE_TAB", "1"), ("ECTOMATTER_CLICKS", "30,166"), ("ECTOMATTER_POINTER_AWAY", "1"), ("ECTOMATTER_WAIT", "900")] }, "m-buttons"),
    (Case { name: "reset-library", size: WIDE, set: &[("ECTOMATTER_PLACE", "library"), ("ECTOMATTER_CLICKS", "368,63;30,954"), ("ECTOMATTER_POINTER_AWAY", "1"), ("ECTOMATTER_WAIT", "900")] }, "library"),
    // (what's typed in a find box cleared too)
    (Case { name: "reset-keyboard-find", size: WIDE, set: &[("ECTOMATTER_PLACE", "keyboard"), ("ECTOMATTER_CLICKS", "620,784"), ("ECTOMATTER_TYPE", "zq"), ("ECTOMATTER_CLICKS_AFTER", "30,122"), ("ECTOMATTER_POINTER_AWAY", "1"), ("ECTOMATTER_WAIT", "900")] }, "kb-keys"),
    (Case { name: "reset-mouse-find", size: WIDE, set: &[("ECTOMATTER_PLACE", "mouse"), ("ECTOMATTER_CLICKS", "600,711"), ("ECTOMATTER_TYPE", "zq"), ("ECTOMATTER_CLICKS_AFTER", "30,166"), ("ECTOMATTER_POINTER_AWAY", "1"), ("ECTOMATTER_WAIT", "900")] }, "m-buttons"),
    (Case { name: "reset-library-find", size: WIDE, set: &[("ECTOMATTER_PLACE", "library"), ("ECTOMATTER_CLICKS", "1030,122"), ("ECTOMATTER_TYPE", "zq"), ("ECTOMATTER_CLICKS_AFTER", "30,954"), ("ECTOMATTER_POINTER_AWAY", "1"), ("ECTOMATTER_WAIT", "900")] }, "library"),
    (Case { name: "reset-home", size: WIDE, set: &[("ECTOMATTER_PLACE", "home"), ("ECTOMATTER_DESK", "edit"), ("ECTOMATTER_CLICKS", "30,66"), ("ECTOMATTER_POINTER_AWAY", "1"), ("ECTOMATTER_WAIT", "900")] }, "home"),
];

/// A press on the main menu's item for a place opens it at its first page, as a tab
/// bar's item does: each place left away from it comes out as its first page's approved
/// picture, pixel for pixel (src/front/first_page.rs).
#[test]
fn a_main_menu_item_opens_its_place_at_its_first_page() {
    built();
    let desk = Desk::new("resets", true, true);
    let approved = here().join("tests/renders/approved");
    let at = repo().join("target/renders/resets");
    let mut wrong = Vec::new();
    for (case, like) in RESETS {
        let want = Picture::read_png(&approved.join(format!("{like}.png"))).unwrap_or_else(|| panic!("{like} has no approved picture"));
        match desk.draw(case, "") {
            Ok((got, said)) => {
                let (n, diff) = got.against(&want);
                if n > 0 {
                    got.write_png(&at.join(format!("{}.png", case.name)));
                    diff.write_png(&at.join(format!("{}.diff.png", case.name)));
                    put(&at.join(format!("{}.log", case.name)), &said);
                    wrong.push(format!("{}: {n} pixels differ from {like}, its first page (target/renders/resets/)", case.name));
                }
            }
            Err(e) => wrong.push(format!("{}: not drawn: {e}", case.name)),
        }
    }
    assert!(wrong.is_empty(), "places a main menu item didn't open at their first page:\n  {}", wrong.join("\n  "));
}

/// Two drawings of the same page are the same: nothing on the desk moves
/// between runs (no clock, no animation caught midway, nothing read from the
/// computer running the test).
#[test]
fn a_page_drawn_twice_is_the_same() {
    built();
    let mut wrong = Vec::new();
    let desks = [(Desk::new("twice-set-up", true, true), SET_UP), (Desk::new("twice-first-run", false, false), FIRST_RUN)];
    for (desk, case) in desks.iter().flat_map(|(d, cases)| chosen(cases).into_iter().map(move |c| (d, c))) {
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

/// Every place the window shows is drawn: each `place` the window's Slint
/// compares with (`root.place == "..."`) has a page here.
#[test]
fn every_place_of_the_window_is_drawn() {
    let mut places = std::collections::BTreeSet::new();
    let mut dirs = vec![here().join("ui")];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "slint") {
                let text = std::fs::read_to_string(&p).unwrap();
                for rest in text.split("place == \"").skip(1) {
                    places.insert(rest.split('"').next().unwrap().to_string());
                }
            }
        }
    }
    assert!(!places.is_empty(), "the window's places weren't found");
    let drawn: Vec<&str> = SET_UP.iter().flat_map(|c| c.set.iter()).filter(|(k, _)| *k == "ECTOMATTER_PLACE").map(|(_, v)| *v).collect();
    let missing: Vec<&String> = places.iter().filter(|p| !drawn.contains(&p.as_str())).collect();
    assert!(missing.is_empty(), "places of the window no page draws: {missing:?}");
}
