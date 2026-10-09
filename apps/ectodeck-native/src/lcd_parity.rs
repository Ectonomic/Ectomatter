//! Ectodeck's screen player against the z10-lcd service it replaces: the
//! frames the service sent on made-up inputs (tests/lcd_parity.py writes them
//! to tests/fixtures/lcd_parity.json, running the service's own code), and the
//! frames Ectodeck sends on the same inputs, which must be the same, character
//! for character and at the same moments.

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::lcd_anim::Ask;
use crate::lcd_player::{Core, Sink};
use crate::lcd_stats::tests::{computer, FakeGpu};
use crate::lcd_stats::Stats;

fn fixture() -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lcd_parity.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

#[test]
fn the_stats_page_reads_alike() {
    for (i, case) in fixture()["stats"].as_array().unwrap().iter().enumerate() {
        let c = &case["computer"];
        let dir = std::env::temp_dir().join(format!("ectodeck-parity-{}-{i}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let freqs: Vec<u64> = c["freqs"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
        let stat = |k: usize| -> [u64; 8] { std::array::from_fn(|j| c["stat"][k][j].as_u64().unwrap()) };
        let mem = (c["mem"][0].as_u64().unwrap(), c["mem"][1].as_u64().unwrap());
        computer(&dir, c["temp"].as_i64().unwrap(), &freqs, stat(0), mem);
        let g = &c["gpu"];
        let gpu = if g.is_null() {
            Err("no GPU".to_string())
        } else {
            Ok((g[0].as_u64().unwrap() as u32, g[1].as_u64().unwrap() as u32, g[2].as_u64().unwrap() as u32, g[3].as_f64().unwrap(), g[4].as_f64().unwrap()))
        };
        let mut s = Stats::new(&dir, Box::new(FakeGpu(vec![gpu])));
        s.cpu_load();
        computer(&dir, c["temp"].as_i64().unwrap(), &freqs, stat(1), mem);
        // read once, then shown by the Stats page as it ships and as it comes
        // back from Ectodeck's settings: both as the service wrote it
        let r = s.read();
        let saved = crate::lcd_page::Page::from_json(&crate::lcd_page::Page::stats().to_json()).unwrap();
        assert_eq!(crate::lcd_page::render(&crate::lcd_page::Page::stats(), &r), strings(&case["rows"]), "computer {i}");
        assert_eq!(crate::lcd_page::render(&saved, &r), strings(&case["rows"]), "computer {i}, the saved page");
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// Frames, kept with the moment each was sent.
struct Kept(Arc<Mutex<Vec<(f64, Vec<String>)>>>, Arc<Mutex<f64>>);
impl Sink for Kept {
    fn find(&mut self) -> bool {
        true
    }
    fn send(&mut self, rows: &[String], _cmd: u8) -> Result<(), ()> {
        let now = *self.1.lock().unwrap();
        self.0.lock().unwrap().push((now, rows.to_vec()));
        Ok(())
    }
}

#[test]
fn the_screen_plays_alike() {
    for (k, run) in fixture()["loops"].as_array().unwrap().iter().enumerate() {
        let dir = std::env::temp_dir().join(format!("ectodeck-parity-loop-{}-{k}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("session");
        let mut pending: Vec<(f64, Value)> = run["events"].as_array().unwrap().iter().map(|e| (e[0].as_f64().unwrap(), e[1].clone())).collect();
        let me = std::process::id().to_string();
        let apply = |pending: &mut Vec<(f64, Value)>, now: f64| {
            while pending.first().is_some_and(|(t, _)| *t <= now + 1e-9) {
                let (_, f) = pending.remove(0);
                if f.is_null() {
                    let _ = std::fs::remove_file(&note);
                } else {
                    let mut text = String::new();
                    for (key, v) in f.as_object().unwrap() {
                        let v = v.as_str().unwrap();
                        let v = if v == "self" { me.as_str() } else if v == "dead" { "999999999" } else { v };
                        text.push_str(&format!("{key}={v}\n"));
                    }
                    let tmp = dir.join("session.tmp");
                    std::fs::write(&tmp, text).unwrap();
                    std::fs::rename(&tmp, &note).unwrap();
                }
            }
        };
        // the fixture's fields are in the order the harness wrote them; serde_json
        // keeps keys sorted, which reads the same (the order of lines doesn't matter)
        let clock = Arc::new(Mutex::new(0.0f64));
        let sent = Arc::new(Mutex::new(Vec::new()));
        let n = Arc::new(Mutex::new(0u32));
        let stats: Ask<Vec<String>> = Arc::new(move || {
            let mut n = n.lock().unwrap();
            *n += 1;
            let n = *n;
            vec![format!("CPU {:>2}° {:>3}% {:>4}MHz", n % 100, n % 7, 4000 + n), format!("GPU {:>2}°", n % 50), format!("RAM {n}"), format!("VRAM{}", n % 3)]
        });
        let mut seed = 0u64;
        let seeds = Box::new(move || {
            seed += 1;
            seed
        });
        apply(&mut pending, 0.0);
        let mut core = Core::new(Box::new(Kept(sent.clone(), clock.clone())), stats, note.clone(), seeds);
        let until = run["until"].as_f64().unwrap();
        let mut now = 0.0f64;
        loop {
            *clock.lock().unwrap() = now;
            if !core.tick(now) {
                continue;
            }
            now += 0.01;
            apply(&mut pending, now);
            if now > until {
                break;
            }
        }
        let want: Vec<(f64, Vec<String>)> = run["pushes"].as_array().unwrap().iter().map(|p| (p[0].as_f64().unwrap(), strings(&p[1]))).collect();
        let got = sent.lock().unwrap().clone();
        for (i, (w, g)) in want.iter().zip(&got).enumerate() {
            assert!((w.0 - g.0).abs() < 1e-6 && w.1 == g.1, "{}: frame {i} differs: service {w:?}, Ectodeck {g:?}", run["name"]);
        }
        assert_eq!(want.len(), got.len(), "{}: frame counts differ", run["name"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
