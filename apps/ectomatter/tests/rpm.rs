//! Ectomatter as an RPM (tools/package-rpm.sh), made from the programs this
//! test was built with, unpacked into a folder of the test's own as rpm
//! would put it under /, and started from there with fresh settings: it
//! holds what tools/install.sh lays out, file for file, at the app's version
//! and under its license.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Every file under `root`, by its path from there.
fn files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                todo.push(p);
            } else {
                out.push(p.strip_prefix(root).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn the_rpm_holds_the_install_and_starts_from_it() {
    let s = Scratch(std::env::temp_dir().join(format!("ectomatter-rpm-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&s.0);
    let bin = Path::new(env!("CARGO_BIN_EXE_ectomatter")).parent().unwrap();
    assert!(bin.join("ectomatter-launch").is_file(), "no {}: cargo build --release -p ectomatter-launch", bin.join("ectomatter-launch").display());
    let made = Command::new(repo().join("tools/package-rpm.sh")).args(["--no-build", "--bin"]).arg(bin).arg("--out").arg(s.0.join("out")).output().unwrap();
    assert!(made.status.success(), "package-rpm.sh failed:\n{}", String::from_utf8_lossy(&made.stderr));
    let rpm = PathBuf::from(String::from_utf8_lossy(&made.stdout).lines().last().unwrap_or_default());
    assert!(rpm.is_file(), "no package:\n{}", String::from_utf8_lossy(&made.stdout));

    // its version and license, the app's own
    let q = Command::new("rpm").args(["-qp", "--qf", "%{VERSION} %{LICENSE}"]).arg(&rpm).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&q.stdout), format!("{} {}", env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_LICENSE")));

    // unpacked as rpm puts it under /, it's the install, file for file
    let root = s.0.join("root");
    std::fs::create_dir_all(&root).unwrap();
    let cpio = Command::new("rpm2cpio").arg(&rpm).stdout(Stdio::piped()).spawn().unwrap();
    let unpacked = Command::new("cpio").args(["-idm", "--quiet"]).current_dir(&root).stdin(cpio.stdout.unwrap()).output().unwrap();
    assert!(unpacked.status.success(), "cpio: {}", String::from_utf8_lossy(&unpacked.stderr));
    let staged = s.0.join("staged");
    let run = Command::new(repo().join("tools/install.sh")).args(["--no-build", "--prefix", "/usr", "--destdir"]).arg(&staged).arg("--bin").arg(bin).output().unwrap();
    assert!(run.status.success(), "install.sh failed:\n{}", String::from_utf8_lossy(&run.stderr));
    let (have, want) = (files(&root), files(&staged));
    assert!(want.len() > 20, "nothing installed: {want:?}");
    assert_eq!(have, want, "the package isn't the install");

    // started from where it's unpacked, with nothing of this computer's
    let home = s.0.join("home");
    let out = home.join("window.raw");
    std::fs::create_dir_all(&home).unwrap();
    let usr = root.join("usr");
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
        .env("ECTOMATTER_PLACE", "settings")
        .current_dir(&home)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success() && std::fs::metadata(&out).is_ok_and(|m| m.len() == 1000 * 700 * 8), "not drawn:\n{said}");
    assert!(said.contains(&format!("its packages are in {}", usr.join("share/ectomatter").display())), "not its packages:\n{said}");
    assert!(said.contains(&format!("its license and notices are read from {}", usr.join("share/licenses/ectomatter").display())), "not its license:\n{said}");
}
