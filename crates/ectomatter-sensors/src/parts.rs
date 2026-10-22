//! The names of the computer's parts, found under `root` (`/` but for tests):
//! the processor's, the motherboard's, and each graphics card's.

use std::path::Path;

use crate::files;

/// The processor's name, as its sensors' heading: "AMD Ryzen 9 9950X3D".
pub(crate) fn cpu_name(root: &Path) -> String {
    let info = files::read_to_string(root.join("proc/cpuinfo")).unwrap_or_default();
    let model = info.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.trim() == "model name").map(|(_, v)| v.trim().to_string())).unwrap_or_else(|| "The processor".into());
    let mut name = model.replace("(R)", "").replace("(TM)", "").replace("(tm)", "");
    for cut in [" with Radeon Graphics", " Processor", " CPU"] {
        if let Some(i) = name.find(cut) {
            name.truncate(i);
        }
    }
    // ("16-Core", "Eight-Core" and the clock Intel adds: "@ 3.60GHz")
    let words: Vec<&str> = name.split_whitespace().filter(|w| !w.ends_with("-Core") && !w.starts_with('@') && !w.ends_with("GHz")).collect();
    words.join(" ")
}

/// The motherboard's name, as its sensors' heading: "MSI MEG X670E ACE".
pub(crate) fn board_name(root: &Path) -> Option<String> {
    let read = |f: &str| files::read_to_string(root.join("sys/class/dmi/id").join(f)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let board = read("board_name")?;
    // (the maker's model code in brackets, "(MS-7D69)", left off)
    let board = board.split(" (").next().unwrap_or(&board).trim().to_string();
    let vendor = read("board_vendor").map(|v| short_vendor(&v)).unwrap_or_default();
    Some(if vendor.is_empty() || board.starts_with(&vendor) { board } else { format!("{vendor} {board}") })
}

/// Names in a sentence: "A", "A and B", "A, B and C".
pub fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A maker as people say it: Micro-Star International Co., Ltd. is MSI.
pub fn short_vendor(v: &str) -> String {
    let l = v.to_lowercase();
    for (k, s) in [("micro-star", "MSI"), ("asustek", "ASUS"), ("gigabyte", "Gigabyte"), ("asrock", "ASRock"), ("nvidia", "NVIDIA"), ("advanced micro devices", "AMD"), ("intel", "Intel"), ("dell", "Dell"), ("lenovo", "Lenovo"), ("hewlett", "HP"), ("biostar", "Biostar")] {
        if l.contains(k) {
            return s.to_string();
        }
    }
    v.split_whitespace().next().unwrap_or("").trim_end_matches(',').to_string()
}

// ------------------------------------------------------------------ graphics cards

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
}

/// A graphics card in this computer.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Card {
    /// its PCI address, 0000:01:00.0
    pub pci: String,
    pub name: String,
    pub vendor: Vendor,
    /// the graphics in the processor, rather than a card of its own
    pub integrated: bool,
}

/// The graphics cards Ectomatter can read, in their PCI addresses' order.
pub(crate) fn cards(root: &Path) -> Vec<Card> {
    let Ok(rd) = files::read_dir(root.join("sys/bus/pci/devices")) else { return Vec::new() };
    let hex = |p: &Path, f: &str| files::read_to_string(p.join(f)).ok().and_then(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok());
    let ids = files::read_to_string(root.join("usr/share/hwdata/pci.ids")).unwrap_or_default();
    let mut all: Vec<Card> = rd
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let pci = e.file_name().to_str()?.to_string();
            (hex(&dir, "class")? >> 16 == 0x03).then_some(())?;
            let (v, d) = (hex(&dir, "vendor")?, hex(&dir, "device")?);
            let vendor = match v {
                0x10de => Vendor::Nvidia,
                0x1002 => Vendor::Amd,
                0x8086 => Vendor::Intel,
                _ => return None,
            };
            let maker = match vendor {
                Vendor::Nvidia => "NVIDIA",
                Vendor::Amd => "AMD",
                Vendor::Intel => "Intel",
            };
            let model = pci_name(&ids, v, d).unwrap_or_else(|| format!("graphics {d:04x}"));
            // (behind the processor's own bridge: AMD's at 00:08, Intel's at 00:02.0 itself)
            let parent = files::canonicalize(&dir).ok().and_then(|p| p.parent().and_then(|q| q.file_name()).map(|n| n.to_string_lossy().into_owned())).unwrap_or_default();
            let integrated = match vendor {
                Vendor::Amd => parent.ends_with(":00:08.1") || parent.ends_with(":00:08.2") || parent.ends_with(":00:08.3"),
                Vendor::Intel => pci.ends_with(":00:02.0"),
                Vendor::Nvidia => false,
            };
            Some(Card { pci, name: format!("{maker} {model}"), vendor, integrated })
        })
        .collect();
    all.sort_by(|a, b| a.pci.cmp(&b.pci));
    all
}

/// A card's model, from the PCI ID list: the name in its brackets where it has one
/// ("GB202 [GeForce RTX 5090]" is GeForce RTX 5090).
fn pci_name(ids: &str, vendor: u32, device: u32) -> Option<String> {
    let (v, d) = (format!("{vendor:04x}"), format!("\t{device:04x}"));
    let mut lines = ids.lines().skip_while(|l| !l.starts_with(&v));
    lines.next()?;
    // (the vendor's devices run to the next vendor, comments among them)
    let line = lines.take_while(|l| !l.starts_with(|c: char| c.is_ascii_hexdigit())).find(|l| l.starts_with(&d))?;
    let name = line[d.len()..].trim();
    Some(match (name.find('['), name.rfind(']')) {
        (Some(a), Some(b)) if b > a => name[a + 1..b].to_string(),
        _ => name.to_string(),
    })
}

