//! Whether this computer lets the person at it open the devices a core
//! reaches, and the rule that does. On Linux a program opens a USB device
//! only where a udev rule lets it: `tools/install.sh` writes one rule file for
//! the cores that come with Ectomatter (`ectomatter --udev-rules`); a core
//! added later gets its own, written into udev's folder for the computer's
//! own rules with the system's password prompt (pkexec), and udev told to
//! read it. Elsewhere a program needs no rule, and nothing is asked.

use std::path::Path;

use serde_json::Value;

/// The USB devices a manifest's core reaches, by their ids in lowercase hex.
pub fn devices(manifest: &Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for m in manifest["matches"].as_array().into_iter().flatten() {
        let (Some(v), Some(p)) = (m["usb"]["vendor"].as_str(), m["usb"]["product"].as_str()) else { continue };
        let id = (v.to_lowercase(), p.to_lowercase());
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// The rule that lets the person at the computer open these devices of a
/// core (its name and id said above each).
pub fn rule(name: &str, id: &str, devices: &[(String, String)]) -> String {
    devices.iter().map(|(v, d)| format!("# {name} ({id})\nSUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"{v}\", ATTRS{{idProduct}}==\"{d}\", TAG+=\"uaccess\"\n")).collect()
}

/// udev's folders of rules: the computer's own, the running system's, and
/// those its packages put in.
#[cfg(target_os = "linux")]
const RULES: &[&str] = &["/etc/udev/rules.d", "/run/udev/rules.d", "/usr/local/lib/udev/rules.d", "/usr/lib/udev/rules.d", "/lib/udev/rules.d"];

/// Of these devices, those no rule in `folders` names.
pub fn missing_in(devices: &[(String, String)], folders: &[&Path]) -> Vec<(String, String)> {
    let mut text = String::new();
    for f in folders {
        for e in std::fs::read_dir(f).into_iter().flatten().flatten() {
            if e.path().extension().is_some_and(|x| x == "rules") {
                text += &std::fs::read_to_string(e.path()).unwrap_or_default().to_lowercase();
                text.push('\n');
            }
        }
    }
    devices.iter().filter(|(v, d)| !text.lines().any(|l| !l.trim_start().starts_with('#') && l.contains(&format!("idvendor}}==\"{v}\"")) && l.contains(&format!("idproduct}}==\"{d}\"")))).cloned().collect()
}

/// Of a manifest's devices, those this computer has no rule for (none where
/// a program needs no rule).
pub fn missing(manifest: &Value) -> Vec<(String, String)> {
    #[cfg(target_os = "linux")]
    {
        let folders: Vec<std::path::PathBuf> = RULES.iter().map(|f| crate::computer::here(f)).collect();
        missing_in(&devices(manifest), &folders.iter().map(|f| f.as_path()).collect::<Vec<_>>())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = manifest;
        Vec::new()
    }
}

/// The rule for a core's devices put in `folder` by `admin` (the program that
/// runs a command as the computer's administrator, after asking for the
/// password), and udev told to read the rules again and look at the devices
/// plugged in now.
pub fn grant_with(admin: &[&str], folder: &Path, manifest: &Value) -> Result<(), String> {
    let id = manifest["id"].as_str().filter(|i| !i.is_empty() && i.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).ok_or("its manifest names no id")?;
    let text = rule(manifest["name"].as_str().unwrap_or(id), id, &devices(manifest));
    let file = folder.join(format!("70-ectomatter-{id}.rules"));
    let (program, before) = admin.split_first().map_or(("sh", &[][..]), |(p, rest)| (*p, rest));
    let mut c = std::process::Command::new(program);
    c.args(before);
    if !admin.is_empty() {
        c.arg("/bin/sh");
    }
    c.args(["-c", "cat > \"$1\" && udevadm control --reload-rules && udevadm trigger --action=add --subsystem-match=hidraw", "sh"]).arg(&file);
    let mut run = c.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped()).spawn().map_err(|e| format!("{program} didn’t start: {e}"))?;
    if let Some(mut i) = run.stdin.take() {
        use std::io::Write;
        i.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    }
    let out = run.wait_with_output().map_err(|e| e.to_string())?;
    match out.status.code() {
        Some(0) => Ok(()),
        // (pkexec's for a password not given, and for being turned down)
        Some(126) | Some(127) if !admin.is_empty() => Err("the password wasn’t given".into()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().lines().last().unwrap_or("the rule wasn’t written").to_string()),
    }
}

/// The rule for a core's devices written with the system's password prompt,
/// where this computer needs one.
pub fn grant(manifest: &Value) -> Result<(), String> {
    if missing(manifest).is_empty() {
        return Ok(());
    }
    grant_with(&["pkexec"], Path::new("/etc/udev/rules.d"), manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mouse() -> Value {
        json!({ "id": "test-mouse", "name": "Test Mouse", "matches": [{ "usb": { "vendor": "1A2B", "product": "3C4D", "interface": 0 } }, { "usb": { "vendor": "1a2b", "product": "3c4d", "interface": 1 } }] })
    }

    /// A core's devices with no rule are found, and the rule written for them
    /// names each once; with it in udev's folder, none is missing.
    #[test]
    fn a_new_cores_devices_get_a_rule() {
        let s = std::env::temp_dir().join(format!("ectomatter-permission-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&s);
        let (rules, bin) = (s.join("rules.d"), s.join("bin"));
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        let m = mouse();
        assert_eq!(devices(&m), [("1a2b".to_string(), "3c4d".to_string())], "one device, however many interfaces");
        assert_eq!(missing_in(&devices(&m), &[&rules]).len(), 1);
        // (udev stood in for by a program that writes down what it was asked)
        let told = s.join("udevadm.told");
        std::fs::write(bin.join("udevadm"), format!("#!/bin/sh\necho \"$@\" >> {}\n", told.display())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(bin.join("udevadm"), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = format!("PATH={}:/usr/bin:/bin", bin.display());
        grant_with(&["env", &path], &rules, &m).unwrap();
        let written = std::fs::read_to_string(rules.join("70-ectomatter-test-mouse.rules")).unwrap();
        assert_eq!(written, "# Test Mouse (test-mouse)\nSUBSYSTEM==\"hidraw\", ATTRS{idVendor}==\"1a2b\", ATTRS{idProduct}==\"3c4d\", TAG+=\"uaccess\"\n");
        assert!(missing_in(&devices(&m), &[&rules]).is_empty(), "the rule written is found");
        assert_eq!(std::fs::read_to_string(&told).unwrap(), "control --reload-rules\ntrigger --action=add --subsystem-match=hidraw\n");
        // (a rule that's only a comment names nothing)
        std::fs::write(rules.join("70-ectomatter-test-mouse.rules"), "# ATTRS{idVendor}==\"1a2b\", ATTRS{idProduct}==\"3c4d\"\n").unwrap();
        assert_eq!(missing_in(&devices(&m), &[&rules]).len(), 1);
        let _ = std::fs::remove_dir_all(&s);
    }
}
