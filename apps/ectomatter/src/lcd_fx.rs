//! The screen's effects: moving patterns made of the characters the keyboard draws, as the
//! first screen mock drew them (mock/z10-redesign/lcd.js, carried over line for line), each
//! the whole screen at a moment `t`, counted in tenths of a second. An Effect block shows the
//! cells of its own place on the screen, behind the other blocks.
//!
//! The mock drew them on the Z10's twenty by four; each is drawn here on a screen of any size,
//! the size the keyboard's core describes (lcd_page::size), and on twenty by four draws exactly
//! what the mock drew. Where an effect's look is a count of things (drops in a column, stars,
//! flakes, rockets), the count goes with the screen's size, as many to a cell as on the mock's.

use std::cell::RefCell;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const FULL: char = '█';
/// a drop at the top, middle and foot of a row
const SUB: [char; 3] = ['\'', '-', '.'];
/// from nothing to the most, a cell at a time
const RAMP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

/// A screen's size, its columns and rows (see [crate::lcd_page::size]).
pub type Size = (usize, usize);

/// A screen's cells at a moment, row by row, each row as many cells as the screen is wide.
pub type Grid = Vec<Vec<char>>;

/// An effect, as its tile names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fx {
    Rain,
    Snow,
    Stars,
    Code,
    Fire,
    Plasma,
    Fireworks,
    Spectrum,
    Heartbeat,
    Aquarium,
    Pong,
    Tetris,
    Snake,
    Highway,
    Invaders,
    Bounce,
}

/// The effects, in the tiles' order.
pub const FXS: [Fx; 16] = [
    Fx::Rain,
    Fx::Snow,
    Fx::Stars,
    Fx::Code,
    Fx::Fire,
    Fx::Plasma,
    Fx::Fireworks,
    Fx::Spectrum,
    Fx::Heartbeat,
    Fx::Aquarium,
    Fx::Pong,
    Fx::Tetris,
    Fx::Snake,
    Fx::Highway,
    Fx::Invaders,
    Fx::Bounce,
];

impl Fx {
    pub fn key(self) -> &'static str {
        match self {
            Fx::Rain => "rain",
            Fx::Snow => "snow",
            Fx::Stars => "stars",
            Fx::Code => "code",
            Fx::Fire => "fire",
            Fx::Plasma => "plasma",
            Fx::Fireworks => "fireworks",
            Fx::Spectrum => "spectrum",
            Fx::Heartbeat => "heartbeat",
            Fx::Aquarium => "aquarium",
            Fx::Pong => "pong",
            Fx::Tetris => "tetris",
            Fx::Snake => "snake",
            Fx::Highway => "highway",
            Fx::Invaders => "invaders",
            Fx::Bounce => "bounce",
        }
    }

    pub fn from_key(k: &str) -> Option<Fx> {
        FXS.into_iter().find(|f| f.key() == k)
    }

    pub fn name(self) -> &'static str {
        match self {
            Fx::Rain => "Rain",
            Fx::Snow => "Snow",
            Fx::Stars => "Starfield",
            Fx::Code => "Code rain",
            Fx::Fire => "Fire",
            Fx::Plasma => "Plasma",
            Fx::Fireworks => "Fireworks",
            Fx::Spectrum => "Spectrum",
            Fx::Heartbeat => "Heartbeat",
            Fx::Aquarium => "Aquarium",
            Fx::Pong => "Pong",
            Fx::Tetris => "Tetris",
            Fx::Snake => "Snake",
            Fx::Highway => "Highway",
            Fx::Invaders => "Space Invaders",
            Fx::Bounce => "Bounce",
        }
    }

    /// The effect as a sentence says it, for a layout's line ("The time, over rain."):
    /// the games by their names, the rest as things.
    pub fn said(self) -> &'static str {
        match self {
            Fx::Rain => "rain",
            Fx::Snow => "snow",
            Fx::Stars => "a starfield",
            Fx::Code => "code rain",
            Fx::Fire => "fire",
            Fx::Plasma => "plasma",
            Fx::Fireworks => "fireworks",
            Fx::Spectrum => "a spectrum",
            Fx::Heartbeat => "a heartbeat",
            Fx::Aquarium => "an aquarium",
            Fx::Pong => "Pong",
            Fx::Tetris => "Tetris",
            Fx::Snake => "Snake",
            Fx::Highway => "a highway",
            Fx::Invaders => "Space Invaders",
            Fx::Bounce => "a bouncing word",
        }
    }
}

/// The Speed slider's ends, in seconds a step: Slow, two seconds; Fast, a twentieth. The
/// keyboard's screen is slow (its liquid crystal: a cell changed again within about a
/// quarter second shows faded or not at all, as Grant saw on 2026-10-13, Bounce at a cell
/// every 0.2 s at the edge and Pong's ball every 0.12 s barely showing), and Fast reaches well
/// past that so the limit can be found on the keyboard itself; Fast comes down to the limit
/// once it is.
pub const SLOWEST: f64 = 2.0;
pub const FASTEST: f64 = 0.05;

/// How long an effect holds each step at `speed`, 0 slow to 100 fast: between the two ends
/// by ratio, so each notch of the slider changes it by the same share.
pub fn period(speed: u8) -> f64 {
    SLOWEST * (FASTEST / SLOWEST).powf(speed.min(100) as f64 / 100.0)
}

/// How far an effect's own clock (the mock's tenths of a second) goes in one step: as far as
/// moves its quickest part about one notch (a drop a third of a row, a star or the ball a
/// cell, a flame a shade), so it moves as the mock drew it, only never faster than the
/// screen shows. (Spectrum steps with the sound instead: see [crate::sound::held].)
fn per_step(fx: Fx) -> f64 {
    match fx {
        // the quickest drop or star: 1.7 a tenth
        Fx::Rain | Fx::Stars => 1.0 / 1.7,
        Fx::Snow => 1.0 / 0.35,
        // the quickest column's head: 0.75 a tenth
        Fx::Code => 1.0 / 0.75,
        // a shade about every half a tenth, where its waves move fastest
        Fx::Fire => 0.5,
        Fx::Plasma => 1.75,
        // (a game's move, a rocket's stage, a column of the trace: one a step)
        Fx::Fireworks | Fx::Heartbeat | Fx::Tetris | Fx::Snake | Fx::Highway | Fx::Invaders => 1.0,
        // (the bubbles: half a step a tenth)
        Fx::Aquarium => 2.0,
        Fx::Pong => 1.2,
        Fx::Bounce => 2.0,
        Fx::Spectrum => 1.0,
    }
}

/// The moment an effect's tile pictures it at while it isn't chosen, in the tenths the effects
/// count in: four seconds in, for most, and for Space Invaders the first shot climbing at
/// the wave.
pub fn still(fx: Fx) -> f64 {
    match fx {
        Fx::Invaders => 1.0,
        _ => 40.0,
    }
}

/// An effect's moment `secs` after its layout came on, at `speed`, in the tenths the
/// effects count in: whole steps only, so no cell changes more than once a step.
pub fn tenths(fx: Fx, secs: f64, speed: u8) -> f64 {
    (secs.max(0.0) / period(speed)).floor() * per_step(fx)
}

/// The mock's noise: for each `i` a number from 0 to 1, always the same one.
fn rnd(i: f64) -> f64 {
    let x = (i * 12.9898).sin() * 43758.5453;
    x - x.floor()
}

/// What the live effects draw from, read where the screen is drawn: the computer's sound for
/// Spectrum, in bands low to high, each 0 to [LOUDEST] (see [crate::sound::held]), spread
/// across the screen's columns, and the heartbeat's trace, its newest column at the right (see
/// [pulse]). Either may be empty: nothing heard, the trace flat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Live {
    pub sound: Vec<f32>,
    pub pulse: Vec<u8>,
}

/// How loud a band of the sound can be: three steps for each of the Z10's four rows, as the
/// sound is measured (see [crate::sound::held]).
const LOUDEST: f64 = 12.0;

impl Live {
    /// Nothing heard, the trace flat.
    pub fn quiet() -> Live {
        Live::default()
    }

    /// As the tiles show the live effects standing still, on a screen `cols` wide: Spectrum as
    /// the mock moved it, and a beat every ten columns.
    pub fn sample(cols: usize) -> Live {
        Live { sound: sample_sound(40.0, cols), pulse: sample_pulse(cols) }
    }
}

/// What effect `fx` at `speed` draws from now, on a screen `cols` wide: the sound held a step
/// for Spectrum, the trace moving on with the processor's `load` (0 to 100) for Heartbeat.
pub fn live(fx: Fx, speed: u8, load: f64, cols: usize) -> Live {
    Live {
        sound: if fx == Fx::Spectrum { crate::sound::held(period(speed)).to_vec() } else { Vec::new() },
        pulse: if fx == Fx::Heartbeat { pulse(period(speed), load, cols) } else { Vec::new() },
    }
}

/// The whole screen at moment `t`, `size` its columns and rows: `word` is the one Bounce
/// bounces, and `live` what Spectrum and Heartbeat draw from. A screen of no cells draws none.
pub fn frame(fx: Fx, t: f64, word: &str, live: &Live, (w, h): Size) -> Grid {
    let t = t.max(0.0);
    // (the games and the fireworks count whole steps)
    let k = t.round() as u64;
    let mut g = vec![vec![' '; w]; h];
    if w == 0 || h == 0 {
        return g;
    }
    let (wf, hf) = (w as f64, h as f64);
    match fx {
        Fx::Rain => {
            // (each drop falls a third of a row at a time, splashes on the foot row, and is
            // gone a while before it falls again; two to a column on four rows, as many more
            // as a taller screen has rows for)
            let (fall, drops) = (3 * h, (h / 2).max(1));
            for c in 0..w {
                for k in 0..drops {
                    let (cf, kf) = (c as f64, k as f64);
                    let sp = 0.9 + rnd(cf * 7.0 + kf) * 0.8;
                    let y = ((t * sp + rnd(cf + kf * 31.0) * (10 * h) as f64) % (fall + 4) as f64).floor() as usize;
                    if y < fall {
                        g[y / 3][c] = SUB[y % 3];
                    } else if y == fall {
                        g[h - 1][c] = ',';
                    }
                }
            }
        }
        Fx::Snow => {
            // (a flake for every five cells)
            for i in 0..(w * h).div_ceil(5) {
                let fi = i as f64;
                let y = ((t * 0.35 + rnd(fi) * (10 * h) as f64) % (3 * h + 2) as f64).floor() as usize;
                let x = ((rnd(fi * 3.0) * wf + (t / 12.0 + fi).sin() * 1.5 + wf) % wf).floor() as usize;
                if y < 3 * h {
                    g[y / 3][x.min(w - 1)] = ['°', '*', '.'][y % 3];
                }
            }
            // (the snow on the ground)
            for c in 0..w {
                if rnd(c as f64 * 5.0) > 0.55 {
                    g[h - 1][c] = '_';
                }
            }
        }
        Fx::Stars => {
            // (fourteen stars on the mock's eighty cells, as many to a cell on any screen)
            for i in 0..(w * h * 14).div_ceil(80) {
                let fi = i as f64;
                let sp = 0.3 + rnd(fi) * 1.4;
                let x = ((t * sp + rnd(fi * 9.0) * wf) % (wf + 2.0)).floor() as i32 - 1;
                let r = ((rnd(fi * 4.0) * hf).floor() as usize).min(h - 1);
                let tail = if sp > 1.1 { '=' } else if sp > 0.7 { '-' } else { '.' };
                if (0..w as i32).contains(&x) {
                    g[r][x as usize] = if sp > 1.1 { '*' } else { '.' };
                }
                if (0..w as i32).contains(&(x - 1)) && sp > 0.7 {
                    g[r][(x - 1) as usize] = tail;
                }
            }
        }
        Fx::Code => {
            let ch: Vec<char> = "01ABZ<>*+=$#%&?".chars().collect();
            // (each column's head runs down the screen and five rows past it, its trail three long)
            let cycle = (h + 5) as f64;
            for c in 0..w {
                let cf = c as f64;
                let sp = 0.25 + rnd(cf) * 0.5;
                let head = ((t * sp + rnd(cf * 3.0) * (3 * h) as f64) % cycle).floor() as i32;
                for r in 0..h as i32 {
                    if r <= head && head - r < 3 {
                        let k = (rnd(cf * 17.0 + r as f64 + (t / 2.0).floor()) * ch.len() as f64).floor() as usize;
                        g[r as usize][c] = ch[k.min(ch.len() - 1)];
                    }
                }
            }
        }
        Fx::Fire => {
            // (the flames as high, for the screen's height, as on the Z10's four rows)
            let (top, lift) = ((h - 1) as f64, hf / 4.0);
            for c in 0..w {
                for r in 0..h {
                    let (cf, rf) = (c as f64, r as f64);
                    let fh = (3.4 + (cf * 0.9 + t / 4.0).sin() * 0.8 + (cf * 2.3 - t / 2.5).sin() * 0.6 + (rnd(cf + (t / 1.5).floor() * 20.0) - 0.5)) * lift;
                    let v = (fh - (top - rf)) / 1.4;
                    g[r][c] = if v > 1.6 { FULL } else { RAMP[(v * 6.0).floor().clamp(0.0, 9.0) as usize] };
                }
            }
        }
        Fx::Plasma => {
            for r in 0..h {
                for c in 0..w {
                    let (cf, rf) = (c as f64, r as f64);
                    let v = (cf / 3.0 + t / 9.0).sin() + (rf / 1.5 + t / 7.0).sin() + ((cf + rf * 2.0) / 4.0 + t / 11.0).sin();
                    g[r][c] = RAMP[(((v + 3.0) / 6.0) * 9.99).floor().clamp(0.0, 9.0) as usize];
                }
            }
        }
        Fx::Fireworks => fireworks(k, &mut g),
        Fx::Heartbeat => {
            // (each column of the trace, its top and its foot: the line, the stroke up, the
            // peak's two sides, the stroke down, and the dip after; in the screen's middle two
            // rows, and on a screen of one row, its top where it has one and its foot elsewhere)
            const LOOKS: [(char, char); 7] = [(' ', '_'), (' ', '/'), ('/', ' '), ('\\', ' '), (' ', '\\'), (' ', '/'), (' ', '\\')];
            let top = h.saturating_sub(2) / 2;
            for c in 0..w {
                let (up, foot) = LOOKS[(traced(&live.pulse, c, w) as usize).min(6)];
                if h == 1 {
                    g[0][c] = if up != ' ' { up } else { foot };
                } else {
                    (g[top][c], g[top + 1][c]) = (up, foot);
                }
            }
        }
        Fx::Tetris => played(&TETRIS, k, (w, h), Tetris::new, Tetris::step).draw(&mut g),
        Fx::Snake => played(&SNAKE, k, (w, h), Snake::new, Snake::step).draw(&mut g),
        Fx::Highway => highway(k, &mut g),
        Fx::Invaders => played(&INVADERS, k, (w, h), Invaders::new, Invaders::step).draw(&mut g),
        Fx::Spectrum => {
            // (the sound's bands spread across the columns, and its levels over the rows, three
            // steps a row)
            let n = live.sound.len();
            for c in 0..w {
                let lv = live.sound.get(c * n / w).copied().unwrap_or(0.0).max(0.0) as f64 * (3 * h) as f64 / LOUDEST;
                for r in 0..h {
                    let k = lv - (h - 1 - r) as f64 * 3.0;
                    g[r][c] = if k >= 3.0 {
                        FULL
                    } else if k >= 2.0 {
                        '='
                    } else if k >= 1.0 {
                        '_'
                    } else {
                        ' '
                    };
                }
            }
        }
        Fx::Aquarium => {
            let put = |g: &mut Grid, r: usize, x: i32, s: &str| {
                for (i, ch) in s.chars().enumerate() {
                    let c = x + i as i32;
                    if (0..w as i32).contains(&c) {
                        g[r][c as usize] = ch;
                    }
                }
            };
            // (the water over the bed, on a screen of two rows or more: a fish in each of its
            // rows, the big one at the top, the small ones swimming either way under it, the
            // three again lower down on a taller screen, a little later each time)
            let bed = (h >= 2).then_some(h - 1);
            for r in 0..bed.unwrap_or(h) {
                let later = (r / 3) as f64 * 11.0;
                match r % 3 {
                    0 => put(&mut g, r, ((t / 6.0 + 14.0 + later) % (wf + 10.0)).floor() as i32 - 5, "><(((°>"),
                    1 => put(&mut g, r, ((t / 3.0 + later) % (wf + 8.0)).floor() as i32 - 4, "><>"),
                    _ => put(&mut g, r, w as i32 - ((t / 4.5 + 9.0 + later) % (wf + 8.0)).floor() as i32 + 4, "<><"),
                }
            }
            // (bubbles rising, one every six columns)
            for i in 0..(w + 1) / 6 {
                let y = (3 * h) as i32 - 1 - ((t * 0.5 + i as f64 * 4.0) % (3 * h) as f64).floor() as i32;
                let x = 4 + i * 6;
                g[(y / 3).clamp(0, h as i32 - 1) as usize][x] = if y % 3 == 2 { '.' } else { 'o' };
            }
            // (the bed, the mock's twenty cells of it again and again)
            if let Some(bed) = bed {
                let sand: Vec<char> = BED.chars().collect();
                for c in 0..w {
                    g[bed][c] = sand[c % sand.len()];
                }
            }
        }
        Fx::Pong => {
            // (the ball between the bats, across and down; the score either side of the net, in
            // the screen's middle)
            let span = (wf - 2.0).max(1.0);
            let x = 1.0 + (((t / 1.2) % (2.0 * span)) - span).abs();
            let rise = (3 * h) as f64 - 1.0;
            let y = (((t / 1.6) % (2.0 * rise)) - rise).abs();
            let r = ((y / 3.0).floor() as usize).min(h - 1);
            g[r][(x.round() as usize).min(w - 1)] = 'o';
            let lp = (r as i32 + if (t / 5.0).sin() > 0.0 { 0 } else { -1 }).clamp(0, h as i32 - 1) as usize;
            g[lp][0] = '|';
            g[r][w - 1] = '|';
            let net = w / 2;
            put(&mut g, 0, net as i32 - 2, '3');
            put(&mut g, 0, net as i32 + 1, '2');
            for row in g.iter_mut() {
                if row[net] == ' ' {
                    row[net] = ':';
                }
            }
        }
        Fx::Bounce => {
            let word: Vec<char> = if word.is_empty() { vec![' '] } else { word.chars().take(w).collect() };
            let n = word.len();
            let span = (w - n).max(1) as f64;
            let x = if n >= w { 0.0 } else { (((t / 2.0) % (2.0 * span)) - span).abs() };
            let fall = (h - 1) as f64;
            let y = if fall > 0.0 { (((t / 5.0) % (2.0 * fall)) - fall).abs() } else { 0.0 };
            let (row, at) = ((y.round() as usize).min(h - 1), x.round() as usize);
            for (i, ch) in word.iter().enumerate() {
                if at + i < w {
                    g[row][at + i] = *ch;
                }
            }
        }
    }
    g
}

/// The aquarium's bed, as the mock drew it across its screen.
const BED: &str = ")  (  )     (   ) ( ";

/// What column `c` of a screen `w` wide shows of the heartbeat's `trace`, the trace's newest
/// column at the screen's right: the trace flat where it is shorter than the screen.
fn traced(trace: &[u8], c: usize, w: usize) -> u8 {
    (trace.len() + c).checked_sub(w).and_then(|i| trace.get(i)).copied().unwrap_or(0)
}

/// A cell set, where the screen has it.
fn put(g: &mut Grid, r: i32, c: i32, ch: char) {
    if let Some(cell) = usize::try_from(r).ok().and_then(|r| g.get_mut(r)).and_then(|row| usize::try_from(c).ok().and_then(|c| row.get_mut(c))) {
        *cell = ch;
    }
}

/// Fireworks at step `k`: a rocket goes up every two steps or so at a column of its own (one
/// for every twenty columns), up the screen to the row under its burst, a row a step, bursts and
/// holds its burst for two more, throws its sparks wide for one, and is gone; two or three in
/// the sky at once. On the Z10's four rows each bursts on the second row; on a taller screen,
/// anywhere from there down.
fn fireworks(k: u64, g: &mut Grid) {
    let (w, h) = (g[0].len() as i32, g.len() as i32);
    // (a rocket lives as many steps as the screen has rows and one more, at most)
    let look = (h as u64 + 4) / 2;
    for stream in 0..(w / 20).max(1) {
        for j in (k / 2).saturating_sub(look)..=k / 2 {
            let seed = j as f64 + stream as f64 * 1000.0;
            let start = 2 * j + (rnd(seed * 1.3 + 0.2) * 2.0) as u64;
            if start > k {
                continue;
            }
            let x = 2 + (rnd(seed * 2.9 + 0.7) * (w - 4).max(0) as f64) as i32;
            let spark = ['*', '+', 'o'][(rnd(seed * 4.1 + 0.9) * 3.0) as usize % 3];
            let mid = 1 + (rnd(seed * 5.3 + 0.4) * (h - 3).max(0) as f64) as i32;
            let climb = (h - 1 - mid).max(0) as u64;
            match k - start {
                d if d < climb => put(g, h - 1 - d as i32, x, '|'),
                d if d < climb + 2 => {
                    for (r, row) in [['\\', '|', '/'], ['-', spark, '-'], ['/', '|', '\\']].iter().enumerate() {
                        for (i, ch) in row.iter().enumerate() {
                            put(g, mid - 1 + r as i32, x - 1 + i as i32, *ch);
                        }
                    }
                }
                d if d == climb + 2 => {
                    for (r, cols) in [(-1, &[-2, 0, 2][..]), (0, &[-2, 2][..]), (1, &[-2, 0, 2][..])] {
                        for dc in cols {
                            put(g, mid + r, x + dc, '.');
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Highway at step `k`: lanes between edges dashing past, the traffic coming on at the right a
/// cell a step, and the car (=>) in a lane the next car ahead isn't in.
fn highway(k: u64, g: &mut Grid) {
    let (w, h) = (g[0].len(), g.len());
    let (first, lanes) = road(h);
    for c in 0..w {
        if (c as u64 + k) % 2 == 0 {
            if first > 0 {
                g[0][c] = '-';
            }
            if first + lanes < h {
                g[h - 1][c] = '-';
            }
        }
    }
    for (lane, c) in traffic(k, (w, h)) {
        if (0..w as i64).contains(&c) {
            g[lane][c as usize] = FULL;
        }
    }
    let mine = car_lane(k, (w, h)) as i32;
    put(g, mine, 1, '=');
    put(g, mine, 2, '>');
}

/// The highway's lanes on a screen `h` rows tall: the first lane's row and how many there are.
/// The Z10's four rows have two lanes between the edges; a taller screen has more lanes, and a
/// shorter one leaves out its foot's edge, then its top's, keeping two lanes while it can.
fn road(h: usize) -> (usize, usize) {
    match h {
        0 | 1 => (0, 1),
        2 => (0, 2),
        3 => (1, 2),
        _ => (1, h - 2),
    }
}

/// The traffic on the highway at step `k`, on a screen of `size`: each car's lane (its row) and
/// column, the ones still to come past the right end. Car j comes on at step 4j or one later,
/// so three cells or more after the one before, and the car always has room to go round.
fn traffic(k: u64, (w, h): Size) -> impl Iterator<Item = (usize, i64)> {
    let (first, lanes) = road(h);
    let on = |j: u64| 4 * j + (rnd(j as f64 * 1.7 + 0.3) * 2.0) as u64;
    // (each lane alike: the lanes the throw is past a share of)
    let lane = move |j: u64| first + (1..lanes).filter(|&i| rnd(j as f64 * 2.3 + 0.5) > i as f64 / lanes as f64).count();
    // (as far back as the cars still on the screen came on)
    let back = (w / 4 + 1) as u64;
    ((k / 4).saturating_sub(back)..=k / 4 + 2).map(move |j| (lane(j), w as i64 - 1 - (k as i64 - on(j) as i64)))
}

/// The lane the car (in the second and third columns) is in at step `k`, on a screen of
/// `size`: the first lane, or the second where the next car still ahead of its tail is in the
/// first.
fn car_lane(k: u64, size: Size) -> usize {
    let (first, lanes) = road(size.1);
    traffic(k, size).filter(|(_, c)| *c >= 1).min_by_key(|(_, c)| *c).map_or(first, |(lane, _)| if lane == first && lanes > 1 { first + 1 } else { first })
}

// ------------------------------------------------------------------ the games

/// A game's dice: the same seed, the same throws, so a moment always looks the same.
#[derive(Clone)]
struct Dice(u64);

impl Dice {
    fn new(seed: u64) -> Dice {
        Dice(seed.wrapping_add(1).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// The most steps a round of a game runs before it starts afresh (sooner once it is lost): a
/// moment is played out from its round's start, never from the very beginning.
const ROUND: u64 = 600;

/// A game as this thread last played it: its round, its step in the round, the screen it was
/// played on, and the game.
type Kept<G> = RefCell<Option<(u64, u64, Size, G)>>;

thread_local! {
    static TETRIS: Kept<Tetris> = const { RefCell::new(None) };
    static SNAKE: Kept<Snake> = const { RefCell::new(None) };
    static INVADERS: Kept<Invaders> = const { RefCell::new(None) };
}

/// A game played to step `k` on a screen of `size`: carried on from where this thread last
/// played it, when that is earlier in the same round on a screen the same size, else from the
/// round's start; a lost game starts again.
fn played<G: Clone>(cache: &'static std::thread::LocalKey<Kept<G>>, k: u64, size: Size, new: impl Fn(u64, Size) -> G, step: impl Fn(&mut G) -> bool) -> G {
    let (round, at) = (k / ROUND, k % ROUND);
    cache.with(|c| {
        let mut c = c.borrow_mut();
        let (mut i, mut g) = match c.take() {
            Some((r, i, s, g)) if r == round && i <= at && s == size => (i, g),
            _ => (0, new(round * 1000, size)),
        };
        while i < at {
            if !step(&mut g) {
                g = new(round * 1000 + i + 1, size);
            }
            i += 1;
        }
        *c = Some((round, i, size, g.clone()));
        g
    })
}

type Shape = [(i32, i32); 4];
/// Which of a screen's cells are taken, row by row.
type Board = Vec<Vec<bool>>;

/// The seven pieces, each its four cells across and down from its corner.
const PIECES: [Shape; 7] = [
    [(0, 0), (1, 0), (2, 0), (3, 0)],
    [(0, 0), (1, 0), (0, 1), (1, 1)],
    [(0, 0), (1, 0), (2, 0), (1, 1)],
    [(1, 0), (2, 0), (0, 1), (1, 1)],
    [(0, 0), (1, 0), (1, 1), (2, 1)],
    [(0, 0), (0, 1), (1, 1), (2, 1)],
    [(2, 0), (0, 1), (1, 1), (2, 1)],
];

/// A piece's turns: a quarter turn at a time, each set at its corner, the same one once.
fn turns(kind: usize) -> Vec<Shape> {
    let mut out: Vec<Shape> = Vec::new();
    let mut p = PIECES[kind];
    for _ in 0..4 {
        let (mx, my) = (p.iter().map(|c| c.0).min().unwrap_or(0), p.iter().map(|c| c.1).min().unwrap_or(0));
        let mut n = p.map(|(x, y)| (x - mx, y - my));
        n.sort();
        if !out.contains(&n) {
            out.push(n);
        }
        p = p.map(|(x, y)| (-y, x));
    }
    out
}

/// How good a board is to play on: its rows filled, against its height, its holes and how
/// uneven it is (the weights of a well-known simple Tetris player).
fn judge(b: &Board, lines: usize) -> f64 {
    let (w, h) = (b[0].len(), b.len());
    let heights: Vec<i32> = (0..w).map(|c| (0..h).find(|&r| b[r][c]).map_or(0, |r| (h - r) as i32)).collect();
    let holes: usize = (0..w).map(|c| (0..h).find(|&r| b[r][c]).map_or(0, |top| (top..h).filter(|&r| !b[r][c]).count())).sum();
    let bumps: i32 = heights.windows(2).map(|w| (w[0] - w[1]).abs()).sum();
    0.76 * lines as f64 - 0.51 * heights.iter().sum::<i32>() as f64 - 0.36 * holes as f64 - 0.18 * bumps as f64
}

/// Tetris on the screen, a well as wide and as deep as the screen: each piece comes in over the
/// column its player picks and falls a row a step; full rows show as a line for a step, then go.
#[derive(Clone)]
struct Tetris {
    board: Board,
    piece: Option<(Shape, i32, i32)>,
    full: Option<Vec<bool>>,
    bag: Vec<usize>,
    dice: Dice,
}

impl Tetris {
    fn new(seed: u64, (w, h): Size) -> Tetris {
        Tetris { board: vec![vec![false; w]; h], piece: None, full: None, bag: Vec::new(), dice: Dice::new(seed) }
    }

    fn fits(board: &Board, s: &Shape, x: i32, y: i32) -> bool {
        let (w, h) = (board[0].len() as i32, board.len() as i32);
        s.iter().all(|&(cx, cy)| {
            let (px, py) = (x + cx, y + cy);
            (0..w).contains(&px) && py < h && (py < 0 || !board[py as usize][px as usize])
        })
    }

    /// The next piece, from a bag of all seven in a thrown order, as Tetris deals them.
    fn deal(&mut self) -> usize {
        if self.bag.is_empty() {
            self.bag = (0..7).collect();
            for i in (1..7).rev() {
                let j = self.dice.below(i as u64 + 1) as usize;
                self.bag.swap(i, j);
            }
        }
        self.bag.pop().unwrap_or(0)
    }

    /// Where the player puts a piece: every turn over every column, dropped straight down,
    /// the board after it judged (see [judge]).
    fn choose(&self, kind: usize) -> (Shape, i32) {
        let mut best: Option<(f64, Shape, i32)> = None;
        for s in turns(kind) {
            let w = s.iter().map(|c| c.0).max().unwrap_or(0) + 1;
            let h = s.iter().map(|c| c.1).max().unwrap_or(0) + 1;
            for x in 0..=(self.board[0].len() as i32 - w) {
                let mut y = -h;
                while Tetris::fits(&self.board, &s, x, y + 1) {
                    y += 1;
                }
                let mut b = self.board.clone();
                let mut over = false;
                for &(cx, cy) in &s {
                    if y + cy < 0 {
                        over = true;
                    } else {
                        b[(y + cy) as usize][(x + cx) as usize] = true;
                    }
                }
                let lines = b.iter().filter(|r| r.iter().all(|c| *c)).count();
                let score = if over { -1000.0 } else { judge(&b, lines) };
                if best.as_ref().is_none_or(|(sc, ..)| score > *sc) {
                    best = Some((score, s, x));
                }
            }
        }
        best.map_or((PIECES[kind], 0), |(_, s, x)| (s, x))
    }

    /// A step: the full rows gone, or the piece a row lower, or locked where it lies, or a new
    /// one coming in. False once the stack is over the top.
    fn step(&mut self) -> bool {
        let (w, h) = (self.board[0].len(), self.board.len());
        if let Some(full) = self.full.take() {
            let kept: Vec<Vec<bool>> = (0..h).filter(|r| !full[*r]).map(|r| self.board[r].clone()).collect();
            let mut b = vec![vec![false; w]; h];
            for (i, row) in kept.into_iter().rev().enumerate() {
                b[h - 1 - i] = row;
            }
            self.board = b;
            return true;
        }
        match self.piece {
            None => {
                let kind = self.deal();
                let (s, x) = self.choose(kind);
                // (its foot row on the screen's top row)
                let y = -s.iter().map(|c| c.1).max().unwrap_or(0);
                if !Tetris::fits(&self.board, &s, x, y) {
                    return false;
                }
                self.piece = Some((s, x, y));
            }
            Some((s, x, y)) if Tetris::fits(&self.board, &s, x, y + 1) => self.piece = Some((s, x, y + 1)),
            Some((s, x, y)) => {
                self.piece = None;
                for &(cx, cy) in &s {
                    if y + cy < 0 {
                        return false;
                    }
                    self.board[(y + cy) as usize][(x + cx) as usize] = true;
                }
                let full: Vec<bool> = self.board.iter().map(|r| r.iter().all(|c| *c)).collect();
                if full.iter().any(|f| *f) {
                    self.full = Some(full);
                }
            }
        }
        true
    }

    fn draw(&self, g: &mut Grid) {
        for (r, row) in self.board.iter().enumerate() {
            for (c, taken) in row.iter().enumerate() {
                if *taken {
                    g[r][c] = if self.full.as_ref().is_some_and(|f| f[r]) { '=' } else { FULL };
                }
            }
        }
        if let Some((s, x, y)) = self.piece {
            for &(cx, cy) in &s {
                put(g, y + cy, x + cx, FULL);
            }
        }
    }
}

/// Snake on the screen: it heads for its food the shortest way that is free, turning off where
/// it would run into itself or into a dead end, and grows a cell with each bite.
#[derive(Clone)]
struct Snake {
    body: std::collections::VecDeque<(i32, i32)>,
    dir: (i32, i32),
    food: (i32, i32),
    /// the screen it plays on, its columns and rows
    size: (i32, i32),
    dice: Dice,
}

impl Snake {
    /// A new game: the snake four cells long on the second row, its head in the sixth column,
    /// heading right (shorter, and nearer the corner, on a screen too small for that).
    fn new(seed: u64, (w, h): Size) -> Snake {
        let (w, h) = (w as i32, h as i32);
        let (head, row) = (5.min(w - 1), 1.min(h - 1));
        let body = (0..4.min(w)).map(|i| (head - i, row)).collect();
        let mut s = Snake { body, dir: (1, 0), food: (0, 0), size: (w, h), dice: Dice::new(seed) };
        s.feed();
        s
    }

    /// Food somewhere the snake isn't; false when there's nowhere left.
    fn feed(&mut self) -> bool {
        let (w, h) = self.size;
        let free: Vec<(i32, i32)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|c| !self.body.contains(c)).collect();
        if free.is_empty() {
            return false;
        }
        self.food = free[self.dice.below(free.len() as u64) as usize];
        true
    }

    /// Whether the snake may go into cell `c`: on the screen, and not itself (its tail cell
    /// moves on as it goes, unless it is `eating`).
    fn free(&self, c: (i32, i32), eating: bool) -> bool {
        let body = self.body.len().saturating_sub(usize::from(!eating));
        (0..self.size.0).contains(&c.0) && (0..self.size.1).contains(&c.1) && !self.body.iter().take(body).any(|b| *b == c)
    }

    /// A step: a cell on, straight or turning. False when every way is blocked.
    fn step(&mut self) -> bool {
        let Some(&head) = self.body.front() else { return false };
        let (dx, dy) = self.dir;
        let mut best: Option<((i32, i32), i32)> = None;
        for d in [(dx, dy), (-dy, dx), (dy, -dx)] {
            let next = (head.0 + d.0, head.1 + d.1);
            let eating = next == self.food;
            if !self.free(next, eating) {
                continue;
            }
            // (the ways on from there, so it doesn't turn into a dead end)
            let room = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().filter(|o| (next.0 + o.0, next.1 + o.1) != head && self.free((next.0 + o.0, next.1 + o.1), false)).count() as i32;
            let cost = ((next.0 - self.food.0).abs() + (next.1 - self.food.1).abs()) * 4 - room.min(2) * 3 + if room == 0 && !eating { 100 } else { 0 };
            if best.is_none_or(|(_, c)| cost < c) {
                best = Some((d, cost));
            }
        }
        let Some((d, _)) = best else { return false };
        self.dir = d;
        let next = (head.0 + d.0, head.1 + d.1);
        self.body.push_front(next);
        if next == self.food {
            return self.feed();
        }
        self.body.pop_back();
        true
    }

    fn draw(&self, g: &mut Grid) {
        put(g, self.food.1, self.food.0, '*');
        for (i, &(x, y)) in self.body.iter().enumerate() {
            put(g, y, x, if i == 0 { '@' } else { 'o' });
        }
    }
}

/// Space Invaders on the screen: two rows of invaders a column apart (six on the Z10's twenty
/// columns, fewer on a narrower screen and six more for every twenty more columns) march from
/// side to side together, a row down at each end and quicker the fewer are left, dropping a
/// bomb now and then; the cannon on the bottom row goes to the nearest, fires one shot at a time
/// and keeps out from under the bombs. A wave shot down, the next marches in; a wave that lands,
/// or the cannon hit three times, and the game starts again. (Every place is a row and a column
/// on the screen.)
#[derive(Clone)]
struct Invaders {
    /// the screen it plays on, its columns, and its bottom row, the cannon's
    w: i32,
    foot: i32,
    /// the invaders standing, two rows of them a column apart, and where the first one's place is
    alive: [Vec<bool>; 2],
    y: i32,
    x: i32,
    dir: i32,
    /// steps until the invaders march again, and how often they have (they look one way, then
    /// the other, at each march)
    wait: u64,
    marched: u64,
    /// the cannon's shot, and the steps before it can fire again once its shot is gone
    shot: Option<(i32, i32)>,
    cool: u8,
    bombs: Vec<(i32, i32)>,
    /// an invader or the cannon hit, shown a step
    boom: Option<(i32, i32)>,
    cannon: i32,
    hits: u8,
    /// steps the cannon stays gone after a hit, the sky stays empty after a wave, and a lost
    /// game stays on the screen
    gone: u8,
    rest: u8,
    over: u8,
    dice: Dice,
}

impl Invaders {
    fn new(seed: u64, (w, h): Size) -> Invaders {
        let n = (w.saturating_sub(8) / 2 + 1).min(6 * (w / 20).max(1));
        let (w, foot) = (w as i32, h as i32 - 1);
        let alive = [vec![true; n], vec![true; n]];
        let mut g = Invaders { w, foot, alive, y: 0, x: 0, dir: 1, wait: 0, marched: 0, shot: None, cool: 0, bombs: Vec::new(), boom: None, cannon: w / 2, hits: 0, gone: 0, rest: 0, over: 0, dice: Dice::new(seed) };
        g.wave();
        g
    }

    /// How many invaders a row has.
    fn per_row(&self) -> usize {
        self.alive[0].len()
    }

    /// A new wave across the top, starting somewhere along it, marching the longer way first.
    fn wave(&mut self) {
        let n = self.per_row();
        let places = (self.w + 1 - 2 * n as i32).max(1);
        let x = self.dice.below(places as u64) as i32;
        (self.alive, self.y, self.x, self.dir, self.wait) = ([vec![true; n], vec![true; n]], 0, x, if x > (places - 1) / 2 { -1 } else { 1 }, 0);
    }

    /// Where each invader standing is.
    fn standing(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        (0..2).flat_map(move |r| (0..self.per_row()).filter(move |&i| self.alive[r][i]).map(move |i| (self.y + r as i32, self.x + 2 * i as i32)))
    }

    /// Where each invader will be when a shot fired now reaches its row (none on the cannon's
    /// row): the shot climbs a row a step and the invaders march every so often, so the cannon
    /// fires ahead of them (counting no turn at the end; there a shot may miss).
    fn ahead(&self) -> Vec<(i32, i32)> {
        let every = (self.standing().count() as i64 / 4).max(1) + 1;
        let marches = |d: i64| if (self.wait as i64) >= d { 0 } else { 1 + (d - 1 - self.wait as i64) / every };
        self.standing().filter(|(r, _)| *r < self.foot).map(|(r, c)| (r, c + self.dir * marches((self.foot - 1 - r) as i64) as i32)).collect()
    }

    /// The invader at `at` shot down, if one stands there.
    fn hit(&mut self, at: (i32, i32)) -> bool {
        let (r, d) = (at.0 - self.y, at.1 - self.x);
        if !(0..2).contains(&r) || !(0..2 * self.per_row() as i32).contains(&d) || d % 2 != 0 || !self.alive[r as usize][(d / 2) as usize] {
            return false;
        }
        self.alive[r as usize][(d / 2) as usize] = false;
        self.boom = Some(at);
        true
    }

    /// The game lost: the cannon blown up, and the screen as it is for a few steps.
    fn lose(&mut self) {
        (self.boom, self.gone, self.over, self.shot) = (Some((self.foot, self.cannon)), u8::MAX, 4, None);
        self.bombs.clear();
    }

    /// A step. False once a lost game has had its few steps on the screen: the invaders landed,
    /// or the cannon was hit three times.
    fn step(&mut self) -> bool {
        let (w, foot) = (self.w, self.foot);
        if self.over > 0 {
            self.over -= 1;
            self.boom = None;
            return self.over > 0;
        }
        self.boom = None;
        if self.rest > 0 {
            self.rest -= 1;
            if self.rest == 0 {
                self.wave();
            }
            return true;
        }
        // the cannon: out of the way of a bomb about to land, else a column toward where the
        // nearest invader of the front row will be (the row that would land first)
        let ahead = self.ahead();
        if self.gone > 0 {
            self.gone -= 1;
        } else {
            let front = ahead.iter().map(|(r, _)| *r).max().unwrap_or(0);
            let near = ahead.iter().filter(|(r, c)| *r == front && (0..w).contains(c)).map(|(_, c)| *c).min_by_key(|c| (c - self.cannon).abs());
            let want = near.map_or(0, |c| (c - self.cannon).signum());
            let safe = |c: i32| (0..w).contains(&c) && !self.bombs.contains(&(foot - 1, c));
            if let Some(d) = [want, 0, -want, 1, -1].into_iter().find(|d| safe(self.cannon + d)) {
                self.cannon += d;
            }
        }
        // its shot a row up, into an invader or off the top; a new one a step or three after
        // it's gone, with an invader coming over the cannon
        if let Some((r, c)) = self.shot.take() {
            if r > 0 && !self.hit((r - 1, c)) {
                self.shot = Some((r - 1, c));
            } else {
                self.cool = 1 + self.dice.below(3) as u8;
            }
        } else if self.cool > 0 {
            self.cool -= 1;
        } else if self.gone == 0 && ahead.iter().any(|(_, c)| *c == self.cannon) {
            if self.hit((foot - 1, self.cannon)) {
                self.cool = 1 + self.dice.below(3) as u8;
            } else {
                self.shot = Some((foot - 1, self.cannon));
            }
        }
        // the bombs a row down, one landing on the cannon
        let mut falling = Vec::new();
        for (r, c) in std::mem::take(&mut self.bombs) {
            if r + 1 == foot && c == self.cannon && self.gone == 0 {
                self.boom = Some((foot, c));
                (self.hits, self.gone) = (self.hits + 1, 3);
                if self.hits >= 3 {
                    self.lose();
                    return true;
                }
            } else if r < foot {
                falling.push((r + 1, c));
            }
        }
        self.bombs = falling;
        // the invaders march: along, or a row down at the end (landing, the game is lost);
        // with twelve standing, every fourth step, and every other step with seven or fewer
        if self.wait > 0 {
            self.wait -= 1;
        } else if self.standing().next().is_some() {
            let cols: Vec<i32> = self.standing().map(|(_, c)| c).collect();
            let (lo, hi) = (cols.iter().min().copied().unwrap_or(0), cols.iter().max().copied().unwrap_or(0));
            if (self.dir > 0 && hi + 1 >= w) || (self.dir < 0 && lo == 0) {
                (self.dir, self.y) = (-self.dir, self.y + 1);
                if self.standing().any(|(r, _)| r >= foot) {
                    self.lose();
                    return true;
                }
            } else {
                self.x += self.dir;
            }
            self.marched += 1;
            self.wait = (cols.len() as u64 / 4).max(1);
            if let Some(s) = self.shot {
                if self.hit(s) {
                    (self.shot, self.cool) = (None, 1 + self.dice.below(3) as u8);
                }
            }
        }
        // now and then a bomb from the lowest invader of a column, two at most at a time (none
        // from right over the cannon's row, too near to get out of the way)
        if self.bombs.len() < 2 && self.dice.below(4) == 0 {
            let lows: Vec<(i32, i32)> = (0..self.per_row()).filter_map(|i| (0..2).rev().find(|&r| self.alive[r][i]).map(|r| (self.y + r as i32, self.x + 2 * i as i32))).filter(|(r, _)| *r < foot - 1).collect();
            if !lows.is_empty() {
                let (r, c) = lows[self.dice.below(lows.len() as u64) as usize];
                self.bombs.push((r + 1, c));
            }
        }
        // the wave shot down: the sky empty a few steps, then the next
        if self.standing().next().is_none() {
            (self.rest, self.shot) = (4, None);
            self.bombs.clear();
        }
        true
    }

    fn draw(&self, g: &mut Grid) {
        // (the back row and the front, each in its two looks)
        const LOOKS: [[char; 2]; 2] = [['Y', 'T'], ['M', 'W']];
        for (r, i) in (0..2).flat_map(|r| (0..self.per_row()).map(move |i| (r, i))) {
            if self.alive[r][i] {
                put(g, self.y + r as i32, self.x + 2 * i as i32, LOOKS[r][(self.marched % 2) as usize]);
            }
        }
        for &(r, c) in &self.bombs {
            put(g, r, c, '!');
        }
        if let Some((r, c)) = self.shot {
            put(g, r, c, '|');
        }
        if self.gone == 0 {
            put(g, self.foot, self.cannon, 'A');
        }
        if let Some((r, c)) = self.boom {
            put(g, r, c, '*');
        }
    }
}

// ------------------------------------------------------------------ the heartbeat

/// The heartbeat as the tiles show it standing still, and as it starts: a beat every ten
/// columns, two on the Z10's twenty.
const BEAT: [u8; 10] = [0, 0, 0, 1, 2, 3, 4, 5, 6, 0];

/// The tiles' heartbeat across a screen `cols` wide.
fn sample_pulse(cols: usize) -> Vec<u8> {
    (0..cols).map(|c| BEAT[c % BEAT.len()]).collect()
}

/// The trace, as it is drawn on: its columns, the beat being drawn (its next column, 0 for
/// none), the columns since the last beat began, and when it last moved on.
struct Pulse {
    cols: Vec<u8>,
    beat: u8,
    since: u32,
    at: Option<Instant>,
}

impl Pulse {
    /// The trace as it starts on a screen `cols` wide: the tiles' beats, and the trace carrying
    /// on from them, a beat begun at the right end finished first.
    fn new(cols: usize) -> Pulse {
        let trace = sample_pulse(cols);
        let last = trace.last().copied().unwrap_or(0);
        let beat = if (1..6).contains(&last) { last + 1 } else { 0 };
        // (a beat begins at the fourth of every ten columns)
        let since = ((cols + BEAT.len() - 4) % BEAT.len()) as u32;
        Pulse { cols: trace, beat, since, at: None }
    }

    /// The next column: the beat drawn on, or the line, or a new beat when one is due, one
    /// every fourteen columns at rest and every seven with the processor flat out (`load`).
    fn next(&mut self, load: f64) -> u8 {
        self.since += 1;
        if self.beat > 0 {
            let c = self.beat;
            self.beat = if c >= 6 { 0 } else { c + 1 };
            return c;
        }
        let every = (14.0 - 7.0 * (load / 100.0).clamp(0.0, 1.0)).round() as u32;
        if self.since >= every {
            (self.since, self.beat) = (0, 2);
            return 1;
        }
        0
    }
}

/// The heartbeat's trace now, as many of its last columns as a screen `cols` wide shows, the
/// newest at the right: a column more every `period` seconds, beating as fast as the
/// processor's `load` (0 to 100) says. Every screen showing a heartbeat draws this one trace,
/// which moves on with the clock (one wider than it has been drawn for shows it flat before).
pub fn pulse(period: f64, load: f64, cols: usize) -> Vec<u8> {
    static PULSE: OnceLock<Mutex<Pulse>> = OnceLock::new();
    let mut p = PULSE.get_or_init(|| Mutex::new(Pulse::new(cols))).lock().unwrap();
    if p.cols.len() < cols {
        let more = cols - p.cols.len();
        p.cols.splice(0..0, std::iter::repeat_n(0, more));
    }
    let now = Instant::now();
    if let Some(at) = p.at {
        let n = (now.duration_since(at).as_secs_f64() / period.max(0.01)).floor() as u64;
        if n > 0 {
            p.at = Some(at + Duration::from_secs_f64(n as f64 * period));
            for _ in 0..n.min(p.cols.len() as u64) {
                let c = p.next(load);
                p.cols.rotate_left(1);
                let end = p.cols.len() - 1;
                p.cols[end] = c;
            }
        }
    } else {
        p.at = Some(now);
    }
    p.cols[p.cols.len() - cols..].to_vec()
}

/// Spectrum's bands as the mock moved them without sound, one for each of a screen's `cols`
/// columns, falling off toward the high ones as the mock's twenty did: for the effect's tile
/// while it isn't chosen, a picture of what the effect looks like.
pub fn sample_sound(t: f64, cols: usize) -> Vec<f32> {
    (0..cols)
        .map(|c| {
            let cf = c as f64;
            ((((t / 2.2 + cf * 0.7).sin() * 0.5 + 0.5) * (1.1 - cf / (1.3 * cols as f64)) * LOUDEST * (0.6 + rnd(cf + (t / 1.2).floor()) * 0.5)).max(0.0)) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Z10's screen, the one the mock drew on.
    const Z10: Size = (20, 4);
    /// Screens of every shape the effects are drawn on: the Z10's, shorter, wider, taller, a
    /// single cell, and none.
    const SIZES: [Size; 9] = [Z10, (16, 2), (40, 4), (20, 8), (24, 3), (16, 1), (5, 1), (1, 1), (0, 0)];

    fn rows(g: &Grid) -> Vec<String> {
        g.iter().map(|r| r.iter().collect()).collect()
    }

    #[test]
    fn every_effect_fills_the_screen_whatever_its_size_in_characters_the_keyboard_draws() {
        for size in SIZES {
            for (name, live) in [("sample", Live::sample(size.0)), ("quiet", Live::quiet()), ("twenty bands, a short trace", Live { sound: vec![12.0; 20], pulse: vec![3; 7] })] {
                for fx in FXS {
                    for t in [0.0, 13.7, 120.0, 599.0, 3600.0 * 10.0] {
                        let r = rows(&frame(fx, t, "EVGA", &live, size));
                        assert_eq!(r.len(), size.1, "{fx:?} at {t} on {size:?}, {name}");
                        for row in &r {
                            assert_eq!(row.chars().count(), size.0, "{fx:?} at {t} on {size:?}, {name}");
                            assert!(row.chars().all(evga_z10::lcd::lcd_draws), "{fx:?} at {t} on {size:?}: {row:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn effects_move_and_a_moment_always_looks_the_same() {
        let quiet = Live::quiet();
        for size in [Z10, (16, 2), (40, 4), (20, 8)] {
            for fx in FXS.into_iter().filter(|f| *f != Fx::Spectrum && *f != Fx::Heartbeat) {
                assert_eq!(frame(fx, 55.0, "EVGA", &quiet, size), frame(fx, 55.0, "EVGA", &quiet, size), "{fx:?} on {size:?}");
                let moved = (1..60).any(|k| frame(fx, 55.0 + k as f64, "EVGA", &quiet, size) != frame(fx, 55.0, "EVGA", &quiet, size));
                assert!(moved, "{fx:?} never moved on {size:?}");
            }
        }
        // Spectrum moves with the sound alone: none, and nothing shows; the loudest fills every
        // row, its twenty bands spread over any width
        assert!(rows(&frame(Fx::Spectrum, 9.0, "", &quiet, Z10)).iter().all(|r| r.trim().is_empty()));
        let loud = Live { sound: vec![12.0; 20], ..Live::quiet() };
        assert_eq!(rows(&frame(Fx::Spectrum, 9.0, "", &loud, Z10)), vec!["█".repeat(20); 4]);
        assert_eq!(rows(&frame(Fx::Spectrum, 9.0, "", &loud, (40, 4))), vec!["█".repeat(40); 4]);
        assert_eq!(rows(&frame(Fx::Spectrum, 9.0, "", &loud, (16, 2))), vec!["█".repeat(16); 2]);
        // (half as loud, half the rows; the low bands at the left)
        let half = Live { sound: (0..20).map(|b| if b < 10 { 6.0 } else { 0.0 }).collect(), ..Live::quiet() };
        assert_eq!(rows(&frame(Fx::Spectrum, 9.0, "", &half, (40, 4)))[2..], [format!("{}{}", "█".repeat(20), " ".repeat(20)), format!("{}{}", "█".repeat(20), " ".repeat(20))]);
        // Heartbeat draws its trace: two beats, as the tile shows it, in the screen's middle rows
        assert_eq!(rows(&frame(Fx::Heartbeat, 9.0, "", &Live::sample(20), Z10))[1..3], ["    /\\        /\\    ", "___/  \\/\\____/  \\/\\_"]);
        assert_eq!(rows(&frame(Fx::Heartbeat, 9.0, "", &Live::sample(16), (16, 2))), ["    /\\        /\\", "___/  \\/\\____/  "]);
        assert_eq!(rows(&frame(Fx::Heartbeat, 9.0, "", &Live::sample(10), (10, 1))), ["___//\\\\/\\_"]);
        // (a trace shorter than the screen: flat at the left, its newest at the right)
        assert_eq!(rows(&frame(Fx::Heartbeat, 9.0, "", &Live { pulse: vec![1, 2, 3], ..Live::quiet() }, (8, 2))), ["      /\\", "_____/  "]);
    }

    #[test]
    fn bounce_bounces_its_word_and_the_speed_scales_the_moment() {
        let r = rows(&frame(Fx::Bounce, 0.0, "EVGA", &Live::quiet(), Z10));
        assert!(r.iter().any(|row| row.contains("EVGA")));
        let whole = rows(&frame(Fx::Bounce, 7.0, "ABCDEFGHIJKLMNOPQRSTUVWXYZ", &Live::quiet(), Z10));
        assert!(whole.iter().any(|row| row == "ABCDEFGHIJKLMNOPQRST"), "a word as wide as the screen stays put, cut to twenty");
        let whole = rows(&frame(Fx::Bounce, 7.0, "ABCDEFGHIJKLMNOPQRSTUVWXYZ", &Live::quiet(), (16, 2)));
        assert!(whole.iter().any(|row| row == "ABCDEFGHIJKLMNOP"), "cut to the screen's sixteen");
        // (on one row it bounces along it, never off it)
        for k in 0..40 {
            assert!(rows(&frame(Fx::Bounce, k as f64, "EVGA", &Live::quiet(), (16, 1)))[0].contains("EVGA"), "at {k}");
        }
        assert_eq!(Fx::from_key("stars"), Some(Fx::Stars));
    }

    #[test]
    fn no_cell_changes_faster_than_the_screen_shows() {
        // every effect at its fastest, looked at every hundredth of a second for half a
        // minute: each cell holds what it shows at least a step, however short a step is
        let quiet = Live::quiet();
        for size in [Z10, (16, 2), (40, 4)] {
            for fx in FXS.into_iter().filter(|f| *f != Fx::Spectrum && *f != Fx::Heartbeat) {
                let mut last: Option<Grid> = None;
                let mut since = vec![vec![f64::NEG_INFINITY; size.0]; size.1];
                let mut changes = 0;
                for k in 0..3000 {
                    let secs = k as f64 / 100.0;
                    let f = frame(fx, tenths(fx, secs, 100), "EVGA", &quiet, size);
                    if let Some(l) = &last {
                        for r in 0..size.1 {
                            for c in 0..size.0 {
                                if f[r][c] != l[r][c] {
                                    // (looked at every hundredth, a change is seen up to a hundredth late)
                                    assert!(secs - since[r][c] >= period(100) - 0.0101, "{fx:?} on {size:?}: cell {r},{c} changed {:.2} s after its last change", secs - since[r][c]);
                                    since[r][c] = secs;
                                    changes += 1;
                                }
                            }
                        }
                    }
                    last = Some(f);
                }
                assert!(changes > 0, "{fx:?} never moved on {size:?}");
            }
        }
        // the slider's ends, and its middle between them by ratio
        assert!((period(100) - FASTEST).abs() < 1e-9 && (period(0) - SLOWEST).abs() < 1e-9);
        assert!((period(50) - (FASTEST * SLOWEST).sqrt()).abs() < 1e-9);
    }

    #[test]
    fn tetris_drops_whole_pieces_and_clears_full_rows() {
        for (size, rows_cleared, games_lost) in [(Z10, 5, 6), ((40, 4), 5, 6), ((10, 8), 5, 20)] {
            let mut g = Tetris::new(3, size);
            let (mut cleared, mut lost) = (0, 0);
            for _ in 0..2000 {
                // (a falling piece never lies over the stack)
                if let Some((s, x, y)) = g.piece {
                    assert!(s.iter().all(|&(cx, cy)| y + cy < 0 || !g.board[(y + cy) as usize][(x + cx) as usize]));
                }
                if g.full.is_some() {
                    cleared += 1;
                }
                if !g.step() {
                    lost += 1;
                    g = Tetris::new(lost, size);
                }
            }
            assert!(cleared >= rows_cleared, "{cleared} rows cleared in 2000 steps on {size:?}");
            assert!(lost <= games_lost, "lost {lost} games in 2000 steps on {size:?}");
        }
        // (on a screen too low for some pieces, the game starts again rather than stopping)
        let mut g = Tetris::new(1, (16, 1));
        for _ in 0..200 {
            if !g.step() {
                g = Tetris::new(2, (16, 1));
            }
        }
        // every piece, every turn, four cells, and the I piece standing and lying
        assert_eq!(turns(0).len(), 2);
        assert_eq!(turns(1).len(), 1);
        assert!((2..7).all(|k| turns(k).len() >= 2));
    }

    #[test]
    fn the_snake_never_runs_into_itself_and_grows_as_it_eats() {
        for (size, grows) in [(Z10, 10), ((40, 4), 10), ((16, 2), 6), ((5, 1), 2)] {
            let mut g = Snake::new(5, size);
            let mut longest = 0;
            for _ in 0..2000 {
                let head = g.body[0];
                assert!(!g.body.iter().skip(1).any(|b| *b == head), "the head on the body on {size:?}");
                assert!(g.body.iter().all(|&(x, y)| (0..size.0 as i32).contains(&x) && (0..size.1 as i32).contains(&y)), "the snake off {size:?}");
                longest = longest.max(g.body.len());
                if !g.step() {
                    g = Snake::new(longest as u64, size);
                }
            }
            assert!(longest >= grows, "the longest the snake grew on {size:?}: {longest}");
        }
    }

    #[test]
    fn the_invaders_march_quicker_as_they_fall_and_waves_are_shot_down() {
        for (size, least) in [(Z10, 30), ((40, 4), 10), ((24, 6), 10)] {
            let mut g = Invaders::new(7, size);
            let (mut waves, mut games, mut marches) = (0, 0, 0);
            for _ in 0..3000 {
                // (a shot is never inside an invader)
                assert!(g.shot.is_none_or(|s| !g.standing().any(|at| at == s)));
                let (before, marched) = (g.standing().count(), g.marched);
                if !g.step() {
                    games += 1;
                    g = Invaders::new(games, size);
                    continue;
                }
                // after each march, a wait as long as a quarter of the invaders standing then (one
                // may have been shot down in the step, before the march or after it)
                if g.marched > marched {
                    marches += 1;
                    let after = g.standing().count();
                    assert!((after..=before).any(|n| g.wait == (n as u64 / 4).max(1)), "{before} to {after} standing, wait {}", g.wait);
                }
                if before > 0 && g.standing().count() == 0 {
                    waves += 1;
                }
            }
            // (the cannon wins most waves; now and then the invaders land)
            assert!(waves >= least && games * 5 <= waves, "{waves} waves shot down and {games} games lost in 3000 steps on {size:?}");
            assert!(marches > 300, "{marches} marches on {size:?}");
        }
        // six to a row on the Z10's twenty columns, twelve on forty, fewer on a narrow screen
        assert_eq!([Z10, (40, 4), (16, 2), (8, 4)].map(|s| Invaders::new(1, s).per_row()), [6, 12, 5, 1]);
        // the screen: the cannon on the bottom row, and both rows of invaders at the start;
        // the tile's picture, the first shot climbing at the whole wave
        let r = rows(&frame(Fx::Invaders, 0.0, "", &Live::quiet(), Z10));
        assert!(r[3].contains('A') && r[0].contains('Y') && r[1].contains('M'), "{r:?}");
        let r = rows(&frame(Fx::Invaders, still(Fx::Invaders), "", &Live::quiet(), Z10));
        assert!(r[0].matches('T').count() == 6 && r[1].matches('W').count() == 6 && r[2].contains('|') && r[3].contains('A'), "{r:?}");
        let r = rows(&frame(Fx::Invaders, 0.0, "", &Live::quiet(), (24, 6)));
        assert!(r[5].contains('A') && r[0].contains('Y'), "the cannon on the bottom row of six: {r:?}");
    }

    #[test]
    fn the_car_always_goes_round_the_traffic() {
        for size in [Z10, (40, 8), (16, 2), (24, 3)] {
            for k in 0..5000 {
                let lane = car_lane(k, size);
                assert!(traffic(k, size).all(|(l, c)| l != lane || !(1..=2).contains(&c)), "a crash at step {k} on {size:?}");
                let f = frame(Fx::Highway, k as f64, "", &Live::quiet(), size);
                assert_eq!((f[lane][1], f[lane][2]), ('=', '>'));
            }
        }
        // the edges on the Z10's four rows, two lanes between; on two rows, the lanes alone
        let f = rows(&frame(Fx::Highway, 0.0, "", &Live::quiet(), Z10));
        assert!(f[0].contains('-') && f[3].contains('-'), "{f:?}");
        let f = rows(&frame(Fx::Highway, 0.0, "", &Live::quiet(), (16, 2)));
        assert!(!f.iter().any(|r| r.contains('-')), "{f:?}");
    }

    #[test]
    fn fireworks_go_up_and_burst() {
        let burst = |r: &String| r.contains("-*-") || r.contains("-+-") || r.contains("-o-");
        let bursts = (0..300).filter(|k| rows(&frame(Fx::Fireworks, *k as f64, "", &Live::quiet(), Z10)).iter().any(burst)).count();
        assert!(bursts > 50, "{bursts} steps with a burst in 300");
        // (on the Z10's screen each bursts on the second row; on a taller one, on rows from there down)
        let rows_burst = |size: Size| (0..300).flat_map(|k| rows(&frame(Fx::Fireworks, k as f64, "", &Live::quiet(), size)).into_iter().enumerate().filter(|(_, r)| burst(r)).map(|(i, _)| i).collect::<Vec<_>>()).collect::<std::collections::BTreeSet<_>>();
        assert_eq!(rows_burst(Z10).into_iter().collect::<Vec<_>>(), [1]);
        assert!(rows_burst((20, 8)).len() >= 3, "{:?}", rows_burst((20, 8)));
        // (twice the rockets on twice the columns)
        let lit = |size: Size| (0..300).map(|k| rows(&frame(Fx::Fireworks, k as f64, "", &Live::quiet(), size)).iter().map(|r| r.chars().filter(|c| *c != ' ').count()).sum::<usize>()).sum::<usize>();
        assert!(lit((40, 4)) > lit(Z10) * 3 / 2, "{} cells lit on forty columns, {} on twenty", lit((40, 4)), lit(Z10));
    }

    #[test]
    fn the_heart_beats_faster_as_the_processor_works_harder() {
        let beats = |load: f64| {
            let mut p = Pulse { cols: vec![0; 20], beat: 0, since: 0, at: None };
            (0..140).filter(|_| p.next(load) == 1).count()
        };
        assert_eq!((beats(0.0), beats(100.0)), (10, 20));
        // the trace starts as the tiles show it, carrying on from its last beat: on the Z10's
        // twenty columns, as it always started
        let z10 = Pulse::new(20);
        assert_eq!((z10.cols, z10.beat, z10.since), (vec![0, 0, 0, 1, 2, 3, 4, 5, 6, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 0], 0, 6));
        // (sixteen columns end in a beat, which the trace finishes before the line)
        let mut p = Pulse::new(16);
        assert_eq!((p.cols.last(), p.beat), (Some(&3), 4));
        assert_eq!((0..4).map(|_| p.next(0.0)).collect::<Vec<_>>(), [4, 5, 6, 0]);
    }

    #[test]
    fn the_tiles_spectrum_falls_off_toward_the_high_bands_as_the_mocks_did() {
        // the mock's own sum on its twenty columns
        let mock: Vec<f32> = (0..20)
            .map(|c| {
                let cf = c as f64;
                ((((40.0f64 / 2.2 + cf * 0.7).sin() * 0.5 + 0.5) * (1.1 - cf / 26.0) * 12.0 * (0.6 + rnd(cf + (40.0f64 / 1.2).floor()) * 0.5)).max(0.0)) as f32
            })
            .collect();
        assert_eq!(sample_sound(40.0, 20), mock);
        assert_eq!(Live::sample(20).sound, mock);
        assert_eq!(sample_sound(40.0, 40).len(), 40);
    }
}
