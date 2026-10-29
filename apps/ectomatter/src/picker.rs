//! The desktop's own file picker, through the desktop portal
//! (org.freedesktop.portal.FileChooser), so the person chooses a file the
//! way they do in every other program on their desktop. Asked away from the
//! window's thread; the answer comes back on a channel.

#[cfg(target_os = "linux")]
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;

#[cfg(target_os = "linux")]
use zbus::wire::{OwnedObjectPath, OwnedVariant, Variant};

#[cfg(target_os = "linux")]
/// Ask the desktop for one file, titled `title`, of the types named `kind`
/// with these patterns; the channel gets its path, or none where the person
/// closed the picker or the desktop has no portal.
pub fn open_file(title: &str, kind: &str, patterns: &[&str]) -> mpsc::Receiver<Option<PathBuf>> {
    let (tx, rx) = mpsc::channel();
    let (title, kind) = (title.to_string(), kind.to_string());
    let patterns: Vec<String> = patterns.iter().map(|p| p.to_string()).collect();
    std::thread::spawn(move || {
        let chosen = ask(&title, &kind, &patterns).unwrap_or_else(|e| {
            eprintln!("ectomatter: the desktop's file picker: {e}");
            None
        });
        let _ = tx.send(chosen);
    });
    rx
}

/// A folder shown in the desktop's file manager.
pub fn show_folder(folder: &std::path::Path) {
    // (each system's own way to open a folder in its file manager)
    let opener = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(folder).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

#[cfg(target_os = "linux")]
fn ask(title: &str, kind: &str, patterns: &[String]) -> zbus::Result<Option<PathBuf>> {
    zbus::block_on(asked(title, kind, patterns))
}

#[cfg(target_os = "linux")]
async fn asked(title: &str, kind: &str, patterns: &[String]) -> zbus::Result<Option<PathBuf>> {
    let conn = zbus::Connection::session().await?;
    // the request's path is known before asking, so its answer cannot be missed
    let token = format!("ectomatter{}", std::process::id());
    let sender = conn.unique_name().map(|n| n.trim_start_matches(':').replace('.', "_")).unwrap_or_default();
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let request = zbus::Proxy::new(&conn, "org.freedesktop.portal.Desktop", path.as_str(), "org.freedesktop.portal.Request").await?;
    let mut answers = request.receive_signal("Response").await?;

    let filter: Vec<(u32, String)> = patterns.iter().map(|p| (0u32, p.clone())).collect();
    let mut options: HashMap<&str, Variant> = HashMap::new();
    options.insert("handle_token", Variant::from(token.as_str()));
    options.insert("modal", Variant::from(true));
    options.insert("filters", Variant::from(vec![(kind.to_string(), filter)]));
    let chooser = zbus::Proxy::new(&conn, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.FileChooser").await?;
    let _: OwnedObjectPath = chooser.call("OpenFile", &("", title, options)).await?;

    let Some(answer) = crate::bus::next(&mut answers).await else { return Ok(None) };
    let (response, results): (u32, HashMap<String, OwnedVariant>) = answer.body().deserialize()?;
    if response != 0 {
        return Ok(None);
    }
    let uris: Vec<String> = results.get("uris").and_then(|v| v.try_clone().ok()).and_then(|v| Vec::<String>::try_from(v).ok()).unwrap_or_default();
    Ok(uris.first().and_then(|u| file_path(u)))
}

#[cfg(target_os = "linux")]
/// A file's path from its file:// address, its %-escapes undone.
fn file_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    use std::os::unix::ffi::OsStringExt;
    Some(PathBuf::from(std::ffi::OsString::from_vec(out)))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn a_file_s_address_gives_its_path() {
        assert_eq!(super::file_path("file:///home/someone/Pictures/keycaps/owl%20face.svg"), Some("/home/someone/Pictures/keycaps/owl face.svg".into()));
        assert_eq!(super::file_path("file:///tmp/a%25b.svg"), Some("/tmp/a%b.svg".into()));
        assert_eq!(super::file_path("https://example.com/a.svg"), None);
    }
}

// Elsewhere: no file picker yet (docs/deferred.md, stage 5): the person is
// told none was chosen.
#[cfg(not(target_os = "linux"))]
pub fn open_file(_: &str, _: &str, _: &[&str]) -> mpsc::Receiver<Option<PathBuf>> {
    let (tx, rx) = mpsc::channel();
    let _ = tx.send(None);
    rx
}
