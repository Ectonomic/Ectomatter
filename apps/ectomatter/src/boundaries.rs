//! The boundaries Ectomatter claims, each tried. A boundary that doesn't hold
//! yet stays here as a known failure (`#[ignore = "known failure: ..."]`), with
//! its item in docs/deferred.md, and turns into an ordinary test the day the
//! code holds. `tools/boundaries.sh` runs every one and says which hold.
//!
//! These read the source, as the design system's checks do (tokens.rs): each
//! says in code what the docs say in words, so a doc that stops being true
//! fails here. The boundaries a check of the source can't prove (a core of its
//! own reaching its device, a plugin that hangs) are tried by running them,
//! beside the code they try.

#[cfg(test)]
pub(crate) mod tests {
    use std::path::{Path, PathBuf};

    pub(crate) fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    pub(crate) fn read(rel: &str) -> String {
        std::fs::read_to_string(repo().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }

    /// A Rust file's code, each line with its number: its comments out, and
    /// every item under `#[cfg(test)]` left out (found by reading the file
    /// as Rust).
    pub(crate) fn code(rel: &str) -> Vec<(usize, String)> {
        let text = read(rel);
        let file = syn::parse_file(&text).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let mut tests = Vec::new();
        tested(&file.items, &mut tests);
        let mut out = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let n = i + 1;
            if tests.iter().any(|(a, b)| (*a..=*b).contains(&n)) {
                continue;
            }
            let line = line.split("//").next().unwrap_or("");
            if !line.trim().is_empty() {
                out.push((n, line.to_string()));
            }
        }
        out
    }

    fn is_test(attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|a| a.path().is_ident("cfg") && a.parse_args::<syn::Ident>().is_ok_and(|i| i == "test"))
    }

    fn lines_of(s: proc_macro2::Span) -> (usize, usize) {
        (s.start().line, s.end().line)
    }

    /// The lines of each item under `#[cfg(test)]`, in modules and impls too.
    fn tested(items: &[syn::Item], out: &mut Vec<(usize, usize)>) {
        use syn::spanned::Spanned;
        for item in items {
            let attrs: &[syn::Attribute] = match item {
                syn::Item::Fn(f) => &f.attrs,
                syn::Item::Mod(m) => &m.attrs,
                syn::Item::Impl(i) => &i.attrs,
                syn::Item::Struct(s) => &s.attrs,
                syn::Item::Enum(e) => &e.attrs,
                syn::Item::Const(c) => &c.attrs,
                syn::Item::Static(s) => &s.attrs,
                syn::Item::Use(u) => &u.attrs,
                syn::Item::Trait(t) => &t.attrs,
                syn::Item::Type(t) => &t.attrs,
                syn::Item::Macro(m) => &m.attrs,
                _ => &[],
            };
            if is_test(attrs) {
                out.push(lines_of(item.span()));
            } else if let syn::Item::Mod(m) = item
                && let Some((_, inner)) = &m.content
            {
                tested(inner, out);
            } else if let syn::Item::Impl(i) = item {
                for ii in &i.items {
                    if let syn::ImplItem::Fn(f) = ii
                        && is_test(&f.attrs)
                    {
                        out.push(lines_of(f.span()));
                    }
                }
            }
        }
    }

    /// The app's own Rust: the window's, the launcher's, and the crates it
    /// builds in that are no package's (the cores' and plugins' shared
    /// crates, crates/ectomatter-sensors and crates/ectomatter-weather, and
    /// the Z10's driver are theirs).
    pub(crate) fn rust_files() -> Vec<String> {
        let mut out = Vec::new();
        let mut dirs: Vec<PathBuf> = ["apps/ectomatter/src", "apps/ectomatter-launch/src", "crates/ectomatter-draw/src", "crates/ectomatter-tokens/src", "crates/ectomatter-protocol/src"].iter().map(|d| repo().join(d)).collect();
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    dirs.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p.strip_prefix(repo()).unwrap().to_string_lossy().into_owned());
                }
            }
        }
        out.sort();
        out
    }

    /// The window's Slint files, each line with its number, comments out.
    pub(crate) fn ui_code() -> Vec<(String, usize, String)> {
        let mut out = Vec::new();
        let mut dirs = vec![repo().join("apps/ectomatter/ui")];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    dirs.push(p);
                } else if p.extension().is_some_and(|x| x == "slint") {
                    let rel = p.strip_prefix(repo()).unwrap().to_string_lossy().into_owned();
                    for (i, line) in std::fs::read_to_string(&p).unwrap().lines().enumerate() {
                        let line = line.split("//").next().unwrap_or("");
                        if !line.trim().is_empty() {
                            out.push((rel.clone(), i + 1, line.to_string()));
                        }
                    }
                }
            }
        }
        out.sort();
        out
    }

    pub(crate) fn manifests() -> Vec<(String, serde_json::Value)> {
        let mut out = Vec::new();
        for folder in ["cores", "plugins"] {
            for e in std::fs::read_dir(repo().join(folder)).unwrap().flatten() {
                let m = e.path().join("core.json");
                if m.exists() {
                    let text = std::fs::read_to_string(&m).unwrap();
                    let v = ectomatter_draw::jsonc::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", m.display()));
                    out.push((format!("{folder}/{}", e.file_name().to_string_lossy()), v));
                }
            }
        }
        out
    }

    /// The protocol's own words, as its doc writes them as code: each
    /// inline `code` span whole (its quotes off), and each word of its code
    /// blocks; never its prose. A package's word that is one of these is the
    /// protocol's, which the app may name.
    pub(crate) fn protocol_words() -> std::collections::BTreeSet<String> {
        let doc = read("docs/core-protocol.md");
        let mut out = std::collections::BTreeSet::new();
        for (i, part) in doc.split("```").enumerate() {
            if i % 2 == 1 {
                out.extend(part.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/')).filter(|w| !w.is_empty()).map(String::from));
            } else {
                out.extend(part.split('`').skip(1).step_by(2).map(|s| s.trim_matches('"').to_string()));
            }
        }
        out
    }

    /// The lines of the app's code and its window's that name one of these
    /// words in quotes, each as file:line word.
    pub(crate) fn quoted_in_app(words: &[&String]) -> Vec<String> {
        let mut found = Vec::new();
        for f in rust_files() {
            if f.ends_with("boundaries.rs") {
                continue;
            }
            for (n, line) in code(&f) {
                if let Some(w) = words.iter().find(|w| line.contains(&format!("\"{w}\""))) {
                    found.push(format!("{f}:{n} {w}"));
                }
            }
        }
        for (f, n, line) in ui_code() {
            if let Some(w) = words.iter().find(|w| line.contains(&format!("\"{w}\""))) {
                found.push(format!("{f}:{n} {w}"));
            }
        }
        found
    }

    pub(crate) fn fail_on(found: Vec<String>, what: &str) {
        assert!(found.is_empty(), "{what}:\n  {}", found.join("\n  "));
    }

    /// docs/core-protocol.md: "the frontend has one screen per capability, so
    /// every device that offers a capability is edited the same way." A device
    /// is only its core, so the app builds in no device's driver.
    #[test]
    fn the_app_links_no_devices_driver() {
        let toml = read("apps/ectomatter/Cargo.toml");
        let deps = toml.split("[dependencies]").nth(1).unwrap_or("").split("\n[").next().unwrap_or("");
        let found: Vec<String> = deps.lines().filter(|l| l.trim_start().starts_with("evga-z10")).map(|l| l.trim().to_string()).collect();
        fail_on(found, "apps/ectomatter/Cargo.toml depends on a device's driver");
    }

    /// The tests see what the app does: a dependency the workspace names is
    /// taken from it by every crate, so its features are the same however
    /// the app is built (a crate asking for its own features changes them for
    /// every crate built with it, and `cargo test` builds more crates than
    /// the app). The members and the dependencies come from the workspace's
    /// Cargo.toml, read as TOML: every kind of dependency table, a system's
    /// own (`[target.'cfg(..)'.dependencies]`), and one renamed (`package =`).
    #[test]
    fn every_crate_takes_the_workspaces_dependencies() {
        let toml = |rel: &str| -> toml::Table { read(rel).parse().unwrap_or_else(|e| panic!("{rel}: {e}")) };
        let ws = toml("Cargo.toml");
        let shared: Vec<String> = ws["workspace"]["dependencies"].as_table().map(|t| t.keys().cloned().collect()).unwrap_or_default();
        assert!(!shared.is_empty(), "the workspace names no dependencies");
        let members: Vec<String> = ws["workspace"]["members"].as_array().into_iter().flatten().filter_map(|m| m.as_str().map(String::from)).collect();
        assert!(members.len() > 1, "the workspace names no members");
        let mut found = Vec::new();
        for m in members {
            let f = format!("{m}/Cargo.toml");
            let t = toml(&f);
            // every dependency table: the crate's own and each system's
            let mut tables: Vec<(String, &toml::Table)> = Vec::new();
            for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(d) = t.get(kind).and_then(|d| d.as_table()) {
                    tables.push((kind.to_string(), d));
                }
                for (target, v) in t.get("target").and_then(|x| x.as_table()).into_iter().flatten() {
                    if let Some(d) = v.get(kind).and_then(|d| d.as_table()) {
                        tables.push((format!("target.{target}.{kind}"), d));
                    }
                }
            }
            for (kind, deps) in tables {
                for (name, spec) in deps {
                    // (the crate it is, where a dependency is renamed)
                    let crate_name = spec.get("package").and_then(|p| p.as_str()).unwrap_or(name);
                    if !shared.iter().any(|s| s == crate_name) {
                        continue;
                    }
                    if spec.get("workspace").and_then(|w| w.as_bool()) != Some(true) {
                        found.push(format!("{f} [{kind}] {name} = {spec}"));
                    }
                }
            }
        }
        fail_on(found, "a crate asks for its own version of a dependency the workspace names");
    }

    /// A snapshot reads nothing of the computer it runs on (tests/renders.rs):
    /// every path of this computer's that Ectomatter reads itself, under /proc
    /// or /sys, goes through `computer::here`, which a snapshot points at the
    /// computer described for it. (Its own process's files, /proc/self, are
    /// its own.)
    #[test]
    fn the_app_reads_this_computer_in_one_place() {
        let mut found = Vec::new();
        let mut seen = 0;
        for f in rust_files().into_iter().filter(|f| f.starts_with("apps/ectomatter/src/")) {
            for (n, line) in code(&f) {
                if !(line.contains("\"/proc") || line.contains("\"/sys")) {
                    continue;
                }
                seen += 1;
                if !line.contains("computer::here(") && !line.contains("\"/proc/self") {
                    found.push(format!("{f}:{n} {}", line.trim()));
                }
            }
        }
        assert!(seen > 0, "no path of the computer's read anywhere: the check reads nothing");
        fail_on(found, "a path of this computer's read past computer::here");
    }

    /// docs/core-protocol.md: "has no page of its own for any device". No page
    /// is found by a device's name, and nothing is kept under one.
    #[test]
    fn no_page_is_found_by_a_devices_name() {
        // (a page's kind, "keyboard" or "mouse", is the protocol's word for a
        // sort of device, not a device's name)
        let names = ["\"z10\"", ".z10.", ".z10\"", "\"sensei310\"", ".sensei310", "\"sensei310-1\"", "\"evga-z10\"", "\"steelseries-sensei-310\"", "EVGA", "Z10", "Sensei", "SteelSeries"];
        let mut found = Vec::new();
        for f in rust_files() {
            // a saved name from before is read once and moved on (old_name.rs)
            if f.ends_with("old_name.rs") || f.ends_with("boundaries.rs") {
                continue;
            }
            for (n, line) in code(&f) {
                if let Some(name) = names.iter().find(|x| line.contains(*x)) {
                    found.push(format!("{f}:{n} {name}"));
                }
            }
        }
        for (f, n, line) in ui_code() {
            if let Some(name) = names.iter().find(|x| line.contains(*x)) {
                found.push(format!("{f}:{n} {name}"));
            }
        }
        fail_on(found, "a device named in the app");
    }

    /// docs/core-protocol.md, section 3: every field a manifest gives is one
    /// the protocol names (ectomatter_protocol::manifest): every package's
    /// that comes with Ectomatter and every one the tests make up.
    #[test]
    fn every_packages_manifest_is_the_protocols() {
        let mut found = Vec::new();
        let mut read = 0;
        let mut dirs = vec![repo().join("cores"), repo().join("plugins"), repo().join("apps/ectomatter/tests/packages")];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                let m = p.join("core.json");
                if m.is_file() {
                    read += 1;
                    let v = ectomatter_draw::jsonc::parse(&std::fs::read_to_string(&m).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", m.display()));
                    let at = m.strip_prefix(repo()).unwrap().display().to_string();
                    if let Err(e) = ectomatter_protocol::manifest::Manifest::of(&v) {
                        found.push(format!("{at}: {e}"));
                    }
                    // (a newer protocol's field is passed over in the app; a package here
                    // speaks this one, so one is a misspelling)
                    for u in ectomatter_protocol::manifest::Manifest::unknown(&v) {
                        found.push(format!("{at}: {u}, which the protocol doesn’t name"));
                    }
                } else if p.is_dir() && !p.ends_with("module") && !p.ends_with("target") {
                    dirs.push(p);
                }
            }
        }
        assert!(read > 2, "no package's manifest was read");
        fail_on(found, "a manifest that isn't the protocol's");
    }

    /// docs/core-protocol.md, section 4: every field a device's description
    /// gives is one the protocol names (ectomatter_protocol::device): each
    /// core's description file here, and the description a core told as it
    /// met its device (the render tests' mouse, recorded from its core).
    #[test]
    fn every_devices_description_is_the_protocols() {
        let mut found = Vec::new();
        let mut seen = 0;
        let mut check = |at: String, v: &serde_json::Value| {
            seen += 1;
            if let Err(e) = ectomatter_protocol::device::Description::of(v) {
                found.push(format!("{at}: {e}"));
            }
            for u in ectomatter_protocol::device::Description::unknown(v) {
                found.push(format!("{at}: {u}, which the protocol doesn’t name"));
            }
        };
        for (name, m) in manifests().into_iter().filter(|(n, _)| n.starts_with("cores/")) {
            if let Some(file) = m["deviceFile"].as_str() {
                let at = format!("{name}/{file}");
                check(at.clone(), &ectomatter_draw::jsonc::parse(&read(&at)).unwrap_or_else(|e| panic!("{at}: {e}")));
            }
        }
        let mouse: serde_json::Value = serde_json::from_str(&read("apps/ectomatter/tests/renders/desk/mouse.json")).unwrap();
        check("apps/ectomatter/tests/renders/desk/mouse.json (device)".into(), &mouse["device"]);
        assert!(seen > 1, "no description read");
        fail_on(found, "a description that isn't the protocol's");
    }

    /// The protocol crate's lists of what a manifest and a description may
    /// give are the doc's: each is a name the doc writes as code.
    #[test]
    fn the_protocols_field_lists_are_the_docs() {
        use ectomatter_protocol::{device, manifest};
        let words = protocol_words();
        let lists: [(&str, &[&str]); 7] = [("manifest", manifest::FIELDS), ("usb", manifest::USB_FIELDS), ("systems", manifest::SYSTEMS), ("description", device::FIELDS), ("firmware", device::FIRMWARE_FIELDS), ("screen", device::SCREEN_FIELDS), ("capabilities", device::CAPABILITIES)];
        let mut found = Vec::new();
        for (list, names) in lists {
            for n in names {
                if !words.contains(*n) {
                    found.push(format!("{list}: {n}"));
                }
            }
        }
        fail_on(found, "a field the protocol's crate names that docs/core-protocol.md doesn't");
    }

    /// docs/core-protocol.md, section 9: "A frontend supports a range of
    /// versions and says which it speaks in `initialize`." Every package and
    /// every example in the doc speaks the protocol the app speaks.
    #[test]
    fn every_package_speaks_the_apps_protocol() {
        let speaks = ectomatter_protocol::PROTOCOL;
        let mut found = Vec::new();
        for (name, m) in manifests() {
            if m["protocol"].as_u64() != Some(speaks as u64) {
                found.push(format!("{name}/core.json: protocol {}", m["protocol"]));
            }
        }
        let doc = read("docs/core-protocol.md");
        for (i, line) in doc.lines().enumerate() {
            for key in ["\"protocol\": ", "\"protocol\":"] {
                if let Some(at) = line.find(key) {
                    let n: String = line[at + key.len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
                    if !n.is_empty() && n != speaks.to_string() {
                        found.push(format!("docs/core-protocol.md:{}: protocol {n}", i + 1));
                    }
                    break;
                }
            }
        }
        fail_on(found, &format!("not protocol {speaks}"));
    }

    /// docs/core-protocol.md, section 11: plugins are on equal footing; none
    /// is reached or named by the app, so a plugin of anyone's does what the
    /// ones Ectomatter comes with do. Each plugin's id and name, from its
    /// manifest, found anywhere in the app's code or its window's (inside a
    /// longer string too: "plugins.weather-feed" is Weather Feed's).
    #[test]
    fn no_plugin_is_reached_by_its_name() {
        let mut names: Vec<String> = Vec::new();
        for (folder, m) in manifests() {
            if folder.starts_with("plugins/") {
                names.extend(["id", "name"].iter().filter_map(|k| m[*k].as_str().map(String::from)));
            }
        }
        assert!(names.len() >= 10, "{names:?}");
        let mut found = Vec::new();
        for f in rust_files() {
            if f.ends_with("boundaries.rs") {
                continue;
            }
            for (n, line) in code(&f) {
                if let Some(name) = names.iter().find(|x| line.contains(x.as_str())) {
                    found.push(format!("{f}:{n} {name}"));
                }
            }
        }
        for (f, n, line) in ui_code() {
            if let Some(name) = names.iter().find(|x| line.contains(x.as_str())) {
                found.push(format!("{f}:{n} {name}"));
            }
        }
        fail_on(found, "a plugin named in the app");
    }

    /// docs/core-protocol.md, section 11, A game's start: a plugin names the
    /// programs its games run through and the variables it sets in its own
    /// manifest, so none of them is written into the app, and a plugin of
    /// anyone's gets its own start without Ectomatter changing.
    #[test]
    fn no_plugins_start_is_written_into_the_app() {
        let mut names: Vec<String> = Vec::new();
        for (_, m) in manifests() {
            for u in m["uses"].as_array().into_iter().flatten() {
                let l = &u["launch"];
                names.extend(l["wrap"].as_object().into_iter().flatten().map(|(p, _)| format!("\"{p}\"")));
                names.extend(l["env"].as_object().into_iter().flatten().map(|(k, _)| format!("\"{k}\"")));
            }
        }
        assert!(names.len() > 20, "the Game Configurator's own start is in its manifest");
        let mut found = Vec::new();
        for f in rust_files() {
            if f.ends_with("boundaries.rs") {
                continue;
            }
            for (n, line) in code(&f) {
                if let Some(name) = names.iter().find(|x| line.contains(x.as_str())) {
                    found.push(format!("{f}:{n} {name}"));
                }
            }
        }
        fail_on(found, "a plugin's program or variable written into the app");
    }

    /// Ectomatter installed on a computer without its source finds its cores
    /// and plugins: no path into the source folder is built into it.
    #[test]
    fn packages_are_found_where_ectomatter_is() {
        let mut found = Vec::new();
        for f in rust_files() {
            // (the tests' own fixtures, lcd_parity.rs a module of tests whole)
            if f.ends_with("boundaries.rs") || f.ends_with("tokens.rs") || f.ends_with("lcd_parity.rs") {
                continue;
            }
            for (n, line) in code(&f) {
                if line.contains("CARGO_MANIFEST_DIR") {
                    found.push(format!("{f}:{n}"));
                }
            }
        }
        fail_on(found, "a path into the source folder");
    }

    /// docs/core-protocol.md, principle 1: "the frontend lends it only the
    /// devices its manifest names." Every core is started the same way, and
    /// none by name.
    #[test]
    fn every_core_starts_the_same_way() {
        let mut found = Vec::new();
        for f in rust_files() {
            if f.ends_with("boundaries.rs") || f.ends_with("core_host.rs") {
                continue;
            }
            for (n, line) in code(&f) {
                if line.contains("Hidapi::new()") || line.contains("fn started_elsewhere") {
                    found.push(format!("{f}:{n} {}", line.trim()));
                }
            }
        }
        fail_on(found, "a core started on its own");
    }

    /// docs/core-protocol.md, displays: a text display is described with its
    /// `cols` and `rows`, and takes "as many rows as it has". The screen's
    /// engine takes the keyboard's screen's size from its core's description
    /// (lcd_page::size) and types none of its own: no constant for its columns
    /// or rows, no grid of a fixed size, no row cut or padded to twenty, and in
    /// the Slint pages, no grid of twenty by four and no picture of its shape.
    /// (The layouts Ectomatter ships keep their places as data, laid out on the
    /// Z10's, as a saved layout keeps its.)
    #[test]
    fn the_screen_is_the_size_its_core_describes() {
        let engine = ["lcd_page.rs", "lcd_anim.rs", "lcd_fx.rs", "lcd_draw.rs", "lcd_player.rs", "layout_page.rs", "screen_edit.rs", "overlays.rs"];
        let typed = ["const COLS", "const ROWS", "const W:", "const H:", "const PW", "const PH", "; W]", "; H]", "; 4]]", "; 20]", "take(20)", ":<20"];
        let mut found = Vec::new();
        for f in engine.map(|f| format!("apps/ectomatter/src/{f}")) {
            for (n, line) in code(&f) {
                if let Some(t) = typed.iter().find(|t| line.contains(*t)) {
                    found.push(format!("{f}:{n} {t}"));
                }
            }
        }
        // (the pages' code draws the screen too: its picture, its card's and the keyboard's display)
        let pages: Vec<String> = rust_files().into_iter().filter(|f| f.starts_with("apps/ectomatter/src/front")).collect();
        assert!(!pages.is_empty(), "the pages' code isn't where the check looks");
        for f in pages {
            for (n, line) in code(&f) {
                if line.contains("lcd") && (line.contains(":<20") || line.contains("take(20)") || line.contains("take(4)")) {
                    found.push(format!("{f}:{n}"));
                }
            }
        }
        // (every window's Slint, app.slint's and each under ui/)
        for (f, n, line) in ui_code() {
            if ["/ 20", "* 4 / 20", "for k in 19", "for k in 3:", "lcd-h * 5", "20 - b.width", "4 - b.height"].iter().any(|t| line.contains(t)) {
                found.push(format!("{f}:{n} {}", line.trim()));
            }
        }
        fail_on(found, "the screen's size typed in");
    }
}
