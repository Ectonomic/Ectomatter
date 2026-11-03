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

/// The libraries a project ships, `name version`: every crate from a
/// registry its crates reach, on every system and with every feature, not
/// through the tests' own dependencies; and its crates' folders.
fn shipped(manifest: &Path) -> (Vec<String>, Vec<PathBuf>) {
    let out = Command::new(std::env::var("CARGO").unwrap_or("cargo".into()))
        .args(["metadata", "--locked", "--all-features", "--format-version", "1", "--manifest-path"])
        .arg(manifest)
        .output()
        .unwrap();
    assert!(out.status.success(), "cargo metadata: {}", String::from_utf8_lossy(&out.stderr));
    let md: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let packages: std::collections::HashMap<&str, &serde_json::Value> = md["packages"].as_array().unwrap().iter().map(|p| (p["id"].as_str().unwrap(), p)).collect();
    let nodes: std::collections::HashMap<&str, &serde_json::Value> = md["resolve"]["nodes"].as_array().unwrap().iter().map(|n| (n["id"].as_str().unwrap(), n)).collect();
    let members: Vec<&str> = md["workspace_members"].as_array().unwrap().iter().filter_map(|m| m.as_str()).collect();
    let (mut seen, mut todo) = (std::collections::HashSet::new(), members.clone());
    while let Some(id) = todo.pop() {
        if !seen.insert(id) {
            continue;
        }
        for d in nodes[id]["deps"].as_array().unwrap() {
            if d["dep_kinds"].as_array().unwrap().iter().any(|k| k["kind"] != "dev") {
                todo.push(d["pkg"].as_str().unwrap());
            }
        }
    }
    let mut libraries: Vec<String> = seen.iter().map(|id| packages[id]).filter(|p| !p["source"].is_null()).map(|p| format!("{} {}", p["name"].as_str().unwrap(), p["version"].as_str().unwrap())).collect();
    libraries.sort();
    let folders = members.iter().map(|id| Path::new(packages[id]["manifest_path"].as_str().unwrap()).parent().unwrap().to_path_buf()).collect();
    (libraries, folders)
}

/// Whether a file of the repository (by its path in it) shows another's
/// work as it is: a typeface, or a file naming its makers' copyright.
fn shows_anothers(f: &str) -> bool {
    let font = [".ttf", ".otf", ".woff", ".woff2"].iter().any(|x| f.to_lowercase().ends_with(x));
    font || std::fs::read_to_string(repo().join(f)).is_ok_and(|t| t.contains(concat!("<copy", "right>")) || t.contains(concat!("SPDX-File", "CopyrightText")))
}

/// The repository's files under these folders that show another's work.
fn carried(folders: &[PathBuf]) -> Vec<String> {
    let root = repo().canonicalize().unwrap();
    let out = Command::new("git").arg("-C").arg(&root).args(["ls-files", "-z"]).output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|f| !f.is_empty() && folders.iter().any(|d| root.join(f).starts_with(d.canonicalize().unwrap())))
        .filter(|f| shows_anothers(f))
        .map(str::to_owned)
        .collect()
}

/// The files a package's release carries, as the install lays them out
/// (tools/release-package.py --list), each by its path in the repository.
fn released(kind: &str, id: &str) -> Vec<String> {
    let out = Command::new("python3").args(["-I"]).arg(repo().join("tools/release-package.py")).arg("--list").arg(repo().join(kind).join(id)).output().unwrap();
    assert!(out.status.success(), "release-package.py --list: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).lines().map(|f| format!("{kind}/{id}/{f}")).collect()
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
    // the notices: each library the app ships, found from Cargo's own
    // record of what it's built from, and each file of its crates' folders
    // that shows another's work, named in NOTICES
    let notices = std::fs::read_to_string(usr.join("share/licenses/ectomatter/NOTICES")).unwrap_or_default();
    let (libraries, folders) = shipped(&repo().join("Cargo.toml"));
    assert!(libraries.len() > 100, "no libraries found: {}", libraries.len());
    for l in &libraries {
        assert!(notices.lines().any(|n| n.starts_with(&format!("{l} ")) || n == l), "{l} isn't in the notices");
    }
    // the guide Build with us opens, as it is in the source
    assert!(std::fs::read(usr.join("share/doc/ectomatter/making-a-package.md")).ok() == std::fs::read(repo().join("docs/making-a-package.md")).ok(), "the guide to making a package isn't installed as it is");
    // the credits About shows: the history's authors
    let credits: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(usr.join("share/licenses/ectomatter/credits.json")).unwrap_or_default()).unwrap_or_default();
    assert!(credits["authors"].as_array().is_some_and(|a| !a.is_empty()), "the credits aren't installed: {credits}");
    for f in carried(&folders) {
        assert!(notices.lines().any(|n| n == f), "{f} isn't in the notices");
    }
    // every package that comes with Ectomatter, as installed: its manifest
    // and module, none of its source, and its own notices
    for kind in ["cores", "plugins"] {
        for e in std::fs::read_dir(repo().join(kind)).unwrap().flatten() {
            let id = e.file_name().to_string_lossy().into_owned();
            if !e.path().join("core.json").is_file() {
                continue;
            }
            let at = usr.join("share/ectomatter").join(kind).join(&id);
            assert!(at.join("core.json").is_file() && at.join("core.wasm").is_file(), "{kind}/{id} not installed whole");
            assert!(!at.join("module").exists(), "{kind}/{id}'s source installed");
            // nor what its own repository keeps beside it
            for kept in ["crates", "tools", "parity", "Cargo.toml", "README.md", "REUSE.toml", ".git"] {
                assert!(!at.join(kept).exists(), "{kind}/{id}'s {kept} installed");
            }
            if e.path().join("module/Cargo.toml").is_file() {
                let notices = std::fs::read_to_string(at.join("NOTICES")).unwrap_or_default();
                let (libraries, _) = shipped(&e.path().join("module/Cargo.toml"));
                for l in libraries {
                    assert!(notices.lines().any(|n| n.starts_with(&format!("{l} ")) || n == l), "{l} isn't in {kind}/{id}'s notices");
                }
                // (each file it carries from others: what its release carries, not
                // what its own repository keeps beside it)
                for f in released(kind, &id).into_iter().filter(|f| shows_anothers(f)) {
                    assert!(notices.lines().any(|n| n == f), "{f} isn't in {kind}/{id}'s notices");
                }
            }
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
    // and Settings' About reads its license and notices where the install put them
    let read = said.lines().find_map(|l| l.strip_prefix("ectomatter: its license and notices are read from ")).map(PathBuf::from);
    assert!(read.as_ref().is_some_and(|d| d.join("LICENSE").is_file() && d.join("NOTICES").is_file()), "its license and notices aren't where it reads them ({read:?}):\n{said}");
}
