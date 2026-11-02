//! The list of packages Ectonomic reviewed (`registry/packages.json`, the
//! file the list's own repository keeps, Ectonomic/ectomatter-packages): each
//! core and plugin by the repository it's released from and who publishes
//! it, each version by the manifest that was reviewed and the SHA-256 its
//! package must have. `registry/check.py` holds every change to the list to
//! that before it's merged. A package from the list is downloaded from that
//! version's GitHub release and taken only if it matches the checksum the
//! list pins, whatever the release itself says, so a package changed after
//! it was reviewed isn't taken.
//!
//! The list is read from ECTOMATTER_REGISTRY (an address, or a file, for a
//! check), else from the list's repository on GitHub, and kept in
//! Ectomatter's cache, read again once a day.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};

use crate::packages;

/// Where the list is.
fn address() -> String {
    std::env::var("ECTOMATTER_REGISTRY").ok().filter(|a| !a.is_empty()).unwrap_or_else(|| "https://raw.githubusercontent.com/Ectonomic/ectomatter-packages/main/packages.json".into())
}

/// Whether the list is a file on this computer (read at once, not from the web).
pub fn is_local() -> bool {
    let at = address();
    !(at.starts_with("https://") || at.starts_with("http://"))
}

/// A package on the list, at the newest version of it that runs on this
/// Ectomatter: where it's released, who publishes it, the manifest that was
/// reviewed, and the checksum its package must have.
#[derive(Clone, Debug, PartialEq)]
pub struct Listed {
    pub repository: String,
    pub publisher: String,
    pub manifest: Value,
    pub sha256: String,
}

impl Listed {
    pub fn id(&self) -> &str {
        self.manifest["id"].as_str().unwrap_or("")
    }
    pub fn version(&self) -> &str {
        self.manifest["version"].as_str().unwrap_or("")
    }
    pub fn kind(&self) -> packages::Kind {
        packages::Kind::of(&self.manifest)
    }
}

/// The list's packages: of each entry, its newest version that runs here
/// (an entry with none that does is left out).
pub fn of(list: &Value) -> Result<Vec<Listed>, String> {
    if list["format"].as_u64() != Some(1) {
        return Err("the list isn’t one this Ectomatter reads".into());
    }
    let mut out = Vec::new();
    for e in list["packages"].as_array().into_iter().flatten() {
        let mut versions: Vec<&Value> = e["versions"].as_array().into_iter().flatten().filter(|v| v["manifest"].is_object() && crate::updates::runs_here(&v["manifest"]).is_ok()).collect();
        versions.sort_by_key(|v| packages::version_key(v["manifest"]["version"].as_str().unwrap_or("0")));
        let Some(v) = versions.last() else { continue };
        out.push(Listed {
            repository: e["repository"].as_str().unwrap_or("").to_string(),
            publisher: e["publisher"].as_str().unwrap_or("").to_string(),
            manifest: v["manifest"].clone(),
            sha256: v["sha256"].as_str().unwrap_or("").to_ascii_lowercase(),
        });
    }
    Ok(out)
}

fn kept_at() -> PathBuf {
    crate::cache_dir().join("packages.json")
}

/// The list as it was last read, and how long ago.
pub fn cached() -> Option<(Vec<Listed>, Duration)> {
    let f = kept_at();
    let age = std::fs::metadata(&f).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok())?;
    let v: Value = serde_json::from_slice(&std::fs::read(&f).ok()?).ok()?;
    of(&v).ok().map(|l| (l, age))
}

/// The list read now, and kept.
pub fn fetch() -> Result<Vec<Listed>, String> {
    let at = address();
    let bytes = if at.starts_with("https://") || at.starts_with("http://") {
        crate::updates::get(&at)?
    } else {
        std::fs::read(&at).map_err(|e| format!("{at}: {e}"))?
    };
    let v: Value = serde_json::from_slice(&bytes).map_err(|e| format!("the list can’t be read: {e}"))?;
    let listed = of(&v)?;
    // (kept for the next start and the daily check; a snapshot keeps nothing)
    if !crate::desk::keeps_nothing() {
        let _ = std::fs::create_dir_all(crate::cache_dir()).and_then(|_| std::fs::write(kept_at(), &bytes));
    }
    Ok(listed)
}

/// The list kept, if it was read within the day, else read now (the kept one
/// where it can't be).
pub fn list() -> Result<Vec<Listed>, String> {
    match cached() {
        Some((l, age)) if age < Duration::from_secs(24 * 3600) => Ok(l),
        kept => fetch().or_else(|e| kept.map(|(l, _)| l).ok_or(e)),
    }
}

/// A package from the list put in place under `into`: its version's release
/// file downloaded, taken only if it matches the checksum the list pins and
/// its manifest is the one that was reviewed. Its manifest.
pub fn install(l: &Listed, into: &Path) -> Result<crate::core_host::Manifest, String> {
    let id = l.id().to_string();
    let bytes = crate::updates::get(&format!("{}/{}/releases/download/{}/{id}.ectocore", crate::updates::base(), l.repository, l.version()))?;
    if crate::updates::sha256(&bytes) != l.sha256 {
        return Err("its package isn’t the one Ectonomic reviewed: it doesn’t match the checksum on the list".into());
    }
    let tmp = std::env::temp_dir().join(format!("ectomatter-{id}-{}.ectocore", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    let m = packages::manifest_in_file(&tmp).and_then(|inside| if inside == l.manifest { Ok(()) } else { Err("its package’s manifest isn’t the one Ectonomic reviewed".into()) }).and_then(|_| packages::install_file(&tmp, into));
    let _ = std::fs::remove_file(&tmp);
    let m = m?;
    // (where it came from, for its updates, and that it came from the list)
    let _ = std::fs::write(into.join(&id).join("source.json"), json!({ "repository": l.repository, "listed": true }).to_string());
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A list of one package, the Monitor Switcher at `version`, pinned to `sha`.
    fn list_of(version: &str, sha: &str, manifest: &Value) -> Value {
        let mut m = manifest.clone();
        m["version"] = json!(version);
        json!({ "format": 1, "packages": [{ "repository": "test/monitor-switcher", "publisher": "Someone", "versions": [{ "manifest": m, "sha256": sha }] }] })
    }

    /// The list of packages in this repository, as tools/release-package.py
    /// made it, is one Ectomatter reads: every package that comes with it, each
    /// by its own manifest.
    #[test]
    fn the_list_names_every_package_that_comes_with_ectomatter() {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../registry/packages.json")).unwrap()).unwrap();
        let listed = of(&v).unwrap();
        for dir in [crate::cores::dir(), crate::cores::plugins_dir()] {
            for e in std::fs::read_dir(dir).unwrap().flatten().filter(|e| e.path().join("core.json").is_file()) {
                let own = ectomatter_draw::jsonc::parse(&std::fs::read_to_string(e.path().join("core.json")).unwrap()).unwrap();
                let id = own["id"].as_str().unwrap().to_string();
                let l = listed.iter().find(|l| l.id() == id).unwrap_or_else(|| panic!("{id} isn't on the list"));
                assert_eq!(l.manifest, own, "the list's manifest of {id} isn't its own core.json (tools/release-package.py)");
            }
        }
    }

    /// Every package that comes with Ectomatter is added from the list on
    /// GitHub as a stranger's Ectomatter adds it: the list read from its
    /// address, the package downloaded from its release and taken as it was
    /// reviewed, put in a folder of its own, and started there, with no
    /// device, speaking this Ectomatter's protocol. The packages are named
    /// by their own manifests here.
    #[test]
    #[ignore = "needs the network: tools/boundaries.sh runs it"]
    fn every_package_is_added_from_its_release_on_github() {
        let _one = crate::updates::tests::GITHUB.lock().unwrap_or_else(|p| p.into_inner());
        // (the real list and the real releases, not a test's)
        unsafe {
            std::env::remove_var("ECTOMATTER_GITHUB");
            std::env::remove_var("ECTOMATTER_REGISTRY");
        }
        let list: Value = serde_json::from_slice(&crate::updates::get(&address()).expect("the list on GitHub")).unwrap();
        let listed = of(&list).unwrap();
        let into = std::env::temp_dir().join(format!("ectomatter-registry-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&into);
        let mut added = 0;
        for dir in [crate::cores::dir(), crate::cores::plugins_dir()] {
            for e in std::fs::read_dir(dir).unwrap().flatten().filter(|e| e.path().join("core.json").is_file()) {
                let own = ectomatter_draw::jsonc::parse(&std::fs::read_to_string(e.path().join("core.json")).unwrap()).unwrap();
                let id = own["id"].as_str().unwrap().to_string();
                let l = listed.iter().find(|l| l.id() == id).unwrap_or_else(|| panic!("{id} isn't on the list on GitHub"));
                let m = install(l, &into).unwrap_or_else(|e| panic!("{id} wasn't added from its release: {e}"));
                assert_eq!(m.id, id);
                let at = into.join(&id).join(l.version());
                let data = into.join(format!("{id}-data"));
                let core = crate::core_host::Core::start_within(&at, data, Box::new(crate::core_host::NoDevices), None, crate::core_host::Limits::default())
                    .unwrap_or_else(|e| panic!("{id} as added doesn't start: {e}"));
                crate::core_host::initialize_within(&core, Value::Null, Duration::from_secs(30)).unwrap_or_else(|e| panic!("{id} as added doesn't answer: {e}"));
                let _ = core.call_within("shutdown", &Value::Null, Duration::from_secs(10));
                added += 1;
            }
        }
        let _ = std::fs::remove_dir_all(&into);
        assert!(added >= 2, "only {added} packages found here");
    }

    /// A package from the list is taken only as it was reviewed: a package
    /// that doesn't match the checksum the list pins isn't, even with its
    /// release's own checksum right, nor one whose manifest isn't the list's.
    #[test]
    fn a_listed_package_is_taken_only_as_it_was_reviewed() {
        let _one = crate::updates::tests::GITHUB.lock().unwrap_or_else(|p| p.into_inner());
        let into = std::env::temp_dir().join(format!("ectomatter-registry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&into);
        let (manifest, package) = crate::updates::tests::package_of("1.0.3");
        let m = ectomatter_draw::jsonc::parse(std::str::from_utf8(&manifest).unwrap()).unwrap();
        let sum = crate::updates::sha256(&package);
        let at = crate::updates::tests::fake_github(vec![
            ("/test/monitor-switcher/releases/download/1.0.3/monitor-switcher.ectocore".into(), package.clone()),
            ("/test/monitor-switcher/releases/download/1.0.3/monitor-switcher.ectocore.sha256".into(), format!("{sum}  monitor-switcher.ectocore").into_bytes()),
        ]);
        unsafe { std::env::set_var("ECTOMATTER_GITHUB", &at) };
        // (pinned to another checksum: refused, though the release's own matches)
        let wrong = of(&list_of("1.0.3", &"0".repeat(64), &m)).unwrap();
        assert_eq!(install(&wrong[0], &into).map(|m| m.version), Err("its package isn’t the one Ectonomic reviewed: it doesn’t match the checksum on the list".into()));
        // (the list's manifest reaching less than the package's: refused)
        let mut less = m.clone();
        less["uses"] = json!([]);
        let other = of(&list_of("1.0.3", &sum, &less)).unwrap();
        assert_eq!(install(&other[0], &into).map(|m| m.version), Err("its package’s manifest isn’t the one Ectonomic reviewed".into()));
        assert!(!into.join("monitor-switcher").exists(), "nothing put in place");
        // (as reviewed: put in place, and kept as from the list)
        let right = of(&list_of("1.0.3", &sum, &m)).unwrap();
        assert_eq!(install(&right[0], &into).map(|m| m.version), Ok("1.0.3".into()));
        let source: Value = serde_json::from_slice(&std::fs::read(into.join("monitor-switcher/source.json")).unwrap()).unwrap();
        assert_eq!(source, json!({ "repository": "test/monitor-switcher", "listed": true }));
        let _ = std::fs::remove_dir_all(&into);
    }

    /// A package on the list is updated to the list's newest version, as
    /// reviewed, and one that can do more waits for the person.
    #[test]
    fn a_listed_package_updates_from_the_list() {
        let _one = crate::updates::tests::GITHUB.lock().unwrap_or_else(|p| p.into_inner());
        let p = packages::Package::read(&crate::cores::plugins_dir().join("monitor-switcher"), false).unwrap();
        let into = std::env::temp_dir().join(format!("ectomatter-registry-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&into);
        let (manifest, package) = crate::updates::tests::package_of("1.0.4");
        let m = ectomatter_draw::jsonc::parse(std::str::from_utf8(&manifest).unwrap()).unwrap();
        let at = crate::updates::tests::fake_github(vec![("/test/monitor-switcher/releases/download/1.0.4/monitor-switcher.ectocore".into(), package.clone())]);
        unsafe { std::env::set_var("ECTOMATTER_GITHUB", &at) };
        let listed = of(&list_of("1.0.4", &crate::updates::sha256(&package), &m)).unwrap();
        assert_eq!(crate::updates::on_list(&p, &listed).map(|l| l.version()), Some("1.0.4"));
        assert_eq!(crate::updates::check_listed(&p, &listed[0], &into), crate::updates::Status::Updated("1.0.4".into()));
        // (reaching the web as well: waits)
        let mut more = m.clone();
        more["version"] = json!("1.0.5");
        more["uses"] = json!(["monitors", { "web": "example.com", "to": "Example" }]);
        let listed = of(&list_of("1.0.5", &"0".repeat(64), &more)).unwrap();
        assert!(matches!(crate::updates::check_listed(&p, &listed[0], &into), crate::updates::Status::Asks(v, _) if v == "1.0.5"));
        let _ = std::fs::remove_dir_all(&into);
    }

    /// Of an entry's versions, the newest that runs here is offered.
    #[test]
    fn the_newest_version_that_runs_here_is_offered() {
        let dir = crate::cores::plugins_dir().join("monitor-switcher");
        let m = ectomatter_draw::jsonc::parse(&std::fs::read_to_string(dir.join("core.json")).unwrap()).unwrap();
        let mut newer = m.clone();
        newer["version"] = json!("9.0.0");
        newer["protocol"] = json!(ectomatter_protocol::PROTOCOL + 1);
        let mut v = list_of("1.0.0", &"a".repeat(64), &m);
        v["packages"][0]["versions"].as_array_mut().unwrap().push(json!({ "manifest": newer, "sha256": "b".repeat(64) }));
        let listed = of(&v).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].version(), "1.0.0", "9.0.0 speaks a newer protocol");
        assert!(of(&json!({ "format": 2, "packages": [] })).is_err());
    }
}
