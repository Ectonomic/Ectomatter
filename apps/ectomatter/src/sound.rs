//! What the computer is playing, for the Spectrum effect: its sound as twenty bands, low to
//! high, each 0 (quiet) to 12 (loud), as the effect counts a column's height in thirds of a
//! row. It is read from the sound server's monitor of the speakers (`parec`, which PipeWire
//! answers as PulseAudio did), only while something asks for the bands: listening starts at
//! the first ask and stops a few seconds after the last, and nothing heard is kept.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// samples a second, and in each look at the sound
const RATE: f32 = 16000.0;
const N: usize = 512;
/// how long listening goes on with nothing asking
const IDLE: Duration = Duration::from_secs(3);
/// how long before trying again when the sound couldn't be read
const RETRY: Duration = Duration::from_secs(10);

struct Ear {
    bands: [f32; 20],
    asked: Instant,
    listening: bool,
    failed: Option<Instant>,
}

static EAR: OnceLock<Arc<Mutex<Ear>>> = OnceLock::new();

/// The sound's twenty bands now: all 0 while nothing plays, or when the sound can't be read.
pub fn bands() -> [f32; 20] {
    if let Some((samples, at)) = played() {
        return looks(&samples, at).last().map(|l| l.1).unwrap_or([0.0; 20]);
    }
    let ear = EAR.get_or_init(|| Arc::new(Mutex::new(Ear { bands: [0.0; 20], asked: Instant::now(), listening: false, failed: None })));
    let mut e = ear.lock().unwrap();
    e.asked = Instant::now();
    if !e.listening && e.failed.is_none_or(|t| t.elapsed() >= RETRY) {
        e.listening = true;
        let ear = ear.clone();
        std::thread::spawn(move || listen(&ear));
    }
    e.bands
}

/// What a Spectrum stepping every `period` seconds shows: the loudest each band was during
/// the last step, changed only once a step, as the screen can show (see
/// [crate::lcd_fx::period]).
pub fn held(period: f64) -> [f32; 20] {
    if let Some((samples, at)) = played() {
        // (the step `at` falls in shows the loudest of the step before it)
        let to = at + samples.len() as f64 / RATE as f64;
        let step = (to / period).floor() * period;
        let mut peak = [0f32; 20];
        for (_, levels) in looks(&samples, at).into_iter().filter(|(t, _)| *t >= step - period && *t < step) {
            for (p, l) in peak.iter_mut().zip(levels) {
                *p = p.max(l);
            }
        }
        return peak;
    }
    static HELD: OnceLock<Mutex<(Option<Instant>, [f32; 20], [f32; 20])>> = OnceLock::new();
    let now = bands();
    let mut h = HELD.get_or_init(|| Mutex::new((None, [0.0; 20], [0.0; 20]))).lock().unwrap();
    for (peak, level) in h.2.iter_mut().zip(now) {
        *peak = peak.max(level);
    }
    if h.0.is_none_or(|since| since.elapsed().as_secs_f64() >= period) {
        (h.0, h.1, h.2) = (Some(Instant::now()), h.2, [0.0; 20]);
    }
    h.1
}

/// ECTOMATTER_SOUND=FILE and ECTOMATTER_SOUND_AT=s: for a picture drawn
/// offscreen, the sound is that file (16 kHz mono, as `parec` hands it here)
/// played over and over since long before, heard `s` seconds into a play of
/// it, in place of the speakers'.
fn played() -> Option<(Vec<f32>, f64)> {
    let file = std::env::var_os("ECTOMATTER_SOUND")?;
    let at = std::env::var("ECTOMATTER_SOUND_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let bytes = std::fs::read(file).ok()?;
    let samples: Vec<f32> = bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
    (!samples.is_empty()).then_some((samples, at))
}

/// The levels a sound played over and over leaves at each look, as `listen`
/// hears it, from the sound's start through a play and on to `at` seconds
/// into the next: when each look was heard, and the levels then.
fn looks(samples: &[f32], at: f64) -> Vec<(f64, [f32; 20])> {
    let to = at + samples.len() as f64 / RATE as f64;
    let mut levels = [0f32; 20];
    let mut out = Vec::new();
    let mut i = 0usize;
    while (i + N) as f64 / RATE as f64 <= to {
        let look: Vec<f32> = (i..i + N).map(|k| samples[k % samples.len()]).collect();
        for (l, now) in levels.iter_mut().zip(heard(&look)) {
            *l = now.max(*l - 0.5);
        }
        i += N;
        out.push((i as f64 / RATE as f64, levels));
    }
    out
}

/// Listen until nothing has asked for a while, or the sound can no longer be read.
fn listen(ear: &Mutex<Ear>) {
    let child = Command::new("parec")
        .args(["--device=@DEFAULT_MONITOR@", "--format=s16le", "--rate=16000", "--channels=1", "--raw", "--latency-msec=30"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        let mut e = ear.lock().unwrap();
        (e.listening, e.failed) = (false, Some(Instant::now()));
        return;
    };
    let mut out = child.stdout.take().expect("parec's output");
    let mut buf = vec![0u8; N * 2];
    let mut levels = [0f32; 20];
    let failed = loop {
        if out.read_exact(&mut buf).is_err() {
            break true;
        }
        let samples: Vec<f32> = buf.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
        // (a level rises at once and falls back a third of a row each look, as a meter does)
        for (l, now) in levels.iter_mut().zip(heard(&samples)) {
            *l = now.max(*l - 0.5);
        }
        let mut e = ear.lock().unwrap();
        e.bands = levels;
        if e.asked.elapsed() >= IDLE {
            break false;
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    let mut e = ear.lock().unwrap();
    e.listening = false;
    e.bands = [0.0; 20];
    if failed {
        e.failed = Some(Instant::now());
    }
}

/// One look at the sound, `N` samples: how loud it is in each of twenty bands spaced evenly
/// by pitch from 60 Hz to 7 kHz, from 0 to 12, the higher bands lifted a little, as music
/// carries less there.
fn heard(samples: &[f32]) -> [f32; 20] {
    let mut re: Vec<f32> = samples.iter().enumerate().map(|(i, s)| s * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (N - 1) as f32).cos())).collect();
    let mut im = vec![0f32; N];
    fft(&mut re, &mut im);
    let bin = RATE / N as f32;
    let (lo, hi) = (60f32, 7000f32);
    std::array::from_fn(|k| {
        let (a, b) = (lo * (hi / lo).powf(k as f32 / 20.0), lo * (hi / lo).powf((k + 1) as f32 / 20.0));
        let (first, last) = (((a / bin).round() as usize).max(1), ((b / bin).round() as usize).max(1));
        let peak = (first..=last.max(first)).map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt()).fold(0f32, f32::max);
        // (a whole-scale tone, windowed, is N / 4 tall)
        let db = 20.0 * (peak / (N as f32 / 4.0) + 1e-9).log10() + 3.0 * ((a * b).sqrt() / 120.0).log2();
        ((db + 54.0) / 54.0 * 12.0).clamp(0.0, 12.0)
    })
}

/// The sound's frequencies, in place: an iterative radix-2 Fourier transform.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -std::f32::consts::TAU / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1f32, 0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                (re[b], im[b]) = (re[a] - tr, im[a] - ti);
                re[a] += tr;
                im[a] += ti;
                (cr, ci) = (cr * wr - ci * wi, cr * wi + ci * wr);
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (needs the computer to be playing something: run by hand, with --ignored)
    #[test]
    #[ignore]
    fn the_computer_s_sound_is_heard_and_listening_stops_when_nothing_asks() {
        let start = Instant::now();
        let mut heard_any = false;
        while start.elapsed() < Duration::from_millis(1500) {
            heard_any |= bands().iter().any(|l| *l > 0.0);
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(heard_any, "nothing heard: is something playing?");
        std::thread::sleep(IDLE + Duration::from_millis(500));
        assert!(!EAR.get().unwrap().lock().unwrap().listening, "still listening with nothing asking");
    }

    #[test]
    fn a_tone_lights_its_own_band_and_quiet_lights_none() {
        assert!(heard(&[0.0; N]).iter().all(|l| *l == 0.0));
        // a loud 1 kHz tone: its band near the top, the lowest bands near nothing
        let tone: Vec<f32> = (0..N).map(|i| 0.8 * (std::f32::consts::TAU * 1000.0 * i as f32 / RATE).sin()).collect();
        let l = heard(&tone);
        let loudest = (0..20).max_by(|a, b| l[*a].total_cmp(&l[*b])).unwrap();
        let k1000 = (0..20).find(|k| 60.0 * (7000f32 / 60.0).powf((*k + 1) as f32 / 20.0) > 1000.0).unwrap();
        assert!(loudest.abs_diff(k1000) <= 1, "{l:?}");
        assert!(l[loudest] > 9.0 && l[0] < 3.0, "{l:?}");
    }

    #[test]
    fn a_sound_played_over_and_over_is_heard_as_the_speakers_are() {
        // a second: quiet, then a 1 kHz beep in its last tenth, over and over
        let beep: Vec<f32> = (0..RATE as usize).map(|i| if i >= RATE as usize * 9 / 10 { 0.8 * (std::f32::consts::TAU * 1000.0 * i as f32 / RATE).sin() } else { 0.0 }).collect();
        let heard_at = |at: f64| looks(&beep, at).last().unwrap().1;
        let k1000 = (0..20).find(|k| 60.0 * (7000f32 / 60.0).powf((*k + 1) as f32 / 20.0) > 1000.0).unwrap();
        // (during the beep, its band; most of a second after it, nothing, as the
        // meter falls back a third of a row each look)
        assert!(heard_at(0.98)[k1000] > 9.0, "{:?}", heard_at(0.98));
        assert!(heard_at(0.85).iter().all(|l| *l == 0.0), "{:?}", heard_at(0.85));
        // (just after the start, the meter still falls from the last play's
        // beep, as the speakers' would: not from quiet, as a first play's)
        assert!(heard_at(0.05)[k1000] > 3.0, "{:?}", heard_at(0.05));
    }

}
