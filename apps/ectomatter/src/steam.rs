//! Steam's games on this computer, read from Steam's own files whether Steam
//! is running or not: where Steam is (steamlocate finds it on Linux, Windows
//! and macOS), each app installed in its libraries, which of them are games
//! and which apps (appinfo.vdf, Steam's record of every app, says; its tools,
//! such as Proton and its runtimes, aren't shown), how long and when each was
//! last played and which the person starred (their own files, read for those
//! values and nothing else: localconfig.vdf also holds keys that must never
//! leave it), and the art Steam keeps for each. Also: the Steam games running
//! now, and the one a process belongs to.

use std::path::{Path, PathBuf};

use steam_vdf_parser::{Obj, Value};

/// What a Steam app is in the Library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Game,
    App,
}

/// The pictures Steam keeps for an app: its tall cover (2:3), the wide
/// picture a page shows at its top, its logo, and its small square icon.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Art {
    pub cover: Option<PathBuf>,
    pub hero: Option<PathBuf>,
    pub logo: Option<PathBuf>,
    pub icon: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct App {
    pub id: u32,
    /// as Steam names it, without its trademark signs
    pub name: String,
    pub kind: Kind,
    /// minutes played in all, and when last played (seconds since 1970; 0, never)
    pub minutes: u64,
    pub last: u64,
    /// the library it's installed in
    pub library: PathBuf,
    pub art: Art,
    /// whether Steam starts its Linux version: it has one (Steam's record lists
    /// Linux among its systems), and Steam Play's own manifests don't send it
    /// to Proton instead
    pub linux: bool,
    /// the Steam Linux Runtime Steam Play's manifests run its Linux version in
    /// (as "SteamLinuxRuntime_sniper"), if they name one
    pub runtime: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Steam {
    pub root: PathBuf,
    pub apps: Vec<App>,
    /// the apps the person starred in Steam, by number
    pub favorites: Vec<u32>,
}

/// Steam's games and apps, or None where Steam isn't on this computer. An
/// offscreen check or a test can be given a Steam of its own to read
/// (ECTOMATTER_STEAM, a folder laid out as Steam's is).
pub fn read() -> Option<Steam> {
    let dir = match std::env::var_os("ECTOMATTER_STEAM") {
        Some(p) => steamlocate::SteamDir::from_dir(Path::new(&p)).ok()?,
        None => steamlocate::SteamDir::locate().ok()?,
    };
    Some(read_dir(&dir))
}

fn read_dir(dir: &steamlocate::SteamDir) -> Steam {
    let root = dir.path().to_path_buf();
    let user = user(&root);
    let info = std::fs::read(root.join("appcache/appinfo.vdf")).ok();
    let info = info.as_deref().and_then(|b| steam_vdf_parser::parse_appinfo(b).ok());
    // (Steam Play's manifests, app 891390: the tool Valve has each app start with)
    let mappings = info.as_ref().and_then(|v| v.get("891390")).map(|e| e.get("appinfo").unwrap_or(e)).and_then(|e| e.get("extended")).and_then(|x| x.get("app_mappings"));
    let played = user.map(|u| played(&root, u)).unwrap_or_default();
    let mut apps: Vec<App> = Vec::new();
    // (a library that can't be read, such as a drive not plugged in, is passed over)
    for lib in dir.libraries().into_iter().flatten().flatten() {
        for app in lib.apps().flatten() {
            if apps.iter().any(|a| a.id == app.app_id) || !installed(&app) {
                continue;
            }
            let art = art(&root, user, app.app_id);
            let entry = info.as_ref().and_then(|v| v.get(&app.app_id.to_string())).map(|e| e.get("appinfo").unwrap_or(e));
            let kind = match entry.and_then(|e| e.as_obj()) {
                Some(e) => kind_of(e),
                // (without Steam's record, an app with a cover in Steam's library is one to show)
                None => art.cover.is_some().then_some(Kind::Game),
            };
            let Some(kind) = kind else { continue };
            let name = app.name.clone().or_else(|| entry.and_then(|e| e.get_str(&["common", "name"])).map(String::from)).unwrap_or_else(|| format!("App {}", app.app_id));
            let (minutes, last) = played.iter().find(|(id, _, _)| *id == app.app_id).map_or((0, 0), |(_, m, l)| (*m, *l));
            let valve = mappings.and_then(|m| m.get(&app.app_id.to_string())).and_then(|m| m.get_str(&["tool"])).map(String::from);
            let (linux, runtime) = start_of(entry.and_then(|e| e.as_obj()).is_some_and(on_linux), valve);
            apps.push(App { id: app.app_id, name: clean(&name), kind, minutes, last, library: lib.path().to_path_buf(), art, linux, runtime });
        }
    }
    let favorites = user.map(|u| favorites(&root, u)).unwrap_or_default();
    Steam { root, apps, favorites }
}

/// Whether an app's files are all there: one being downloaded for the first
/// time isn't yet a game to play.
fn installed(app: &steamlocate::App) -> bool {
    app.state_flags.is_none_or(|f| f.flags().any(|s| s == steamlocate::app::StateFlag::FullyInstalled))
}

/// What Steam's record says an app is: a game, an app (one Steam calls a tool
/// counts when Steam gives it a cover in its library, as SteamVR has; its
/// other tools are Proton, its runtimes and the like), or neither.
fn kind_of(entry: &Obj) -> Option<Kind> {
    let common = entry.get("common")?.as_obj()?;
    match common.get("type")?.as_str()?.to_ascii_lowercase().as_str() {
        "game" | "demo" => Some(Kind::Game),
        "application" => Some(Kind::App),
        "tool" => common.get("library_assets").and_then(|a| a.as_obj()).filter(|a| a.contains_key("library_capsule")).map(|_| Kind::App),
        _ => None,
    }
}

/// Whether Steam's record lists Linux among the systems an app runs on
/// (common/oslist, as "windows,linux").
fn on_linux(entry: &Obj) -> bool {
    entry.get("common").and_then(|c| c.as_obj()).and_then(|c| c.get("oslist")).and_then(|o| o.as_str()).is_some_and(|l| l.split(',').any(|s| s.trim().eq_ignore_ascii_case("linux")))
}

/// How Steam starts an app that may have a Linux version, by Steam Play's
/// own tool for it: its Windows version where the tool is a Proton build,
/// else its Linux version, in the Steam Linux Runtime the tool names, if one.
fn start_of(linux: bool, valve: Option<String>) -> (bool, Option<String>) {
    match valve {
        Some(t) if t.to_ascii_lowercase().contains("proton") => (false, None),
        Some(t) if t.to_ascii_lowercase().starts_with("steamlinuxruntime") => (linux, linux.then_some(t)),
        _ => (linux, None),
    }
}

/// A name as the Library shows it: Steam's names carry trademark signs.
pub fn clean(name: &str) -> String {
    let kept: String = name.chars().filter(|c| !['\u{2122}', '\u{00ae}', '\u{00a9}'].contains(c)).collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A key of an object, matched without regard to case, as Steam's files spell
/// the same key both ways.
fn get_any<'a, 't>(o: &'a Obj<'t>, key: &str) -> Option<&'a Value<'t>> {
    o.get(key).or_else(|| o.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v))
}

/// The person signed in to Steam last, by their account's number (the name of
/// their folder in userdata): the one marked most recent, or the newest sign-in.
fn user(root: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(root.join("config/loginusers.vdf")).ok()?;
    let v = steam_vdf_parser::parse_text(&text).ok()?;
    let mut best: Option<((bool, u64), u64)> = None;
    for (id, u) in v.as_obj()?.iter() {
        let (Ok(id), Some(u)) = (id.parse::<u64>(), u.as_obj()) else { continue };
        let recent = get_any(u, "MostRecent").and_then(|m| m.as_str()) == Some("1");
        let when = get_any(u, "Timestamp").and_then(|m| m.as_str()).and_then(|t| t.parse::<u64>().ok()).unwrap_or(0);
        if best.is_none_or(|(b, _)| (recent, when) > b) {
            best = Some(((recent, when), id));
        }
    }
    // (a Steam id less the base every individual account's starts from)
    best.and_then(|(_, id)| id.checked_sub(76_561_197_960_265_728)).and_then(|a| u32::try_from(a).ok())
}

/// Each app's minutes played and when it was last played, from the person's
/// own settings; nothing else in the file is kept.
fn played(root: &Path, user: u32) -> Vec<(u32, u64, u64)> {
    let Ok(text) = std::fs::read_to_string(root.join(format!("userdata/{user}/config/localconfig.vdf"))) else { return Vec::new() };
    played_in(&text)
}

pub(crate) fn played_in(text: &str) -> Vec<(u32, u64, u64)> {
    let Ok(v) = steam_vdf_parser::parse_text(text) else { return Vec::new() };
    let mut o = v.as_obj();
    for key in ["Software", "Valve", "Steam", "apps"] {
        o = o.and_then(|o| get_any(o, key)).and_then(|x| x.as_obj());
    }
    let Some(apps) = o else { return Vec::new() };
    let num = |a: &Obj, k: &str| get_any(a, k).and_then(|x| x.as_str()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    apps.iter()
        .filter_map(|(id, a)| {
            let a = a.as_obj()?;
            Some((id.parse().ok()?, num(a, "Playtime"), num(a, "LastPlayed")))
        })
        .collect()
}

/// The apps the person starred in Steam, from the collections Steam keeps for them.
fn favorites(root: &Path, user: u32) -> Vec<u32> {
    let Ok(text) = std::fs::read_to_string(root.join(format!("userdata/{user}/config/cloudstorage/cloud-storage-namespace-1.json"))) else { return Vec::new() };
    favorites_in(&text)
}

pub(crate) fn favorites_in(text: &str) -> Vec<u32> {
    let Ok(serde_json::Value::Array(pairs)) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    let Some(val) = pairs.iter().find(|p| p[0] == "user-collections.favorite").and_then(|p| p[1]["value"].as_str()) else { return Vec::new() };
    let Ok(c) = serde_json::from_str::<serde_json::Value>(val) else { return Vec::new() };
    let ids = |k: &str| -> Vec<u32> { c[k].as_array().into_iter().flatten().filter_map(|x| x.as_u64().and_then(|n| u32::try_from(n).ok())).collect() };
    let removed = ids("removed");
    ids("added").into_iter().filter(|id| !removed.contains(id)).collect()
}

/// The art Steam keeps for an app: a picture the person chose in Steam first,
/// then Steam's own (kept by newer clients in a folder of its own per picture,
/// by older ones at the top of the app's folder or beside it).
fn art(root: &Path, user: Option<u32>, id: u32) -> Art {
    let grid = user.map(|u| root.join(format!("userdata/{u}/config/grid")));
    let chosen = |suffix: &str| grid.as_ref().and_then(|g| ["png", "jpg", "jpeg"].iter().map(|e| g.join(format!("{id}{suffix}.{e}"))).find(|p| p.is_file()));
    let cache = root.join("appcache/librarycache");
    let own = cache.join(id.to_string());
    let folders: Vec<PathBuf> = std::fs::read_dir(&own).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    let find = |names: &[&str]| -> Option<PathBuf> {
        names.iter().find_map(|n| {
            let at_top = own.join(n);
            let beside = cache.join(format!("{id}_{n}"));
            [at_top, beside].into_iter().find(|p| p.is_file()).or_else(|| folders.iter().map(|f| f.join(n)).find(|p| p.is_file()))
        })
    };
    // (the icon is named for its own contents: forty hexadecimal digits)
    let icon = std::fs::read_dir(&own).into_iter().flatten().flatten().map(|e| e.path()).find(|p| {
        p.extension().is_some_and(|e| e == "jpg") && p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()))
    });
    Art {
        cover: chosen("p").or_else(|| find(&["library_600x900_2x.jpg", "library_600x900.jpg", "library_capsule_2x.jpg", "library_capsule.jpg"])),
        hero: chosen("_hero").or_else(|| find(&["library_hero.jpg"])),
        logo: chosen("_logo").or_else(|| find(&["logo.png"])),
        icon,
    }
}

/// The Steam games running now: each one's number and the process Steam
/// started it under (its "reaper", which everything the game runs comes from).
#[cfg(target_os = "linux")]
pub fn running() -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        // (only a process by that name is read further)
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        if comm.trim_end() != "reaper" {
            continue;
        }
        if let Some(id) = std::fs::read(format!("/proc/{pid}/cmdline")).ok().and_then(|c| app_of_reaper(&c)) {
            out.push((id, pid));
        }
    }
    out.sort_unstable();
    out
}

/// (Steam's games are seen starting on Linux so far.)
#[cfg(not(target_os = "linux"))]
pub fn running() -> Vec<(u32, u32)> {
    Vec::new()
}

/// A reaper's game, by its command line: Steam starts each game as
/// `reaper SteamLaunch AppId=N -- ...`.
pub fn app_of_reaper(cmdline: &[u8]) -> Option<u32> {
    let args: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
    let first = std::str::from_utf8(args.first()?).ok()?;
    if Path::new(first).file_name()? != "reaper" || !args.contains(&b"SteamLaunch".as_slice()) {
        return None;
    }
    args.iter().find_map(|a| std::str::from_utf8(a).ok()?.strip_prefix("AppId=")?.parse().ok())
}

/// A process's parent, from its stat line: the fourth field, after its name
/// in parentheses (which may hold spaces and parentheses of its own).
pub fn parent_in(stat: &str) -> Option<u32> {
    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_lose_their_trademark_signs() {
        assert_eq!(clean("DEEPDRIVER™ 2"), "DEEPDRIVER 2");
        assert_eq!(clean("LEGO® Star Wars™: The Skywalker Saga"), "LEGO Star Wars: The Skywalker Saga");
        assert_eq!(clean("Horizon Forbidden West™ Complete Edition"), "Horizon Forbidden West Complete Edition");
    }

    #[test]
    fn steams_record_says_what_an_app_is() {
        let kind = |text: &str| {
            let v = steam_vdf_parser::parse_text(text).unwrap();
            kind_of(v.as_obj().unwrap())
        };
        assert_eq!(kind(r#""appinfo" { "common" { "type" "Game" } }"#), Some(Kind::Game));
        // (Steam spells it both ways)
        assert_eq!(kind(r#""appinfo" { "common" { "type" "game" } }"#), Some(Kind::Game));
        assert_eq!(kind(r#""appinfo" { "common" { "type" "Application" } }"#), Some(Kind::App));
        // SteamVR is a tool with a cover in Steam's library; Proton and the redistributables aren't
        assert_eq!(kind(r#""appinfo" { "common" { "type" "Tool" "library_assets" { "library_capsule" "en" "library_hero" "en" } } }"#), Some(Kind::App));
        assert_eq!(kind(r#""appinfo" { "common" { "type" "Tool" "library_assets" { "library_hero" "en" } } }"#), None);
        assert_eq!(kind(r#""appinfo" { "common" { "type" "Tool" } }"#), None);
        assert_eq!(kind(r#""appinfo" { "common" { "type" "DLC" } }"#), None);
        let linux = |text: &str| on_linux(steam_vdf_parser::parse_text(text).unwrap().as_obj().unwrap());
        assert!(linux(r#""appinfo" { "common" { "oslist" "windows,macos,linux" } }"#));
        assert!(!linux(r#""appinfo" { "common" { "oslist" "windows" } }"#));
        assert!(!linux(r#""appinfo" { "common" { "type" "Game" } }"#));
        // Steam Play's own tool for it: Counter-Strike 2 in the sniper runtime, Hollow Knight through Proton
        assert_eq!(start_of(true, Some("SteamLinuxRuntime_sniper".into())), (true, Some("SteamLinuxRuntime_sniper".into())));
        assert_eq!(start_of(true, Some("proton-stable".into())), (false, None));
        assert_eq!(start_of(true, None), (true, None));
        assert_eq!(start_of(true, Some("steamrt4-any".into())), (true, None), "only a Proton build sends it to its Windows version");
    }

    #[test]
    fn only_playtime_and_last_played_are_read() {
        let text = r#""UserLocalConfigStore" { "Software" { "Valve" { "Steam" { "apps" {
            "1808500" { "LastPlayed" "1787489550" "Playtime" "21842" "cloud" { "last_sync_state" "synchronized" } }
            "418530" { "Playtime" "2307" }
        } } } } "streaming_v2" { "EnableStreaming" "0" } }"#;
        let mut got = played_in(text);
        got.sort();
        assert_eq!(got, vec![(418530, 2307, 0), (1808500, 21842, 1787489550)]);
        // (lower-case keys, as some clients write them)
        assert_eq!(played_in(r#""UserLocalConfigStore" { "software" { "valve" { "steam" { "Apps" { "7" { "Playtime" "5" } } } } } }"#), vec![(7, 5, 0)]);
    }

    #[test]
    fn favorites_are_the_starred_less_the_unstarred() {
        let value = serde_json::json!({ "id": "favorite", "name": "Favorites", "added": [250820, 1808500, 553850], "removed": [553850] }).to_string();
        let text = serde_json::json!([["user-collections.hidden", { "key": "user-collections.hidden", "value": "{}" }], ["user-collections.favorite", { "key": "user-collections.favorite", "value": value }]]).to_string();
        assert_eq!(favorites_in(&text), vec![250820, 1808500]);
        assert_eq!(favorites_in("not json"), Vec::<u32>::new());
    }

    #[test]
    fn a_reaper_names_its_game() {
        let cmd = b"/home/u/.local/share/Steam/ubuntu12_32/reaper\0SteamLaunch\0AppId=1808500\0--\0/x/proton\0waitforexitandrun\0game.exe\0";
        assert_eq!(app_of_reaper(cmd), Some(1808500));
        // (another program with the same words isn't Steam's)
        assert_eq!(app_of_reaper(b"/usr/bin/bash\0SteamLaunch\0AppId=5\0"), None);
        assert_eq!(app_of_reaper(b"/x/reaper\0AppId=5\0"), None);
    }

    /// This computer's own Steam, read as the Library reads it (run by hand:
    /// it says only how many there are of each and which are favorites).
    #[test]
    #[ignore]
    fn reads_this_computers_steam() {
        let t = std::time::Instant::now();
        let s = read().expect("no Steam on this computer");
        let n = |k: Kind| s.apps.iter().filter(|a| a.kind == k).count();
        eprintln!("read in {:?}: {} games, {} apps, {} favorites", t.elapsed(), n(Kind::Game), n(Kind::App), s.favorites.len());
        let apps: Vec<&str> = s.apps.iter().filter(|a| a.kind == Kind::App).map(|a| a.name.as_str()).collect();
        eprintln!("apps: {apps:?}");
        let no_cover: Vec<&str> = s.apps.iter().filter(|a| a.art.cover.is_none()).map(|a| a.name.as_str()).collect();
        eprintln!("without a cover: {no_cover:?}");
        let never: usize = s.apps.iter().filter(|a| a.kind == Kind::Game && a.minutes == 0).count();
        eprintln!("games never played: {never}");
    }

    #[test]
    fn a_process_names_its_parent() {
        assert_eq!(parent_in("4142 (kwin_wayland) S 4100 4142 4142 0 -1"), Some(4100));
        // (a name with spaces and parentheses of its own)
        assert_eq!(parent_in("77 (Web Content (x)) S 12 77 77 0 -1"), Some(12));
    }
}
