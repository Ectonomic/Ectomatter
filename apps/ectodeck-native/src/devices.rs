//! What Ectodeck can see of a device from outside it, without opening it:
//! whether it is plugged in, whether Ectodeck is allowed to open it, and which
//! other programs already have it open.
//!
//! On Linux all three come from the kernel's own files. Elsewhere there is no
//! equivalent to ask, so the answers are left unknown rather than guessed.

use serde_json::{Value, json};

const VID: u16 = 0x3842;
const PID: u16 = 0x2611;

#[cfg(target_os = "linux")]
pub fn z10() -> Value {
    use std::fs;
    let read = |p: &str| fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default();

    // plugged in: a USB device with the Z10's ids
    let present = fs::read_dir("/sys/bus/usb/devices").into_iter().flatten().flatten().any(|d| {
        let p = d.path();
        read(&format!("{}/idVendor", p.display())) == format!("{VID:04x}")
            && read(&format!("{}/idProduct", p.display())) == format!("{PID:04x}")
    });

    // the keyboard's HID nodes, which are what programs open
    let id = format!("HID_ID=0003:{:08X}:{:08X}", VID, PID);
    let nodes: Vec<String> = fs::read_dir("/sys/class/hidraw").into_iter().flatten().flatten()
        .filter(|d| read(&format!("{}/device/uevent", d.path().display())).to_uppercase().contains(&id))
        .map(|d| format!("/dev/{}", d.file_name().to_string_lossy()))
        .collect();

    // allowed to open it: every node opens for reading and writing
    let reachable = !nodes.is_empty()
        && nodes.iter().all(|n| fs::OpenOptions::new().read(true).write(true).open(n).is_ok());

    // who else has it open: any other process of this user's holding a node
    let me = std::process::id();
    let mut holders: Vec<Value> = Vec::new();
    for p in fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Ok(pid) = p.file_name().to_string_lossy().parse::<u32>() else { continue };
        if pid == me {
            continue;
        }
        let holds = fs::read_dir(p.path().join("fd")).into_iter().flatten().flatten()
            .filter_map(|fd| fs::read_link(fd.path()).ok())
            .any(|t| nodes.iter().any(|n| t.to_string_lossy() == *n));
        if holds && !holders.iter().any(|h| h["pid"] == pid) {
            holders.push(json!({ "pid": pid, "name": program_name(pid) }));
        }
    }
    json!({ "present": present, "reachable": reachable, "holders": holders })
}

/// The name a person would know a program by: an interpreter's script or
/// module rather than the interpreter, and a path's last part.
#[cfg(target_os = "linux")]
fn program_name(pid: u32) -> String {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let args: Vec<String> = raw.split(|b| *b == 0).filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned()).collect();
    let base = |s: &str| s.rsplit('/').next().unwrap_or(s).to_string();
    let Some(first) = args.first() else { return format!("process {pid}") };
    let prog = base(first);
    if prog.starts_with("python") || prog == "node" || prog == "perl" {
        if let Some(i) = args.iter().position(|a| a == "-m") {
            if let Some(m) = args.get(i + 1) {
                return m.clone();
            }
        }
        if let Some(script) = args.iter().skip(1).find(|a| !a.starts_with('-')) {
            return base(script);
        }
    }
    prog
}

#[cfg(not(target_os = "linux"))]
pub fn z10() -> Value {
    json!({ "present": null, "reachable": null, "holders": [] })
}
