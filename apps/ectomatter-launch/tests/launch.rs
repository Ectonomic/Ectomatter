//! The launcher run as Steam runs it: Steam's environment for a game, a
//! runtime and a Proton build that write down what they were given, and
//! Ectomatter listening for the game's start.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn script(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Steam hands ARC Raiders to the launcher: Ectomatter is told the game is
/// starting and the launcher waits for its answer; then the runtime starts
/// the Proton build with Steam's verb, the game's command and the recipe's
/// arguments, in the recipe's environment.
#[test]
fn steam_starts_a_game_through_the_launcher() {
    let s: PathBuf = std::env::temp_dir().join(format!("ectomatter-launch-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&s);
    let (root, proton, rt, out) = (s.join("Steam"), s.join("Steam/compatibilitytools.d/Proton-GE Latest"), s.join("Steam/steamapps/common/SteamLinuxRuntime_4"), s.join("ran.txt"));
    // the runtime starts what follows its "--"; the Proton build writes down its arguments and environment
    script(&rt.join("_v2-entry-point"), "#!/bin/sh\nwhile [ \"$1\" != \"--\" ]; do shift; done\nshift\nexec \"$@\"\n");
    script(&proton.join("proton"), &format!("#!/bin/sh\nfor a in \"$@\"; do echo \"arg $a\"; done > '{0}'\necho \"wayland ${{PROTON_ENABLE_WAYLAND:-none}}\" >> '{0}'\necho \"hidraw ${{PROTON_DISABLE_HIDRAW:-none}}\" >> '{0}'\n", out.display()));
    std::fs::write(proton.join("toolmanifest.vdf"), "\"manifest\" { \"require_tool_appid\" \"4183110\" }").unwrap();
    std::fs::write(root.join("steamapps/appmanifest_4183110.acf"), "\"AppState\" { \"installdir\" \"SteamLinuxRuntime_4\" }").unwrap();
    let data = s.join("data/com.ectonomic.ectomatter/launch");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("1808500.json"), serde_json::json!({ "name": "ARC Raiders", "proton": proton, "env": { "PROTON_ENABLE_WAYLAND": "1" }, "unset": ["PROTON_DISABLE_HIDRAW"], "args": ["-nolauncher"] }).to_string()).unwrap();
    // Ectomatter, listening: it reads the game's start, writes the game's
    // recipe afresh (its arguments changed since), and answers after a moment
    let run_dir = s.join("run");
    std::fs::create_dir_all(run_dir.join("ectomatter")).unwrap();
    let listener = UnixListener::bind(run_dir.join("ectomatter/start.sock")).unwrap();
    let (fresh, proton2) = (data.join("1808500.json"), proton.clone());
    let told = std::thread::spawn(move || {
        let (mut c, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(c.try_clone().unwrap()).read_line(&mut line).unwrap();
        std::fs::write(&fresh, serde_json::json!({ "name": "ARC Raiders", "proton": proton2, "env": { "PROTON_ENABLE_WAYLAND": "1" }, "unset": ["PROTON_DISABLE_HIDRAW"], "args": ["-nolauncher", "-fresh"] }).to_string()).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        c.write_all(b"go\n").unwrap();
        line
    });
    let t0 = Instant::now();
    let status = Command::new(env!("CARGO_BIN_EXE_ectomatter-launch"))
        .args(["waitforexitandrun", "/games/ARC Raiders/PioneerGame.exe", "-dx12"])
        .env_clear()
        .env("HOME", &s)
        .env("PATH", "/usr/bin:/bin")
        .env("XDG_DATA_HOME", s.join("data"))
        .env("XDG_RUNTIME_DIR", &run_dir)
        .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", &root)
        .env("SteamAppId", "1808500")
        .env("PROTON_DISABLE_HIDRAW", "1")
        .status()
        .unwrap();
    assert!(status.success());
    let asked = told.join().unwrap();
    assert!(t0.elapsed() >= Duration::from_millis(300), "the launcher waited for Ectomatter's answer");
    let asked: serde_json::Value = serde_json::from_str(&asked).unwrap();
    assert_eq!((asked["app"].as_str(), asked["name"].as_str()), (Some("1808500"), Some("ARC Raiders")));
    assert_eq!(asked["pid"].as_i64(), Some(std::process::id() as i64), "the launcher's parent, under Steam's reaper");
    let ran = std::fs::read_to_string(&out).unwrap();
    assert_eq!(ran, "arg waitforexitandrun\narg /games/ARC Raiders/PioneerGame.exe\narg -dx12\narg -nolauncher\narg -fresh\nwayland 1\nhidraw none\n", "the recipe as Ectomatter wrote it during the hold");
    let log = std::fs::read_to_string(data.join("1808500.log")).unwrap();
    assert!(log.starts_with("ARC Raiders (1808500)\n") && log.contains("set PROTON_ENABLE_WAYLAND=1") && log.contains("_v2-entry-point --verb=waitforexitandrun --"), "{log}");
    let _ = std::fs::remove_dir_all(&s);
}

/// With Ectomatter not running, the game starts at once.
#[test]
fn a_game_starts_at_once_without_ectomatter() {
    let s: PathBuf = std::env::temp_dir().join(format!("ectomatter-launch-alone-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&s);
    let (root, out) = (s.join("Steam"), s.join("ran.txt"));
    script(&root.join("compatibilitytools.d/Proton-GE Latest/proton"), &format!("#!/bin/sh\necho \"$1 $2\" > '{}'\n", out.display()));
    let t0 = Instant::now();
    let status = Command::new(env!("CARGO_BIN_EXE_ectomatter-launch"))
        .args(["waitforexitandrun", "game.exe"])
        .env_clear()
        .env("HOME", &s)
        .env("PATH", "/usr/bin:/bin")
        .env("XDG_RUNTIME_DIR", s.join("run"))
        .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", &root)
        .env("SteamAppId", "42")
        .status()
        .unwrap();
    assert!(status.success());
    assert!(t0.elapsed() < Duration::from_secs(5));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "waitforexitandrun game.exe\n", "no recipe at all: the installed GE-Proton, no runtime asked for");
    let _ = std::fs::remove_dir_all(&s);
}
