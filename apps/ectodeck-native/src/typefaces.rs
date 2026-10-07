//! The typefaces a person adds to Ectodeck's own (print::LIBRARY): kept in
//! Ectodeck's data, typefaces/, so a setup that names one finds it on this
//! computer whatever the computer has installed. Legends are set only in
//! Ectodeck's typefaces, the cores' and these; never in the computer's.

use std::path::{Path, PathBuf};

/// Where the added typefaces are kept.
pub fn dir() -> PathBuf {
    crate::data_dir().join("typefaces")
}

/// The added typefaces' files.
pub fn files() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf")))
        .collect();
    out.sort();
    out
}

/// A typeface's file added: copied into Ectodeck's data (the same file
/// again keeps its copy) and taken in by the legends. The families it
/// holds, or a sentence saying why not.
pub fn add(from: &Path) -> Result<Vec<String>, String> {
    let families = ectodeck_draw::print::families_in(from);
    if families.is_empty() {
        return Err("That file isn't a typeface Ectodeck can read. Choose a .ttf or .otf file.".into());
    }
    let folder = dir();
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(from).map_err(|_| "That file can't be read.".to_string())?;
    let name = from.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "typeface.ttf".into());
    let mut to = folder.join(&name);
    let mut n = 2;
    while to.exists() && std::fs::read(&to).ok().as_deref() != Some(bytes.as_slice()) {
        to = folder.join(format!("{}-{n}.{}", Path::new(&name).file_stem().unwrap_or_default().to_string_lossy(), Path::new(&name).extension().unwrap_or_default().to_string_lossy()));
        n += 1;
    }
    if !to.exists() {
        std::fs::write(&to, bytes).map_err(|e| e.to_string())?;
    }
    ectodeck_draw::print::add_fonts([to]);
    ectodeck_draw::print::reload();
    Ok(families)
}

/// The typefaces the person's setups name that Ectodeck has none of, taken
/// in from this computer once, where it has them installed: a setup made
/// before Ectodeck kept its own typefaces keeps its look, now on any computer.
pub fn adopt_named(names: &[String]) -> Vec<String> {
    let have: Vec<String> = ectodeck_draw::print::families(&ectodeck_draw::print::faces()).into_iter().map(|(n, _)| n).collect();
    let wanted: Vec<&String> = names.iter().filter(|n| !n.is_empty() && !have.contains(n)).collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut system = resvg::usvg::fontdb::Database::new();
    system.load_system_fonts();
    let mut adopted = Vec::new();
    for name in wanted {
        let files: Vec<PathBuf> = system
            .faces()
            .filter(|f| f.families.iter().any(|(n, _)| n == name))
            .filter_map(|f| match &f.source {
                resvg::usvg::fontdb::Source::File(p) => Some(p.clone()),
                _ => None,
            })
            .collect();
        let mut any = false;
        for f in files {
            any |= add(&f).is_ok();
        }
        if any {
            adopted.push(name.clone());
        }
    }
    adopted
}
