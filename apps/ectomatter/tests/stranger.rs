//! Ectomatter as a stranger's computer has it: installed away from its
//! source, with fresh settings, none of Grant's data, and only the packages
//! it was given. Each test draws the window offscreen (`--snapshot`), which
//! opens no device, and checks it was drawn.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A folder of the test's own, empty, gone after it.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let d = std::env::temp_dir().join(format!("ectomatter-stranger-{name}-{}", std::process::id()));
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

/// A package's folder copied as an install has it: its files, not the
/// module's source or what building it left.
fn copy_package(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let name = e.file_name();
        let p = e.path();
        if name == "module" || name.to_string_lossy().contains(".before-") {
            continue;
        }
        if p.is_dir() {
            copy_package(&p, &to.join(&name));
        } else {
            std::fs::copy(&p, to.join(&name)).unwrap();
        }
    }
}

/// The packages that come with Ectomatter, copied under `share`, but those
/// `left_out`.
fn packages(share: &Path, left_out: &[&str]) {
    for folder in ["cores", "plugins"] {
        for e in std::fs::read_dir(repo().join(folder)).unwrap().flatten() {
            let id = e.file_name().to_string_lossy().into_owned();
            if e.path().join("core.json").exists() && !left_out.contains(&id.as_str()) {
                assert!(e.path().join("core.wasm").exists(), "{folder}/{id} isn't built: run tools/build-{folder}.sh");
                copy_package(&e.path(), &share.join(folder).join(&id));
            }
        }
    }
}

/// The window drawn offscreen at `place` by the program `exe`, with nothing
/// of this computer's settings or data: whether it was drawn, and what it said.
fn snapshot(exe: &Path, home: &Path, packages: Option<&Path>, place: &str) -> (bool, String) {
    let out = home.join("window.raw");
    let mut c = Command::new(exe);
    c.arg("--snapshot")
        .arg(&out)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("HOME", home)
        .env("ECTOMATTER_SIZE", "1000x700")
        .env("ECTOMATTER_PLACE", place)
        .env_remove("ECTOMATTER_PACKAGES")
        .current_dir(home);
    if let Some(p) = packages {
        c.env("ECTOMATTER_PACKAGES", p);
    }
    let run = c.output().unwrap();
    let said = String::from_utf8_lossy(&run.stderr).into_owned();
    let drawn = run.status.success() && std::fs::metadata(&out).is_ok_and(|m| m.len() == 1000 * 700 * 8);
    (drawn, said)
}

/// docs/deferred.md, stage 2: Ectomatter starts with any of its cores left
/// out, the keyboard's among them, and shows the devices it has.
#[test]
fn ectomatter_starts_without_the_keyboards_core() {
    let s = Scratch::new("no-keyboard");
    packages(&s.0.join("share"), &["evga-z10"]);
    for place in ["home", "settings", "z10"] {
        let (drawn, said) = snapshot(Path::new(env!("CARGO_BIN_EXE_ectomatter")), &s.0, Some(&s.0.join("share")), place);
        assert!(drawn, "{place} wasn't drawn:\n{said}");
        assert!(!said.contains("panicked"), "{place}:\n{said}");
        assert!(said.contains(&format!("its packages are in {}", s.0.join("share").display())), "{place}:\n{said}");
        assert!(!said.contains("the evga-z10 core"), "{place}: the keyboard's core was found:\n{said}");
    }
}

/// Installed as an install lays it out, `bin/ectomatter` beside
/// `share/ectomatter`, away from its source, it finds its own packages there.
#[test]
fn ectomatter_finds_its_packages_where_it_is_installed() {
    let s = Scratch::new("installed");
    let bin = s.0.join("usr/bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_ectomatter"), bin.join("ectomatter")).unwrap();
    packages(&s.0.join("usr/share/ectomatter"), &[]);
    let (drawn, said) = snapshot(&bin.join("ectomatter"), &s.0, None, "home");
    assert!(drawn, "Home wasn't drawn:\n{said}");
    assert!(said.contains(&format!("its packages are in {}", s.0.join("usr/bin/../share/ectomatter").display())) || said.contains(&format!("its packages are in {}", s.0.join("usr/share/ectomatter").display())), "not its own packages:\n{said}");
    assert!(said.contains("the weather-feed plugin started"), "the plugins it came with weren't found:\n{said}");
}
