//! Plugins' updates, from each one's own GitHub releases: once a day, for
//! each package whose manifest names its `repository`, Ectomatter reads the
//! manifest attached to the repository's latest release (core.json) and,
//! where its version is newer, runs on this Ectomatter and can do nothing
//! more on the computer than the one in use, downloads the package attached
//! beside it (<id>.ectocore, a zip of its folder), checks it against the
//! checksum attached beside that (<id>.ectocore.sha256, its SHA-256 in hex)
//! and puts it in place as that version's folder (packages.rs). A version
//! that can do more waits for the person: Settings offers it to review, in
//! the form a package is added from. GitHub's own address
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
/// unpacked into `into`): its manifest. Only the package `expected` says,
/// the manifest that was checked or shown to the person: the same package
/// and version, reaching nothing more than it says.
pub fn install_latest(repo: &str, id: &str, expected: &Value, into: &Path) -> Result<crate::core_host::Manifest, String> {
    let bytes = latest(repo, &format!("{id}.ectocore"))?;
    // (only the package its release says it is: its SHA-256, as the release gives it)
    let sum = latest(repo, &format!("{id}.ectocore.sha256")).map_err(|_| format!("its release gives no checksum ({id}.ectocore.sha256), so Ectomatter can’t tell the package is whole"))?;
    let said = String::from_utf8_lossy(&sum).split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    if said != sha256(&bytes) {
        return Err("its package doesn’t match the checksum its release gives".into());
    }
    let tmp = std::env::temp_dir().join(format!("ectomatter-{id}-{}.ectocore", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    let m = packages::manifest_in_file(&tmp).and_then(|inside| as_expected(&inside, expected)).and_then(|_| packages::install_file(&tmp, into));
    let _ = std::fs::remove_file(&tmp);
    let m = m?;
    if m.id != id {
        return Err(format!("its package is {}, not {id}", m.id));
    }
    // (where it came from, for its updates, whether or not its manifest says)
    let _ = std::fs::write(into.join(id).join("source.json"), json!({ "repository": repo }).to_string());
    Ok(m)
}

/// Whether the manifest inside a package is the one that was checked or
/// shown: the same id and version, reaching nothing the other doesn't.
fn as_expected(inside: &Value, expected: &Value) -> Result<(), String> {
    if inside["id"] != expected["id"] || inside["version"] != expected["version"] {
        return Err(format!(
            "its package is {} {}, not the {} {} its release describes",
            inside["id"].as_str().unwrap_or("?"),
            inside["version"].as_str().unwrap_or("?"),
            expected["id"].as_str().unwrap_or("?"),
            expected["version"].as_str().unwrap_or("?")
        ));
    }
    if !more_than(inside, expected).is_empty() {
        return Err("its package can do more on the computer than its release describes".into());
    }
    Ok(())
}

/// A file's SHA-256, in hex.
fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Everything a manifest lets its package reach, each once, in a form two
/// manifests can be compared by: each USB device it matches, each address
/// and web host by itself (not the name it gives it), each file or folder,
/// and each other use.
fn reach(m: &Value) -> Vec<String> {
    let u = crate::core_host::Uses::of(&m["uses"]);
    let mut out: Vec<String> = m["matches"].as_array().into_iter().flatten().map(|x| format!("usb {x}")).collect();
    out.extend(u.network.iter().map(|(a, _)| format!("network {a}")));
    out.extend(u.web.iter().map(|(h, _)| format!("web {h}")));
    out.extend(u.read.iter().map(|r| format!("read {r}")));
    out.extend([(u.monitors, "monitors"), (u.nvidia, "nvidia"), (u.launch, "launch")].into_iter().filter(|(on, _)| *on).map(|(_, w)| w.to_string()));
    out.extend(u.other.iter().cloned());
    // (each program a game may run through, each option it may be given, and
    // each variable a game may be given, each with every way its value may be
    // written, so a wider one is more)
    for w in &u.starts.wrappers {
        out.push(format!("wrap {}", w.program));
        for (o, how) in &w.options {
            out.push(format!("wrap {} {o}", w.program));
            out.extend(how.iter().flat_map(|h| h.within()).map(|k| format!("wrap {} {o} as {}", w.program, k.name())));
        }
        out.extend(w.last.iter().map(|l| format!("wrap {} {l}", w.program)));
    }
    out.extend(u.starts.tools.iter().map(|p| format!("tool {p}")));
    for (k, how) in &u.starts.variables {
        out.push(format!("env {k}"));
        out.extend(how.within().iter().map(|h| format!("env {k} as {}", h.name())));
    }
    out
}

/// What a package's manifest lets it reach that the one in use doesn't, in
/// words for the person: a web host or an address named as the manifest
/// names it and by itself, so a new one is never hidden behind a name the
/// person knows.
pub fn more_than(new: &Value, now: &Value) -> Vec<String> {
    let had = reach(now);
    let u = crate::core_host::Uses::of(&new["uses"]);
    let more: Vec<String> = reach(new).into_iter().filter(|r| !had.contains(r)).collect();
    let mut out: Vec<String> = Vec::new();
    for r in &more {
        // (a wider way of writing is said only of what was there before)
        if r.split_once(" as ").is_some_and(|(of, _)| more.iter().any(|m| m == of)) {
            continue;
        }
        let parts: Vec<&str> = r.split(' ').collect();
        let words = match parts[..] {
            ["web", h] => format!("Reaches {} over the internet, at {h}.", u.web.iter().find(|(x, _)| x == h).map_or(h, |(_, to)| to.as_str())),
            ["network", a] => format!("Talks to {} at {a}.", u.network.iter().find(|(x, _)| x == a).map_or(a, |(_, to)| to.as_str())),
            ["read", p] => format!("Reads {p} without changing it."),
            ["usb", ..] => "Talks to another USB device.".into(),
            ["wrap", p] => format!("Runs games through {p}."),
            ["wrap", p, o] => format!("Runs {p} with the option {o}."),
            ["wrap", p, o, "as", _] => format!("Runs {p} with more kinds of values for the option {o}."),
            ["tool", p] => format!("Runs {p} in a game’s prefix."),
            ["env", k] => format!("Sets {k} for each game."),
            ["env", k, "as", _] => format!("Sets {k} to more kinds of values than before."),
            _ => packages::can_words(&json!({ "uses": [r] })).into_iter().next().unwrap_or(r.clone()),
        };
        if !out.contains(&words) {
            out.push(words);
        }
    }
    out
}

/// Where a package's updates stand: up to date (when it was checked); a
/// version put in place; a version that can do more, with what more,
/// waiting for the person; or the check failing, with why.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    UpToDate(u64),
    Updated(String),
    Asks(String, Vec<String>),
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
        // (a version waiting for the person, until it's the one in use)
        Some("asks") => match r["version"].as_str() {
            Some(v) if packages::version_key(v) > packages::version_key(p.version()) => format!("{v} asks to do more: waiting for your OK"),
            Some(v) => format!("Updated to {v}"),
            None => "Up to date".into(),
        },
        _ => "Not checked yet".into(),
    }
}

/// The version of a package waiting for the person, newer than the one in
/// use and able to do more, if there's one.
pub fn waiting(p: &Package) -> Option<String> {
    let r = &record()[p.id()];
    let v = r["version"].as_str().filter(|_| r["status"] == "asks")?;
    (packages::version_key(v) > packages::version_key(p.version())).then(|| v.to_string())
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
            Status::Asks(v, more) => json!({ "status": "asks", "version": v, "more": more, "at": now }),
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
    // (one that can do more on the computer waits for the person to say so)
    let more = more_than(&m, &p.manifest.all);
    if !more.is_empty() {
        return Status::Asks(v.to_string(), more);
    }
    match install_latest(repo, p.id(), &m, into) {
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
        package_using(version, "[\"monitors\"]")
    }

    /// The same, its manifest's `uses` these.
    pub fn package_using(version: &str, uses: &str) -> (Vec<u8>, Vec<u8>) {
        let dir = crate::cores::plugins_dir().join("monitor-switcher");
        let manifest = std::fs::read_to_string(dir.join("core.json")).unwrap().replace("\"version\": \"1.0.0\"", &format!("\"version\": \"{version}\"")).replace("\"uses\": [\"monitors\"]", &format!("\"uses\": {uses}"));
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

    /// One test at a time names its fake GitHub (ECTOMATTER_GITHUB is the whole process's).
    static GITHUB: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
        let _one = GITHUB.lock().unwrap_or_else(|p| p.into_inner());
        let (manifest, package) = package_of("1.0.1");
        let base = "/test/monitor-switcher/releases/latest/download";
        let sum = format!("{}  monitor-switcher.ectocore\n", sha256(&package)).into_bytes();
        let at = fake_github(vec![(format!("{base}/core.json"), manifest), (format!("{base}/monitor-switcher.ectocore"), package), (format!("{base}/monitor-switcher.ectocore.sha256"), sum)]);
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

    /// A newer release that can do more on the computer than the version in
    /// use waits for the person, saying what more; one whose package doesn't
    /// match the checksum its release gives, or that gives none, isn't put
    /// in place. Each repository a fake GitHub of its own.
    #[test]
    fn an_update_that_can_do_more_waits_and_a_package_must_match_its_checksum() {
        let _one = GITHUB.lock().unwrap_or_else(|p| p.into_inner());
        let p = Package::read(&crate::cores::plugins_dir().join("monitor-switcher"), false).unwrap();
        let into = std::env::temp_dir().join(format!("ectomatter-updates-more-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&into);
        let base = "/test/monitor-switcher/releases/latest/download";
        // (asks to reach the web as well: waits, and nothing is downloaded)
        let (manifest, package) = package_using("1.1.0", "[\"monitors\", { \"web\": \"example.com\", \"to\": \"Example\" }]");
        let at = fake_github(vec![(format!("{base}/core.json"), manifest), (format!("{base}/monitor-switcher.ectocore"), package)]);
        unsafe { std::env::set_var("ECTOMATTER_GITHUB", &at) };
        match check(&p, "test/monitor-switcher", &into) {
            Status::Asks(v, more) => {
                assert_eq!(v, "1.1.0");
                assert_eq!(more.len(), 1, "{more:?}");
                assert_eq!(more[0], "Reaches Example over the internet, at example.com.");
            }
            other => panic!("{other:?}"),
        }
        assert!(!into.join("monitor-switcher").exists(), "nothing put in place");
        // (the same name for another host still asks)
        let renamed = json!({ "uses": [{ "web": "elsewhere.example", "to": "Example" }] });
        assert_eq!(more_than(&renamed, &json!({ "uses": [{ "web": "example.com", "to": "Example" }] })), ["Reaches Example over the internet, at elsewhere.example."]);
        assert!(more_than(&json!({ "uses": ["monitors"] }), &json!({ "uses": ["monitors", "nvidia"] })).is_empty(), "less is no more");
        // (a game's start: a new program, a new option, a new variable and a wider one are each more)
        let start = |wrap: Value, env: Value| json!({ "uses": [{ "launch": { "wrap": wrap, "env": env } }] });
        let now = start(json!({ "foowrap": { "options": { "--level": "number" } } }), json!({ "FOO_TUNE": "number" }));
        let new = start(json!({ "foowrap": { "options": { "--level": "number", "--fast": null } }, "barwrap": {} }), json!({ "FOO_TUNE": "list", "FOO_NEW": "word" }));
        let mut more = more_than(&new, &now);
        more.sort();
        assert_eq!(more, ["Runs foowrap with the option --fast.", "Runs games through barwrap.", "Sets FOO_NEW for each game.", "Sets FOO_TUNE to more kinds of values than before."]);
        assert!(more_than(&now, &new).is_empty(), "less is no more");
        // (a package that isn't what its checksum says, and one with no checksum)
        let (manifest, package) = package_of("1.0.2");
        let at = fake_github(vec![(format!("{base}/core.json"), manifest.clone()), (format!("{base}/monitor-switcher.ectocore"), package.clone()), (format!("{base}/monitor-switcher.ectocore.sha256"), b"0000  monitor-switcher.ectocore".to_vec())]);
        unsafe { std::env::set_var("ECTOMATTER_GITHUB", &at) };
        assert_eq!(check(&p, "test/monitor-switcher", &into), Status::Failed("its package doesn’t match the checksum its release gives".into()));
        let at = fake_github(vec![(format!("{base}/core.json"), manifest), (format!("{base}/monitor-switcher.ectocore"), package)]);
        unsafe { std::env::set_var("ECTOMATTER_GITHUB", &at) };
        assert!(matches!(check(&p, "test/monitor-switcher", &into), Status::Failed(why) if why.contains("gives no checksum")));
        assert!(!into.join("monitor-switcher").exists(), "nothing put in place");
        // (a whole package whose own manifest isn't the one its release
        // describes: another version, or reaching more)
        let (described, _) = package_of("1.0.2");
        for (inside, why) in [(package_of("1.0.3").1, "not the monitor-switcher 1.0.2"), (package_using("1.0.2", "[\"monitors\", { \"web\": \"example.com\", \"to\": \"Example\" }]").1, "can do more on the computer")] {
            let sum = format!("{}  monitor-switcher.ectocore", sha256(&inside)).into_bytes();
            let at = fake_github(vec![(format!("{base}/core.json"), described.clone()), (format!("{base}/monitor-switcher.ectocore"), inside), (format!("{base}/monitor-switcher.ectocore.sha256"), sum)]);
            unsafe { std::env::set_var("ECTOMATTER_GITHUB", &at) };
            assert!(matches!(check(&p, "test/monitor-switcher", &into), Status::Failed(w) if w.contains(why)), "{why}");
            assert!(!into.join("monitor-switcher").exists(), "nothing put in place");
        }
        let _ = std::fs::remove_dir_all(&into);
        unsafe { std::env::remove_var("ECTOMATTER_GITHUB") };
    }
}
