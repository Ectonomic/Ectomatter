//! Ectomatter installed by `tools/install.sh` into a folder of the test's
//! own, as a package of it would lay it out under /usr, and started from
//! there with fresh settings and none of this computer's: it finds what the
//! install put beside it and nothing of its source.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// tools/install.sh's layout under /usr, staged in a folder of the test's
/// own, from the programs this test was built with.
#[test]
fn an_install_lays_ectomatter_out_and_it_starts_from_there() {
    let s = Scratch(std::env::temp_dir().join(format!("ectomatter-install-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&s.0);
    let bin = Path::new(env!("CARGO_BIN_EXE_ectomatter")).parent().unwrap();
    let run = Command::new(repo().join("tools/install.sh")).args(["--no-build", "--prefix", "/usr", "--destdir"]).arg(&s.0).arg("--bin").arg(bin).output().unwrap();
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "install.sh failed:\n{said}");
    let usr = s.0.join("usr");
    for f in ["bin/ectomatter", "share/applications/com.ectonomic.ectomatter.desktop", "share/icons/hicolor/scalable/apps/com.ectonomic.ectomatter.svg", "lib/udev/rules.d/70-ectomatter.rules", "share/licenses/ectomatter/LICENSE"] {
        assert!(usr.join(f).is_file(), "{f} not installed");
    }
    // Ectomatter's license, and every license some of its dependencies alone
    // come under (LICENSES/, which tools/licenses.sh holds to deny.toml's
    // exceptions), as they are
    assert!(std::fs::read(usr.join("share/licenses/ectomatter/LICENSE")).ok() == std::fs::read(repo().join("LICENSE")).ok(), "LICENSE not installed as it is");
    for e in std::fs::read_dir(repo().join("LICENSES")).into_iter().flatten().flatten() {
        let at = usr.join("share/licenses/ectomatter").join(e.file_name());
        assert!(std::fs::read(&at).ok() == std::fs::read(e.path()).ok(), "{} not installed as it is", e.path().display());
    }
    // every package that comes with Ectomatter, as installed: its manifest
    // and module, none of its source
    for kind in ["cores", "plugins"] {
        for e in std::fs::read_dir(repo().join(kind)).unwrap().flatten() {
            let id = e.file_name().to_string_lossy().into_owned();
            if !e.path().join("core.json").is_file() {
                continue;
            }
            let at = usr.join("share/ectomatter").join(kind).join(&id);
            assert!(at.join("core.json").is_file() && at.join("core.wasm").is_file(), "{kind}/{id} not installed whole");
            assert!(!at.join("module").exists(), "{kind}/{id}'s source installed");
        }
    }
    // the rules name every core's devices (made from the installed manifests)
    let rules = std::fs::read_to_string(usr.join("lib/udev/rules.d/70-ectomatter.rules")).unwrap();
    for e in std::fs::read_dir(repo().join("cores")).unwrap().flatten() {
        let Ok(text) = std::fs::read_to_string(e.path().join("core.json")) else { continue };
        let m = ectomatter_draw::jsonc::parse(&text).unwrap();
        for usb in m["matches"].as_array().into_iter().flatten().filter_map(|x| x.get("usb")) {
            let v = usb["vendor"].as_str().unwrap().to_lowercase();
            assert!(rules.contains(&format!("ATTRS{{idVendor}}==\"{v}\"")), "{v} not in the rules:\n{rules}");
        }
    }
    let entry = std::fs::read_to_string(usr.join("share/applications/com.ectonomic.ectomatter.desktop")).unwrap();
    assert!(entry.lines().any(|l| l == "Exec=ectomatter"), "the entry starts the installed program:\n{entry}");
    // started from where it's installed, with nothing of this computer's
    let home = s.0.join("home");
    let out = home.join("window.raw");
    std::fs::create_dir_all(&home).unwrap();
    let run = Command::new(usr.join("bin/ectomatter"))
        .env_clear()
        .arg("--snapshot")
        .arg(&out)
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("ECTOMATTER_SIZE", "1000x700")
        .env("ECTOMATTER_PLACE", "home")
        .current_dir(&home)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success() && std::fs::metadata(&out).is_ok_and(|m| m.len() == 1000 * 700 * 8), "not drawn:\n{said}");
    let share = usr.join("share/ectomatter");
    assert!(said.contains(&format!("its packages are in {}", usr.join("bin/../share/ectomatter").display())) || said.contains(&format!("its packages are in {}", share.display())), "not its installed packages:\n{said}");
    assert!(!said.contains(&repo().canonicalize().unwrap().display().to_string()), "it reached its source folder:\n{said}");
}
