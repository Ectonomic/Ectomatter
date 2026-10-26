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
mod tests {
    use std::path::{Path, PathBuf};

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn read(rel: &str) -> String {
        std::fs::read_to_string(repo().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }

    /// A Rust file's code, each line with its number: its comments out, and
    /// every item under `#[cfg(test)]` left out (found by reading the file
    /// as Rust).
    fn code(rel: &str) -> Vec<(usize, String)> {
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

    fn rust_files() -> Vec<String> {
        let mut out = Vec::new();
        let mut dirs = vec![repo().join("apps/ectomatter/src")];
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

    fn manifests() -> Vec<(String, serde_json::Value)> {
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

    fn fail_on(found: Vec<String>, what: &str) {
        assert!(found.is_empty(), "{what}:\n  {}", found.join("\n  "));
    }

    /// docs/core-protocol.md: "the frontend has one screen per capability, so
    /// every device that offers a capability is edited the same way." A device
    /// is only its core, so the app builds in no device's driver.
    #[test]
    #[ignore = "known failure: the app links the EVGA Z10's driver (docs/deferred.md, stage 2)"]
    fn the_app_links_no_devices_driver() {
        let toml = read("apps/ectomatter/Cargo.toml");
        let deps = toml.split("[dependencies]").nth(1).unwrap_or("").split("\n[").next().unwrap_or("");
        let found: Vec<String> = deps.lines().filter(|l| l.trim_start().starts_with("evga-z10")).map(|l| l.trim().to_string()).collect();
        fail_on(found, "apps/ectomatter/Cargo.toml depends on a device's driver");
    }

    /// docs/core-protocol.md: "has no page of its own for any device". No page
    /// is found by a device's name, and nothing is kept under one.
    #[test]
    #[ignore = "known failure: the keyboard and mouse pages are the Z10's and the Sensei 310's (docs/deferred.md, stage 2)"]
    fn no_page_is_found_by_a_devices_name() {
        let names = ["\"z10\"", "\"mouse\"", "\"sensei310\"", "\"sensei310-1\"", "\"evga-z10\"", "\"steelseries-sensei-310\""];
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
        let ui = read("apps/ectomatter/ui/app.slint");
        for (i, line) in ui.lines().enumerate() {
            let line = line.split("//").next().unwrap_or("");
            if let Some(name) = names.iter().find(|x| line.contains(*x)) {
                found.push(format!("apps/ectomatter/ui/app.slint:{} {name}", i + 1));
            }
        }
        fail_on(found, "a device named in the app");
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
    #[ignore = "known failure: the Z10's and the mouse's cores are started by name (docs/deferred.md, stage 2)"]
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
}
