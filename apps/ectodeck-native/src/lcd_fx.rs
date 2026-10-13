//! The screen's effects: moving patterns made of the characters the keyboard draws, as the
//! first screen mock drew them (mock/z10-redesign/lcd.js, carried over line for line), each
//! the whole screen's four rows of twenty at a moment `t`, counted in tenths of a second. An
//! Effect block shows the cells of its own place on the screen, behind the other blocks.

const W: usize = 20;
const H: usize = 4;
const FULL: char = '█';
/// a drop at the top, middle and foot of a row
const SUB: [char; 3] = ['\'', '-', '.'];
/// from nothing to the most, a cell at a time
const RAMP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

/// An effect, as its tile names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fx {
    Rain,
    Snow,
    Stars,
    Code,
    Fire,
    Plasma,
    Life,
    Spectrum,
    Aquarium,
    Pong,
    Bounce,
}

/// The effects, in the tiles' order.
pub const FXS: [Fx; 11] = [Fx::Rain, Fx::Snow, Fx::Stars, Fx::Code, Fx::Fire, Fx::Plasma, Fx::Life, Fx::Spectrum, Fx::Aquarium, Fx::Pong, Fx::Bounce];

impl Fx {
    pub fn key(self) -> &'static str {
        match self {
            Fx::Rain => "rain",
            Fx::Snow => "snow",
            Fx::Stars => "stars",
            Fx::Code => "code",
            Fx::Fire => "fire",
            Fx::Plasma => "plasma",
            Fx::Life => "life",
            Fx::Spectrum => "spectrum",
            Fx::Aquarium => "aquarium",
            Fx::Pong => "pong",
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
            Fx::Life => "Life",
            Fx::Spectrum => "Spectrum",
            Fx::Aquarium => "Aquarium",
            Fx::Pong => "Pong",
            Fx::Bounce => "Bounce",
        }
    }
}

/// An effect's moment: `secs` since its layout came on, at `speed` (0 slow to 100 fast; 50
/// plays it as the mock does), in the tenths of a second the effects count in.
pub fn tenths(secs: f64, speed: u8) -> f64 {
    secs.max(0.0) * 10.0 * 2f64.powf((speed.min(100) as f64 - 50.0) / 50.0 * 1.5)
}

/// The mock's noise: for each `i` a number from 0 to 1, always the same one.
fn rnd(i: f64) -> f64 {
    let x = (i * 12.9898).sin() * 43758.5453;
    x - x.floor()
}

/// The whole screen at moment `t`: `word` is the one Bounce bounces, and `sound` the
/// computer's sound for Spectrum, in twenty bands low to high, each 0 to 12.
pub fn frame(fx: Fx, t: f64, word: &str, sound: &[f32; W]) -> [[char; W]; H] {
    let t = t.max(0.0);
    let mut g = [[' '; W]; H];
    match fx {
        Fx::Rain => {
            for c in 0..W {
                for k in 0..2 {
                    let (cf, kf) = (c as f64, k as f64);
                    let sp = 0.9 + rnd(cf * 7.0 + kf) * 0.8;
                    let y = ((t * sp + rnd(cf + kf * 31.0) * 40.0) % 16.0).floor() as usize;
                    if y < 12 {
                        g[y / 3][c] = SUB[y % 3];
                    } else if y == 12 {
                        g[3][c] = ',';
                    }
                }
            }
        }
        Fx::Snow => {
            for i in 0..16 {
                let fi = i as f64;
                let y = ((t * 0.35 + rnd(fi) * 40.0) % 14.0).floor() as usize;
                let x = ((rnd(fi * 3.0) * 20.0 + (t / 12.0 + fi).sin() * 1.5 + 20.0) % 20.0).floor() as usize;
                if y < 12 {
                    g[y / 3][x.min(W - 1)] = ['°', '*', '.'][y % 3];
                }
            }
            // (the snow on the ground)
            for c in 0..W {
                if rnd(c as f64 * 5.0) > 0.55 {
                    g[3][c] = '_';
                }
            }
        }
        Fx::Stars => {
            for i in 0..14 {
                let fi = i as f64;
                let sp = 0.3 + rnd(fi) * 1.4;
                let x = ((t * sp + rnd(fi * 9.0) * 20.0) % 22.0).floor() as i32 - 1;
                let r = ((rnd(fi * 4.0) * 4.0).floor() as usize).min(H - 1);
                let tail = if sp > 1.1 { '=' } else if sp > 0.7 { '-' } else { '.' };
                if (0..W as i32).contains(&x) {
                    g[r][x as usize] = if sp > 1.1 { '*' } else { '.' };
                }
                if (0..W as i32).contains(&(x - 1)) && sp > 0.7 {
                    g[r][(x - 1) as usize] = tail;
                }
            }
        }
        Fx::Code => {
            let ch: Vec<char> = "01ABZ<>*+=$#%&?".chars().collect();
            for c in 0..W {
                let cf = c as f64;
                let sp = 0.25 + rnd(cf) * 0.5;
                let head = ((t * sp + rnd(cf * 3.0) * 12.0) % 9.0).floor() as i32;
                for r in 0..H as i32 {
                    if r <= head && head - r < 3 {
                        let k = (rnd(cf * 17.0 + r as f64 + (t / 2.0).floor()) * ch.len() as f64).floor() as usize;
                        g[r as usize][c] = ch[k.min(ch.len() - 1)];
                    }
                }
            }
        }
        Fx::Fire => {
            for c in 0..W {
                for r in 0..H {
                    let (cf, rf) = (c as f64, r as f64);
                    let h = 3.4 + (cf * 0.9 + t / 4.0).sin() * 0.8 + (cf * 2.3 - t / 2.5).sin() * 0.6 + (rnd(cf + (t / 1.5).floor() * 20.0) - 0.5);
                    let v = (h - (3.0 - rf)) / 1.4;
                    g[r][c] = if v > 1.6 { FULL } else { RAMP[(v * 6.0).floor().clamp(0.0, 9.0) as usize] };
                }
            }
        }
        Fx::Plasma => {
            for r in 0..H {
                for c in 0..W {
                    let (cf, rf) = (c as f64, r as f64);
                    let v = (cf / 3.0 + t / 9.0).sin() + (rf / 1.5 + t / 7.0).sin() + ((cf + rf * 2.0) / 4.0 + t / 11.0).sin();
                    g[r][c] = RAMP[(((v + 3.0) / 6.0) * 9.99).floor().clamp(0.0, 9.0) as usize];
                }
            }
        }
        Fx::Life => {
            let cells = life_at((t / 3.0).floor() as u64);
            for (i, on) in cells.iter().enumerate() {
                if *on {
                    g[i / W][i % W] = 'o';
                }
            }
        }
        Fx::Spectrum => {
            for c in 0..W {
                let lv = sound[c].max(0.0) as f64;
                for r in 0..H {
                    let k = lv - (3 - r) as f64 * 3.0;
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
            let put = |g: &mut [[char; W]; H], r: usize, x: i32, s: &str| {
                for (i, ch) in s.chars().enumerate() {
                    let c = x + i as i32;
                    if (0..W as i32).contains(&c) {
                        g[r][c as usize] = ch;
                    }
                }
            };
            put(&mut g, 1, ((t / 3.0) % 28.0).floor() as i32 - 4, "><>");
            put(&mut g, 2, 20 - ((t / 4.5 + 9.0) % 28.0).floor() as i32 + 4, "<><");
            put(&mut g, 0, ((t / 6.0 + 14.0) % 30.0).floor() as i32 - 5, "><(((°>");
            // (bubbles rising)
            for i in 0..3 {
                let y = 11 - ((t * 0.5 + i as f64 * 4.0) % 12.0).floor() as i32;
                let x = 4 + i * 6;
                g[(y / 3).clamp(0, 3) as usize][x] = if y % 3 == 2 { '.' } else { 'o' };
            }
            put(&mut g, 3, 0, ")  (  )     (   ) (");
        }
        Fx::Pong => {
            let x = 1.0 + (((t / 1.2) % 36.0) - 18.0).abs();
            let y = (((t / 1.6) % 22.0) - 11.0).abs();
            let r = ((y / 3.0).floor() as usize).min(H - 1);
            g[r][(x.round() as usize).min(W - 1)] = 'o';
            let lp = (r as i32 + if (t / 5.0).sin() > 0.0 { 0 } else { -1 }).clamp(0, 3) as usize;
            g[lp][0] = '|';
            g[r][W - 1] = '|';
            g[0][8] = '3';
            g[0][11] = '2';
            for row in g.iter_mut() {
                if row[10] == ' ' {
                    row[10] = ':';
                }
            }
        }
        Fx::Bounce => {
            let word: Vec<char> = if word.is_empty() { vec![' '] } else { word.chars().take(W).collect() };
            let w = word.len();
            let span = (W - w).max(1) as f64;
            let x = if w >= W { 0.0 } else { (((t / 2.0) % (2.0 * span)) - span).abs() };
            let y = (((t / 5.0) % 6.0) - 3.0).abs();
            let (row, at) = ((y.round() as usize).min(H - 1), x.round() as usize);
            for (i, ch) in word.iter().enumerate() {
                if at + i < W {
                    g[row][at + i] = *ch;
                }
            }
        }
    }
    g
}

/// The board of Life after `n` generations: seeded afresh every thirty, and whenever it
/// stands still, swings between two boards or nearly dies out, as the mock seeded it (there
/// by chance; here from the mock's own noise, so a moment always looks the same).
fn life_at(n: u64) -> [bool; W * H] {
    let seed = |k: u64| -> [bool; W * H] { std::array::from_fn(|i| rnd(k as f64 * 97.0 + i as f64 * 1.37 + 0.5) > 0.6) };
    let start = n - n % 30;
    let mut cells = seed(start);
    let (mut prev, mut prev2): (Option<[bool; W * H]>, Option<[bool; W * H]>) = (None, None);
    for g in start..n {
        let key = cells;
        if Some(key) == prev || Some(key) == prev2 || key.iter().filter(|v| **v).count() < 8 {
            cells = seed(g * 7919 + 1);
        }
        (prev2, prev) = (prev, Some(key));
        cells = step(&cells);
    }
    cells
}

/// One generation of Life on the screen's cells, its edges joined round.
fn step(c: &[bool; W * H]) -> [bool; W * H] {
    std::array::from_fn(|i| {
        let (r, col) = ((i / W) as i32, (i % W) as i32);
        let mut s = 0;
        for dr in -1..=1 {
            for dc in -1..=1 {
                if (dr, dc) != (0, 0) && c[((r + dr + H as i32) % H as i32) as usize * W + ((col + dc + W as i32) % W as i32) as usize] {
                    s += 1;
                }
            }
        }
        s == 3 || (c[i] && s == 2)
    })
}

/// Spectrum's bands as the mock moved them without sound: for the effect's tile while it
/// isn't chosen, a picture of what the effect looks like.
pub fn sample_sound(t: f64) -> [f32; W] {
    std::array::from_fn(|c| {
        let cf = c as f64;
        ((((t / 2.2 + cf * 0.7).sin() * 0.5 + 0.5) * (1.1 - cf / 26.0) * 12.0 * (0.6 + rnd(cf + (t / 1.2).floor()) * 0.5)).max(0.0)) as f32
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(g: [[char; W]; H]) -> Vec<String> {
        g.iter().map(|r| r.iter().collect()).collect()
    }

    #[test]
    fn every_effect_fills_four_rows_of_twenty_in_characters_the_keyboard_draws() {
        let sound = sample_sound(40.0);
        for fx in FXS {
            for t in [0.0, 13.7, 120.0, 3600.0 * 10.0] {
                let r = rows(frame(fx, t, "EVGA", &sound));
                assert_eq!(r.len(), 4);
                for row in &r {
                    assert_eq!(row.chars().count(), 20, "{fx:?} at {t}");
                    assert!(row.chars().all(ectotype_core::lcd::lcd_draws), "{fx:?} at {t}: {row:?}");
                }
            }
        }
    }

    #[test]
    fn effects_move_and_a_moment_always_looks_the_same() {
        let quiet = [0.0; W];
        for fx in FXS.into_iter().filter(|f| *f != Fx::Spectrum) {
            assert_eq!(frame(fx, 55.0, "EVGA", &quiet), frame(fx, 55.0, "EVGA", &quiet), "{fx:?}");
            let moved = (1..60).any(|k| frame(fx, 55.0 + k as f64, "EVGA", &quiet) != frame(fx, 55.0, "EVGA", &quiet));
            assert!(moved, "{fx:?} never moved");
        }
        // Spectrum moves with the sound alone: none, and nothing shows
        assert!(rows(frame(Fx::Spectrum, 9.0, "", &quiet)).iter().all(|r| r.trim().is_empty()));
        let loud = [12.0; W];
        assert_eq!(rows(frame(Fx::Spectrum, 9.0, "", &loud)), vec!["█".repeat(20); 4]);
    }

    #[test]
    fn bounce_bounces_its_word_and_the_speed_scales_the_moment() {
        let r = rows(frame(Fx::Bounce, 0.0, "EVGA", &[0.0; W]));
        assert!(r.iter().any(|row| row.contains("EVGA")));
        let whole = rows(frame(Fx::Bounce, 7.0, "ABCDEFGHIJKLMNOPQRSTUVWXYZ", &[0.0; W]));
        assert!(whole.iter().any(|row| row == "ABCDEFGHIJKLMNOPQRST"), "a word as wide as the screen stays put, cut to twenty");
        assert_eq!(tenths(2.0, 50), 20.0);
        assert!(tenths(2.0, 0) < 20.0 && tenths(2.0, 100) > 20.0);
        assert_eq!(Fx::from_key("stars"), Some(Fx::Stars));
    }
}
