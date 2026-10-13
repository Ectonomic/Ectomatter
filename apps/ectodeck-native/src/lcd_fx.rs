//! The screen's effects: moving patterns made of the characters the keyboard draws, as the
//! first screen mock drew them (mock/z10-redesign/lcd.js, carried over line for line), each
//! the whole screen's four rows of twenty at a moment `t`, counted in tenths of a second. An
//! Effect block shows the cells of its own place on the screen, behind the other blocks.

use std::cell::RefCell;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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
    Fireworks,
    Spectrum,
    Heartbeat,
    Aquarium,
    Pong,
    Tetris,
    Snake,
    Highway,
    Bounce,
}

/// The effects, in the tiles' order.
pub const FXS: [Fx; 15] = [
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
        Fx::Fireworks | Fx::Heartbeat | Fx::Tetris | Fx::Snake | Fx::Highway => 1.0,
        // (the bubbles: half a step a tenth)
        Fx::Aquarium => 2.0,
        Fx::Pong => 1.2,
        Fx::Bounce => 2.0,
        Fx::Spectrum => 1.0,
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
/// Spectrum, in twenty bands low to high, each 0 to 12 (see [crate::sound::held]), and the
/// heartbeat's trace (see [pulse]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Live {
    pub sound: [f32; W],
    pub pulse: [u8; W],
}

impl Live {
    /// Nothing heard, the trace flat.
    pub fn quiet() -> Live {
        Live { sound: [0.0; W], pulse: [0; W] }
    }

    /// As the tiles show the live effects standing still: Spectrum as the mock moved it, and
    /// two beats.
    pub fn sample() -> Live {
        Live { sound: sample_sound(40.0), pulse: SAMPLE_PULSE }
    }
}

/// What effect `fx` at `speed` draws from now: the sound held a step for Spectrum, the trace
/// moving on with the processor's `load` (0 to 100) for Heartbeat.
pub fn live(fx: Fx, speed: u8, load: f64) -> Live {
    Live {
        sound: if fx == Fx::Spectrum { crate::sound::held(period(speed)) } else { [0.0; W] },
        pulse: if fx == Fx::Heartbeat { pulse(period(speed), load) } else { [0; W] },
    }
}

/// The whole screen at moment `t`: `word` is the one Bounce bounces, and `live` what Spectrum
/// and Heartbeat draw from.
pub fn frame(fx: Fx, t: f64, word: &str, live: &Live) -> [[char; W]; H] {
    let t = t.max(0.0);
    // (the games and the fireworks count whole steps)
    let k = t.round() as u64;
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
        Fx::Fireworks => fireworks(k, &mut g),
        Fx::Heartbeat => {
            // (each column of the trace, its top and its foot: the line, the stroke up, the
            // peak's two sides, the stroke down, and the dip after)
            const LOOKS: [(char, char); 7] = [(' ', '_'), (' ', '/'), ('/', ' '), ('\\', ' '), (' ', '\\'), (' ', '/'), (' ', '\\')];
            for c in 0..W {
                (g[1][c], g[2][c]) = LOOKS[(live.pulse[c] as usize).min(6)];
            }
        }
        Fx::Tetris => played(&TETRIS, k, Tetris::new, Tetris::step).draw(&mut g),
        Fx::Snake => played(&SNAKE, k, Snake::new, Snake::step).draw(&mut g),
        Fx::Highway => highway(k, &mut g),
        Fx::Spectrum => {
            for c in 0..W {
                let lv = live.sound[c].max(0.0) as f64;
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

fn put(g: &mut [[char; W]; H], r: i32, c: i32, ch: char) {
    if (0..H as i32).contains(&r) && (0..W as i32).contains(&c) {
        g[r as usize][c as usize] = ch;
    }
}

/// Fireworks at step `k`: a rocket goes up every two steps or so at a column of its own,
/// two rows in two steps, bursts and holds its burst for two more, throws its sparks wide for
/// one, and is gone; two or three in the sky at once.
fn fireworks(k: u64, g: &mut [[char; W]; H]) {
    for j in (k / 2).saturating_sub(4)..=k / 2 {
        let start = 2 * j + (rnd(j as f64 * 1.3 + 0.2) * 2.0) as u64;
        if start > k {
            continue;
        }
        let x = 2 + (rnd(j as f64 * 2.9 + 0.7) * 16.0) as i32;
        let spark = ['*', '+', 'o'][(rnd(j as f64 * 4.1 + 0.9) * 3.0) as usize % 3];
        match k - start {
            0 => put(g, 3, x, '|'),
            1 => put(g, 2, x, '|'),
            2 | 3 => {
                for (r, row) in [['\\', '|', '/'], ['-', spark, '-'], ['/', '|', '\\']].iter().enumerate() {
                    for (i, ch) in row.iter().enumerate() {
                        put(g, r as i32, x - 1 + i as i32, *ch);
                    }
                }
            }
            4 => {
                for (r, cols) in [(0, &[-2, 0, 2][..]), (1, &[-2, 2][..]), (2, &[-2, 0, 2][..])] {
                    for dc in cols {
                        put(g, r, x + dc, '.');
                    }
                }
            }
            _ => {}
        }
    }
}

/// Highway at step `k`: two lanes between edges dashing past, the traffic coming on at the
/// right a cell a step, and the car (=>) in whichever lane the next car ahead isn't.
fn highway(k: u64, g: &mut [[char; W]; H]) {
    for c in 0..W {
        if (c as u64 + k) % 2 == 0 {
            g[0][c] = '-';
            g[3][c] = '-';
        }
    }
    for (lane, c) in traffic(k) {
        if (0..W as i64).contains(&c) {
            g[lane][c as usize] = FULL;
        }
    }
    let mine = car_lane(k);
    (g[mine][1], g[mine][2]) = ('=', '>');
}

/// The traffic on the highway at step `k`: each car's lane (row 1 or 2) and column, the ones
/// still to come past the right end. Car j comes on at step 4j or one later, so three cells
/// or more after the one before, and the car always has room to go round.
fn traffic(k: u64) -> impl Iterator<Item = (usize, i64)> {
    let on = |j: u64| 4 * j + (rnd(j as f64 * 1.7 + 0.3) * 2.0) as u64;
    let lane = |j: u64| 1 + usize::from(rnd(j as f64 * 2.3 + 0.5) > 0.5);
    ((k / 4).saturating_sub(6)..=k / 4 + 2).map(move |j| (lane(j), W as i64 - 1 - (k as i64 - on(j) as i64)))
}

/// The lane the car (in the second and third columns) is in at step `k`: the other one from
/// the next car still ahead of its tail.
fn car_lane(k: u64) -> usize {
    traffic(k).filter(|(_, c)| *c >= 1).min_by_key(|(_, c)| *c).map_or(1, |(lane, _)| 3 - lane)
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

thread_local! {
    static TETRIS: RefCell<Option<(u64, u64, Tetris)>> = const { RefCell::new(None) };
    static SNAKE: RefCell<Option<(u64, u64, Snake)>> = const { RefCell::new(None) };
}

/// A game played to step `k`: carried on from where this thread last played it, when that is
/// earlier in the same round, else from the round's start; a lost game starts again.
fn played<G: Clone>(cache: &'static std::thread::LocalKey<RefCell<Option<(u64, u64, G)>>>, k: u64, new: impl Fn(u64) -> G, step: impl Fn(&mut G) -> bool) -> G {
    let (round, at) = (k / ROUND, k % ROUND);
    cache.with(|c| {
        let mut c = c.borrow_mut();
        let (mut i, mut g) = match c.take() {
            Some((r, i, g)) if r == round && i <= at => (i, g),
            _ => (0, new(round * 1000)),
        };
        while i < at {
            if !step(&mut g) {
                g = new(round * 1000 + i + 1);
            }
            i += 1;
        }
        *c = Some((round, i, g.clone()));
        g
    })
}

type Shape = [(i32, i32); 4];
type Board = [[bool; W]; H];

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
    let heights: Vec<i32> = (0..W).map(|c| (0..H).find(|&r| b[r][c]).map_or(0, |r| (H - r) as i32)).collect();
    let holes: usize = (0..W).map(|c| (0..H).find(|&r| b[r][c]).map_or(0, |top| (top..H).filter(|&r| !b[r][c]).count())).sum();
    let bumps: i32 = heights.windows(2).map(|w| (w[0] - w[1]).abs()).sum();
    0.76 * lines as f64 - 0.51 * heights.iter().sum::<i32>() as f64 - 0.36 * holes as f64 - 0.18 * bumps as f64
}

/// Tetris on the screen, a well twenty wide and four deep: each piece comes in over the
/// column its player picks and falls a row a step; full rows show as a line for a step, then go.
#[derive(Clone)]
struct Tetris {
    board: Board,
    piece: Option<(Shape, i32, i32)>,
    full: Option<[bool; H]>,
    bag: Vec<usize>,
    dice: Dice,
}

impl Tetris {
    fn new(seed: u64) -> Tetris {
        Tetris { board: [[false; W]; H], piece: None, full: None, bag: Vec::new(), dice: Dice::new(seed) }
    }

    fn fits(board: &Board, s: &Shape, x: i32, y: i32) -> bool {
        s.iter().all(|&(cx, cy)| {
            let (px, py) = (x + cx, y + cy);
            (0..W as i32).contains(&px) && py < H as i32 && (py < 0 || !board[py as usize][px as usize])
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
            for x in 0..=(W as i32 - w) {
                let mut y = -h;
                while Tetris::fits(&self.board, &s, x, y + 1) {
                    y += 1;
                }
                let mut b = self.board;
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
        if let Some(full) = self.full.take() {
            let kept: Vec<[bool; W]> = (0..H).filter(|r| !full[*r]).map(|r| self.board[r]).collect();
            let mut b = [[false; W]; H];
            for (i, row) in kept.iter().rev().enumerate() {
                b[H - 1 - i] = *row;
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
                let full: [bool; H] = std::array::from_fn(|r| self.board[r].iter().all(|c| *c));
                if full.iter().any(|f| *f) {
                    self.full = Some(full);
                }
            }
        }
        true
    }

    fn draw(&self, g: &mut [[char; W]; H]) {
        for r in 0..H {
            for c in 0..W {
                if self.board[r][c] {
                    g[r][c] = if self.full.is_some_and(|f| f[r]) { '=' } else { FULL };
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
    dice: Dice,
}

impl Snake {
    fn new(seed: u64) -> Snake {
        let mut s = Snake { body: [(5, 1), (4, 1), (3, 1), (2, 1)].into_iter().collect(), dir: (1, 0), food: (0, 0), dice: Dice::new(seed) };
        s.feed();
        s
    }

    /// Food somewhere the snake isn't; false when there's nowhere left.
    fn feed(&mut self) -> bool {
        let free: Vec<(i32, i32)> = (0..H as i32).flat_map(|y| (0..W as i32).map(move |x| (x, y))).filter(|c| !self.body.contains(c)).collect();
        if free.is_empty() {
            return false;
        }
        self.food = free[self.dice.below(free.len() as u64) as usize];
        true
    }

    /// Whether the snake may go into cell `c`: on the screen, and not itself (its tail cell
    /// moves on as it goes, unless it is `eating`).
    fn free(&self, c: (i32, i32), eating: bool) -> bool {
        let body = self.body.len() - usize::from(!eating);
        (0..W as i32).contains(&c.0) && (0..H as i32).contains(&c.1) && !self.body.iter().take(body).any(|b| *b == c)
    }

    /// A step: a cell on, straight or turning. False when every way is blocked.
    fn step(&mut self) -> bool {
        let head = self.body[0];
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

    fn draw(&self, g: &mut [[char; W]; H]) {
        put(g, self.food.1, self.food.0, '*');
        for (i, &(x, y)) in self.body.iter().enumerate() {
            put(g, y, x, if i == 0 { '@' } else { 'o' });
        }
    }
}

// ------------------------------------------------------------------ the heartbeat

/// The heartbeat as the tiles show it standing still, and as it starts: two beats.
const SAMPLE_PULSE: [u8; W] = [0, 0, 0, 1, 2, 3, 4, 5, 6, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 0];

/// The trace, as it is drawn on: its columns, the beat being drawn (its next column, 0 for
/// none), the columns since the last beat began, and when it last moved on.
struct Pulse {
    cols: [u8; W],
    beat: u8,
    since: u32,
    at: Option<Instant>,
}

impl Pulse {
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

/// The heartbeat's trace now, its last twenty columns, the newest at the right: a column more
/// every `period` seconds, beating as fast as the processor's `load` (0 to 100) says. Every
/// screen showing a heartbeat draws this one trace, which moves on with the clock.
pub fn pulse(period: f64, load: f64) -> [u8; W] {
    static PULSE: OnceLock<Mutex<Pulse>> = OnceLock::new();
    let mut p = PULSE.get_or_init(|| Mutex::new(Pulse { cols: SAMPLE_PULSE, beat: 0, since: 6, at: None })).lock().unwrap();
    let now = Instant::now();
    let Some(at) = p.at else {
        p.at = Some(now);
        return p.cols;
    };
    let n = (now.duration_since(at).as_secs_f64() / period.max(0.01)).floor() as u64;
    if n > 0 {
        p.at = Some(at + Duration::from_secs_f64(n as f64 * period));
        for _ in 0..n.min(W as u64) {
            let c = p.next(load);
            p.cols.rotate_left(1);
            p.cols[W - 1] = c;
        }
    }
    p.cols
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
        let live = Live::sample();
        for fx in FXS {
            for t in [0.0, 13.7, 120.0, 599.0, 3600.0 * 10.0] {
                let r = rows(frame(fx, t, "EVGA", &live));
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
        let quiet = Live::quiet();
        for fx in FXS.into_iter().filter(|f| *f != Fx::Spectrum && *f != Fx::Heartbeat) {
            assert_eq!(frame(fx, 55.0, "EVGA", &quiet), frame(fx, 55.0, "EVGA", &quiet), "{fx:?}");
            let moved = (1..60).any(|k| frame(fx, 55.0 + k as f64, "EVGA", &quiet) != frame(fx, 55.0, "EVGA", &quiet));
            assert!(moved, "{fx:?} never moved");
        }
        // Spectrum moves with the sound alone: none, and nothing shows
        assert!(rows(frame(Fx::Spectrum, 9.0, "", &quiet)).iter().all(|r| r.trim().is_empty()));
        let loud = Live { sound: [12.0; W], ..Live::quiet() };
        assert_eq!(rows(frame(Fx::Spectrum, 9.0, "", &loud)), vec!["█".repeat(20); 4]);
        // Heartbeat draws its trace: two beats, as the tile shows it
        assert_eq!(rows(frame(Fx::Heartbeat, 9.0, "", &Live::sample()))[1..3], ["    /\\        /\\    ", "___/  \\/\\____/  \\/\\_"]);
    }

    #[test]
    fn bounce_bounces_its_word_and_the_speed_scales_the_moment() {
        let r = rows(frame(Fx::Bounce, 0.0, "EVGA", &Live::quiet()));
        assert!(r.iter().any(|row| row.contains("EVGA")));
        let whole = rows(frame(Fx::Bounce, 7.0, "ABCDEFGHIJKLMNOPQRSTUVWXYZ", &Live::quiet()));
        assert!(whole.iter().any(|row| row == "ABCDEFGHIJKLMNOPQRST"), "a word as wide as the screen stays put, cut to twenty");
        assert_eq!(Fx::from_key("stars"), Some(Fx::Stars));
    }

    #[test]
    fn no_cell_changes_faster_than_the_screen_shows() {
        // every effect at its fastest, looked at every hundredth of a second for half a
        // minute: each cell holds what it shows at least a step, however short a step is
        let quiet = Live::quiet();
        for fx in FXS.into_iter().filter(|f| *f != Fx::Spectrum && *f != Fx::Heartbeat) {
            let mut last: Option<[[char; W]; H]> = None;
            let mut since = [[f64::NEG_INFINITY; W]; H];
            let mut changes = 0;
            for k in 0..3000 {
                let secs = k as f64 / 100.0;
                let f = frame(fx, tenths(fx, secs, 100), "EVGA", &quiet);
                if let Some(l) = last {
                    for r in 0..H {
                        for c in 0..W {
                            if f[r][c] != l[r][c] {
                                // (looked at every hundredth, a change is seen up to a hundredth late)
                                assert!(secs - since[r][c] >= period(100) - 0.0101, "{fx:?}: cell {r},{c} changed {:.2} s after its last change", secs - since[r][c]);
                                since[r][c] = secs;
                                changes += 1;
                            }
                        }
                    }
                }
                last = Some(f);
            }
            assert!(changes > 0, "{fx:?} never moved");
        }
        // the slider's ends, and its middle between them by ratio
        assert!((period(100) - FASTEST).abs() < 1e-9 && (period(0) - SLOWEST).abs() < 1e-9);
        assert!((period(50) - (FASTEST * SLOWEST).sqrt()).abs() < 1e-9);
    }

    #[test]
    fn tetris_drops_whole_pieces_and_clears_full_rows() {
        let mut g = Tetris::new(3);
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
                g = Tetris::new(lost);
            }
        }
        assert!(cleared >= 5, "{cleared} rows cleared in 2000 steps");
        assert!(lost <= 6, "lost {lost} games in 2000 steps");
        // every piece, every turn, four cells, and the I piece standing and lying
        assert_eq!(turns(0).len(), 2);
        assert_eq!(turns(1).len(), 1);
        assert!((2..7).all(|k| turns(k).len() >= 2));
    }

    #[test]
    fn the_snake_never_runs_into_itself_and_grows_as_it_eats() {
        let mut g = Snake::new(5);
        let mut longest = 0;
        for _ in 0..2000 {
            let head = g.body[0];
            assert!(!g.body.iter().skip(1).any(|b| *b == head), "the head on the body");
            longest = longest.max(g.body.len());
            if !g.step() {
                g = Snake::new(longest as u64);
            }
        }
        assert!(longest >= 10, "the longest the snake grew: {longest}");
    }

    #[test]
    fn the_car_always_goes_round_the_traffic() {
        for k in 0..5000 {
            let lane = car_lane(k);
            assert!(traffic(k).all(|(l, c)| l != lane || !(1..=2).contains(&c)), "a crash at step {k}");
            let f = frame(Fx::Highway, k as f64, "", &Live::quiet());
            assert_eq!((f[lane][1], f[lane][2]), ('=', '>'));
        }
    }

    #[test]
    fn fireworks_go_up_and_burst() {
        let bursts = (0..300).filter(|k| rows(frame(Fx::Fireworks, *k as f64, "", &Live::quiet())).iter().any(|r| r.contains("-*-") || r.contains("-+-") || r.contains("-o-"))).count();
        assert!(bursts > 50, "{bursts} steps with a burst in 300");
    }

    #[test]
    fn the_heart_beats_faster_as_the_processor_works_harder() {
        let beats = |load: f64| {
            let mut p = Pulse { cols: [0; W], beat: 0, since: 0, at: None };
            (0..140).filter(|_| p.next(load) == 1).count()
        };
        assert_eq!((beats(0.0), beats(100.0)), (10, 20));
    }
}
