//! Plugins' updates, from each one's own GitHub releases: once a day, for
//! each package whose manifest names its `repository`, Ectomatter reads the
//! manifest attached to the repository's latest release (core.json) and,
//! where its version is newer and it runs on this Ectomatter, downloads the
//! package attached beside it (<id>.ectocore, a zip of its folder) and puts
//! it in place as that version's folder (packages.rs). GitHub's own address
//! for a latest release's file is used, which needs no account and isn't
//! counted against the few questions its API answers without one.
//!
//! Add a plugin reads a repository the same way, for the form to say what
//! the plugin is and what it can do before it's added.
//!
//! When it last looked, and what it found, are kept in Ectomatter's data folder.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::packages::{self, Package};

/// Where releases are: GitHub, or for a check another address that answers
/// the same (ECTOMATTER_GITHUB).
fn base() -> String {
    std::env::var("ECTOMATTER_GITHUB").ok().filter(|b| !b.is_empty()).unwrap_or_else(|| "https://github.com".into())
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(60))).user_agent(concat!("Ectomatter/", env!("CARGO_PKG_VERSION"))).build().into()
}

/// A file of a repository's latest release, as bytes.
fn latest(repo: &str, file: &str) -> Result<Vec<u8>, String> {
    let url = format!("{}/{repo}/releases/latest/download/{file}", base());
    let r = agent().get(&url).call();
    match r {
        Ok(mut res) => res.body_mut().with_config().limit(64 << 20).read_to_vec().map_err(|e| e.to_string()),
        Err(ureq::Error::StatusCode(404)) => Err(format!("{repo} has no release with {file}")),
        Err(e) => Err(e.to_string()),
    }
}

/// The manifest of a repository's latest release.
pub fn release_manifest(repo: &str) -> Result<Value, String> {
    let b = latest(repo, "core.json")?;
    let t = String::from_utf8(b).map_err(|_| "its core.json isn’t text".to_string())?;
    ectomatter_draw::jsonc::parse(&t).map_err(|e| format!("its core.json can’t be read: {e}"))
}

/// Whether a manifest's package runs on this Ectomatter.
pub fn runs_here(m: &Value) -> Result<(), String> {
    let needs = m["ectomatter"].as_str().unwrap_or("0");
    if packages::version_key(needs) > packages::version_key(env!("CARGO_PKG_VERSION")) {
        return Err(format!("it needs Ectomatter {needs} or later"));
    }
    let speaks = ectomatter_protocol::PROTOCOL as u64;
    match m["protocol"].as_u64() {
        Some(p) if p > speaks => return Err("it speaks a newer core protocol than this Ectomatter".into()),
        Some(p) if p < speaks => return Err("it speaks an older core protocol than this Ectomatter".into()),
        Some(_) => {}
        None => return Err("it doesn’t say which core protocol it speaks".into()),
    }
    if !packages::works_here(m) {
        let names: Vec<String> = packages::systems_of(m).iter().map(|s| s.to_string()).collect();
        return Err(format!("it works on {}, not on {}", packages::joined(&names), packages::this_system()));
    }
    Ok(())
}

/// A repository's latest release put in place (its package downloaded and
/// unpacked into `into`): its manifest.
pub fn install_latest(repo: &str, id: &str, into: &Path) -> Result<crate::core_host::Manifest, String> {
    let bytes = latest(repo, &format!("{id}.ectocore"))?;
    let tmp = std::env::temp_dir().join(format!("ectomatter-{id}-{}.ectocore", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    let m = packages::install_file(&tmp, into);
    let _ = std::fs::remove_file(&tmp);
    let m = m?;
    if m.id != id {
        return Err(format!("its package is {}, not {id}", m.id));
    }
    // (where it came from, for its updates, whether or not its manifest says)
    let _ = std::fs::write(into.join(id).join("source.json"), json!({ "repository": repo }).to_string());
    Ok(m)
}

/// Where a package's updates stand.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    UpToDate(u64),
    Updated(String),
    Failed(String),
}

/// Where the record of the checks is kept.
fn kept_at() -> Option<PathBuf> {
    let offscreen = cfg!(test) || std::env::args().any(|a| a == "--snapshot");
    (!offscreen).then(|| crate::data_dir().join("updates.json"))
}

fn record() -> Value {
    kept_at().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| json!({}))
}

/// How a package's updates stand, in words for its row in Settings.
pub fn status_of(p: &Package) -> String {
    let r = &record()[p.id()];
    match r["status"].as_str() {
        Some("updated") => format!("Updated to {}", r["version"].as_str().unwrap_or(p.version())),
        Some("failed") => "Couldn’t check for updates".into(),
        Some("up to date") => "Up to date".into(),
        _ => "Not checked yet".into(),
    }
}

/// The checks, on a thread of their own: each package with a repository
/// looked at once a day (and a moment after Ectomatter starts, where a day has
/// passed); a package updated is told by its id, for its plugin to start again.
pub struct Checker {
    pub updated: mpsc::Receiver<String>,
}

impl Checker {
    pub fn start() -> Checker {
        let (tx, updated) = mpsc::channel();
        if kept_at().is_some() {
            let _ = std::thread::Builder::new().name("updates".into()).spawn(move || {
                std::thread::sleep(Duration::from_secs(30));
                loop {
                    check_all(&tx);
                    std::thread::sleep(Duration::from_secs(60 * 60));
                }
            });
        }
        Checker { updated }
    }
}

/// Each package with a repository, looked at where a day has passed since it last was.
fn check_all(tx: &mpsc::Sender<String>) {
    let mut rec = record();
    let now = crate::games::now();
    for p in packages::all() {
        let Some(repo) = p.repository() else { continue };
        if rec[p.id()]["at"].as_u64().is_some_and(|t| now.saturating_sub(t) < 24 * 60 * 60) {
            continue;
        }
        let s = check(&p, &repo, &packages::added_dir(p.kind()));
        rec[p.id()] = match &s {
            Status::UpToDate(at) => json!({ "status": "up to date", "at": at }),
            Status::Updated(v) => json!({ "status": "updated", "version": v, "at": now }),
            Status::Failed(why) => json!({ "status": "failed", "why": why, "at": now }),
        };
        if let Some(path) = kept_at() {
            let _ = std::fs::write(path, serde_json::to_vec_pretty(&rec).unwrap_or_default());
        }
        if let Status::Updated(_) = s {
            let _ = tx.send(p.id().to_string());
        }
    }
}

/// One package checked: its latest release's version, and the package put
/// in place (under `into`) if that's newer and runs here.
pub fn check(p: &Package, repo: &str, into: &Path) -> Status {
    let m = match release_manifest(repo) {
        Ok(m) => m,
        Err(e) => return Status::Failed(e),
    };
    let v = m["version"].as_str().unwrap_or("0");
    if m["id"].as_str() != Some(p.id()) {
        return Status::Failed(format!("{repo}’s latest release is another plugin"));
    }
    if packages::version_key(v) <= packages::version_key(p.version()) {
        return Status::UpToDate(crate::games::now());
    }
    if let Err(why) = runs_here(&m) {
        return Status::Failed(format!("version {v} can’t be used: {why}"));
    }
    match install_latest(repo, p.id(), into) {
        Ok(m) => Status::Updated(m.version),
        Err(e) => Status::Failed(e),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// The Monitor Switcher as a package file of another version: its
    /// folder, zipped.
    pub fn package_of(version: &str) -> (Vec<u8>, Vec<u8>) {
        let dir = crate::cores::plugins_dir().join("monitor-switcher");
        let manifest = std::fs::read_to_string(dir.join("core.json")).unwrap().replace("\"version\": \"1.0.0\"", &format!("\"version\": \"{version}\""));
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            z.start_file("monitor-switcher/core.json", o).unwrap();
            z.write_all(manifest.as_bytes()).unwrap();
            for f in ["core.wasm", "glyph.svg"] {
                z.start_file(format!("monitor-switcher/{f}"), o).unwrap();
                z.write_all(&std::fs::read(dir.join(f)).unwrap()).unwrap();
            }
            z.finish().unwrap();
        }
        (manifest.into_bytes(), out.into_inner())
    }

    /// A stand-in for GitHub's releases on this computer: a repository's
    /// latest release's files by name, everything else not found. Its address.
    pub fn fake_github(files: Vec<(String, Vec<u8>)>) -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let at = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for c in l.incoming().flatten() {
                let mut c = c;
                let mut buf = [0u8; 4096];
                let n = c.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("").to_string();
                let found = files.iter().find(|(p, _)| *p == path);
                let (status, body): (&str, &[u8]) = match found {
                    Some((_, b)) => ("200 OK", b),
                    None => ("404 Not Found", b"not found"),
                };
                let _ = write!(c, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = c.write_all(body);
            }
        });
        at
    }

    /// A newer release is put in place beside the one Ectomatter came with and
    /// is then the one used; the same version is up to date; another plugin's
    /// release isn't taken.
    #[test]
    fn a_newer_release_is_put_in_place() {
        let (manifest, package) = package_of("1.0.1");
        let base = "/test/monitor-switcher/releases/latest/download";
        let at = fake_github(vec![(format!("{base}/core.json"), manifest), (format!("{base}/monitor-switcher.ectocore"), package)]);
        // (set for this test alone: no other test asks GitHub)
        unsafe { std::env::set_var("ECTOMATTER_GITHUB", &at) };
        let into = std::env::temp_dir().join(format!("ectomatter-updates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&into);
        let p = Package::read(&crate::cores::plugins_dir().join("monitor-switcher"), false).unwrap();
        assert_eq!(check(&p, "test/monitor-switcher", &into), Status::Updated("1.0.1".into()));
        assert!(into.join("monitor-switcher/1.0.1/core.wasm").is_file() && !into.join("monitor-switcher/1.0.1/.unfinished").exists());
        let all = packages::all_in(&crate::cores::plugins_dir(), &into);
        let used = all.iter().find(|x| x.id() == "monitor-switcher").unwrap();
        assert_eq!((used.version(), used.added), ("1.0.1", true));
        assert_eq!(used.repository().as_deref(), Some("test/monitor-switcher"), "it knows where its updates come from");
        // the version it has is up to date
        assert!(matches!(check(used, "test/monitor-switcher", &into), Status::UpToDate(_)));
        // a release of nothing Ectomatter knows by that name isn't taken
        assert!(matches!(check(&p, "test/elsewhere", &into), Status::Failed(_)));
        let _ = std::fs::remove_dir_all(&into);
        unsafe { std::env::remove_var("ECTOMATTER_GITHUB") };
    }
}
