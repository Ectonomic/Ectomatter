//! The screen's launch and exit sequences, for game-helper's notes: the
//! z10-lcd service's z10anim.py, carried over line for line so the keyboard
//! shows the same frames. Each sequence runs on a thread of its own and hands
//! over one frame at a time, as a Python generator does, so what it asks of
//! game-helper's progress and the computer's readings is asked at the same
//! moments. Its random choices come from Python's own generator (Mersenne
//! Twister, with Python's seeding and its way of choosing), so a sequence
//! seeded alike comes out the same in both, character for character.
//!
//! The service drew on the Z10's twenty by four; each sequence is drawn here on
//! the screen the keyboard's core describes (lcd_page::size), and on twenty by
//! four draws the service's frames. A narrower or wider screen centers and cuts
//! to its own width; one with fewer rows leaves out what fits least (see
//! [Places]).

use std::sync::mpsc;
use std::sync::Arc;

/// A screen's size, its columns and rows (see [crate::lcd_page::size]).
pub type Size = (usize, usize);

const SCRAMBLE: &str = "#%&*+=?@$<>/|";
/// ms an animation frame stays up
const FRAME: u32 = 40;
/// ms a held frame stays up while game-helper is waited on
const WAIT_TICK: u32 = 100;
/// ms a sequence waits on game-helper, in all, before it stops waiting
const WAIT_LIMIT: u32 = 60000;
/// ms a screen stays up at the end of a sequence, at most, for what it shows until
/// (the game's window opening, a few seconds passing): a game whose window never
/// comes up holds the screen a minute, no more
const HOLD_LIMIT: u32 = 60000;

/// One frame: a row for each of the screen's, each as wide as the screen, how long it stays
/// up, and the step it belongs to.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub rows: Vec<String>,
    pub ms: u32,
    pub phase: &'static str,
}

// ------------------------------------------------------------------ Python's random

/// Python's `random.Random`: the Mersenne Twister, seeded as Python seeds it
/// from an integer, choosing and shuffling as Python does.
pub struct PyRandom {
    mt: [u32; 624],
    i: usize,
}

impl PyRandom {
    pub fn new(seed: u64) -> PyRandom {
        let mut r = PyRandom { mt: [0; 624], i: 624 };
        // the seed's 32-bit words, lowest first; zero is one word of zero
        let mut key = vec![seed as u32];
        if seed >> 32 != 0 {
            key.push((seed >> 32) as u32);
        }
        r.init_by_array(&key);
        r
    }

    fn init_genrand(&mut self, s: u32) {
        self.mt[0] = s;
        for i in 1..624 {
            self.mt[i] = 1812433253u32.wrapping_mul(self.mt[i - 1] ^ (self.mt[i - 1] >> 30)).wrapping_add(i as u32);
        }
        self.i = 624;
    }

    fn init_by_array(&mut self, key: &[u32]) {
        self.init_genrand(19650218);
        let (mut i, mut j) = (1usize, 0usize);
        for _ in 0..624.max(key.len()) {
            self.mt[i] = (self.mt[i] ^ (self.mt[i - 1] ^ (self.mt[i - 1] >> 30)).wrapping_mul(1664525)).wrapping_add(key[j]).wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= 624 {
                self.mt[0] = self.mt[623];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..623 {
            self.mt[i] = (self.mt[i] ^ (self.mt[i - 1] ^ (self.mt[i - 1] >> 30)).wrapping_mul(1566083941)).wrapping_sub(i as u32);
            i += 1;
            if i >= 624 {
                self.mt[0] = self.mt[623];
                i = 1;
            }
        }
        self.mt[0] = 0x8000_0000;
    }

    fn next_u32(&mut self) -> u32 {
        if self.i >= 624 {
            for k in 0..624 {
                let y = (self.mt[k] & 0x8000_0000) | (self.mt[(k + 1) % 624] & 0x7fff_ffff);
                self.mt[k] = self.mt[(k + 397) % 624] ^ (y >> 1) ^ if y & 1 != 0 { 0x9908_b0df } else { 0 };
            }
            self.i = 0;
        }
        let mut y = self.mt[self.i];
        self.i += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// `getrandbits(k)` for k up to 32
    fn bits(&mut self, k: u32) -> u32 {
        self.next_u32() >> (32 - k)
    }

    /// `_randbelow(n)`: a number from 0 to n - 1
    pub fn below(&mut self, n: usize) -> usize {
        let k = usize::BITS - n.leading_zeros();
        loop {
            let r = self.bits(k) as usize;
            if r < n {
                return r;
            }
        }
    }

    pub fn choice<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len())]
    }

    pub fn shuffle<T>(&mut self, xs: &mut [T]) {
        for i in (1..xs.len()).rev() {
            let j = self.below(i + 1);
            xs.swap(i, j);
        }
    }
}

// ------------------------------------------------------------------ text, as Python slices it

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

/// `s[a:b]`, by characters, with Python's clamping
fn slice(s: &str, a: usize, b: usize) -> String {
    s.chars().skip(a).take(b.saturating_sub(a)).collect()
}

fn ljust(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w { s.to_string() } else { format!("{s}{}", " ".repeat(w - n)) }
}

/// `s[:w].ljust(w)`, a row `w` wide
fn cell(s: &str, w: usize) -> String {
    ljust(&slice(s, 0, w), w)
}

/// A screen of `size` with nothing on it.
fn blank((w, h): Size) -> Vec<String> {
    vec![" ".repeat(w); h]
}

/// `text[:w].center(w)`, as Python centers: with an even width the extra space goes right
pub fn center(text: &str, w: usize) -> String {
    let t = slice(text, 0, w);
    let n = t.chars().count();
    let marg = w - n;
    let left = marg / 2 + (marg & w & 1);
    format!("{}{t}{}", " ".repeat(left), " ".repeat(marg - left))
}

/// The bar of the changes done, a row `w` wide: as many cells to fill as leave room after it
/// for a count of one figure each (fourteen on the Z10's twenty), then the count.
pub fn bar(done: usize, total: usize, w: usize) -> String {
    let cells = w.saturating_sub(6);
    if total == 0 {
        return " ".repeat(w);
    }
    let fill = ((cells * done) as f64 / total as f64).round_ties_even() as usize;
    cell(&format!("[{}{}] {done}/{total}", "#".repeat(fill), ".".repeat(cells.saturating_sub(fill))), w)
}

pub fn played(minutes: i64) -> String {
    if minutes != 0 { format!("PLAYED {}H {:02}M", minutes.div_euclid(60), minutes.rem_euclid(60)) } else { "PLAYED UNDER 1 MIN".into() }
}

/// `fit(rows)`: the screen's rows, each its width
pub fn fit(rows: &[String], (w, h): Size) -> Vec<String> {
    (0..h).map(|i| cell(rows.get(i).map_or("", |s| s.as_str()), w)).collect()
}

/// Where a sequence's rows go on a screen: its title ("> LAUNCHING"), the game's name, what is
/// being set (or the time played) and the bar of the changes done, the Z10's four rows. A
/// screen of three rows leaves out what is being set; of two, the title too; of one, the bar
/// too, so the name and the bar show longest. A taller screen keeps the four at its top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Places {
    pub title: Option<usize>,
    pub name: Option<usize>,
    pub step: Option<usize>,
    pub bar: Option<usize>,
}

impl Places {
    pub fn on(rows: usize) -> Places {
        let (title, name, step, bar) = match rows {
            0 => (None, None, None, None),
            1 => (None, Some(0), None, None),
            2 => (None, Some(0), None, Some(1)),
            3 => (Some(0), Some(1), None, Some(2)),
            _ => (Some(0), Some(1), Some(2), Some(3)),
        };
        Places { title, name, step, bar }
    }
}

/// Row `r` of the screen made `text`, where the screen has that row.
fn set(rows: &mut [String], r: Option<usize>, text: String) {
    if let Some(row) = r.and_then(|r| rows.get_mut(r)) {
        *row = text;
    }
}

// ------------------------------------------------------------------ one page to the next

/// The frames that carry the screen from the page shown (`old`) to the next
/// (`new`), as `how` brings a page in, on a screen of `size`, built as the launch sequence
/// builds its own: the wipe the new page sweeping in from the left over the old, a
/// column every 25 ms (with no # at its edge: the keyboard's screen is too slow
/// to show a cell that changes again so soon, so it showed only in Ectomatter),
/// the dissolve the cells changing in a random order over 16 frames (35 ms
/// each), Type the screen cleared and the page's words typed in row by row
/// behind a cursor (30 ms a letter). Cut has none: the page shows at once.
/// The new page itself is not among them; it follows the last.
pub fn page_turn(old: &[String], new: &[String], how: crate::lcd_page::Transition, seed: u64, size: Size) -> Vec<Frame> {
    use crate::lcd_page::Transition;
    let (w, h) = size;
    let (old, new) = (fit(old, size), fit(new, size));
    let mut out = Vec::new();
    let mut add = |rows: Vec<String>, ms: u32| out.push(Frame { rows, ms, phase: "Turn" });
    match how {
        Transition::Cut => {}
        Transition::Wipe => {
            // (the old page first, then a column more of the new one each frame)
            for x in 0..w {
                let rows = old.iter().zip(&new).map(|(o, n)| cell(&format!("{}{}", slice(n, 0, x), slice(o, x, usize::MAX)), w)).collect();
                add(rows, 25);
            }
        }
        Transition::Dissolve => {
            let mut rng = PyRandom::new(seed);
            let (oc, nc): (Vec<Vec<char>>, Vec<Vec<char>>) = (old.iter().map(|r| chars(r)).collect(), new.iter().map(|r| chars(r)).collect());
            let mut cells: Vec<(usize, usize)> = (0..h).flat_map(|r| (0..w).map(move |c| (r, c))).filter(|&(r, c)| oc[r][c] != nc[r][c]).collect();
            rng.shuffle(&mut cells);
            let mut grid = oc.clone();
            let per = cells.len().div_ceil(16).max(1);
            // (the last cells change as the page itself shows)
            for chunk in cells.chunks(per).take(cells.len().div_ceil(per).saturating_sub(1)) {
                for &(r, c) in chunk {
                    grid[r][c] = nc[r][c];
                }
                add(grid.iter().map(|g| g.iter().collect()).collect(), 35);
            }
        }
        Transition::Type => {
            let mut rows = blank(size);
            add(rows.clone(), 60);
            for (r, line) in new.iter().enumerate() {
                let text = line.trim_end();
                let n = text.chars().count();
                for i in 1..=n {
                    // a space is typed with the letter after it
                    if text.chars().nth(i - 1) == Some(' ') {
                        continue;
                    }
                    rows[r] = cell(&format!("{}_", slice(text, 0, i)), w);
                    add(rows.clone(), 30);
                }
                rows[r] = cell(text, w);
            }
        }
    }
    out
}

// ------------------------------------------------------------------ a sequence, frame by frame

/// The sequence was dropped before it finished: stop where it stands.
pub struct Stopped;
type Go<T> = Result<T, Stopped>;

/// What a running sequence asks of the world, each when it asks.
pub type Ask<T> = Arc<dyn Fn() -> T + Send + Sync>;

/// The sequence's side of the hand-over.
struct Yield {
    out: mpsc::SyncSender<Frame>,
    resume: mpsc::Receiver<()>,
}

struct Seq {
    y: Yield,
    rng: PyRandom,
    t: u32,
    cursor: Option<(usize, String)>,
    waited: u32,
    /// the screen it draws on, and where its rows go there
    size: Size,
    at: Places,
}

impl Seq {
    /// Hand a frame over, and wait to be asked for the next.
    fn add(&mut self, rows: &[String], ms: u32, phase: &'static str) -> Go<()> {
        let mut shown = fit(rows, self.size);
        if let Some((r, text)) = &self.cursor {
            let blink = if (self.t / 400) % 2 == 0 { "_" } else { "" };
            set(&mut shown, Some(*r), ljust(&format!("{text}{blink}"), self.size.0));
        }
        self.t += ms;
        self.y.out.send(Frame { rows: shown, ms, phase }).map_err(|_| Stopped)?;
        self.y.resume.recv().map_err(|_| Stopped)
    }

    /// A column of # sweeps across; the screen behind it is the new one.
    fn wipe(&mut self, old: &[String], new: &[String], phase: &'static str, leftward: bool) -> Go<Vec<String>> {
        let w = self.size.0;
        let (old, new) = (fit(old, self.size), fit(new, self.size));
        let xs: Vec<i32> = if leftward { (-1..w as i32).rev().collect() } else { (-1..w as i32).collect() };
        for x in xs {
            let mut rows = Vec::new();
            for (o, n) in old.iter().zip(&new) {
                let mark = if (0..w as i32).contains(&x) { "#" } else { "" };
                let cut = x.max(0) as usize;
                let rest = (x + 1) as usize;
                let row = if leftward {
                    format!("{}{mark}{}", slice(o, 0, cut), slice(n, rest, usize::MAX))
                } else {
                    format!("{}{mark}{}", slice(n, 0, cut), slice(o, rest, usize::MAX))
                };
                rows.push(cell(&row, w));
            }
            self.add(&rows, 25, phase)?;
        }
        Ok(new)
    }

    /// Letters appear one at a time with a block cursor after them, on row `r` where the
    /// screen has it (the frames come as long, so the sequence keeps its time).
    fn type_(&mut self, rows: &[String], r: Option<usize>, text: &str, phase: &'static str, ms: u32) -> Go<Vec<String>> {
        let mut rows = rows.to_vec();
        for i in 0..=text.chars().count() {
            set(&mut rows, r, ljust(&format!("{}_", slice(text, 0, i)), self.size.0));
            self.add(&rows, ms, phase)?;
        }
        Ok(rows)
    }

    /// Every letter flickers through random glyphs, then locks in left to right.
    fn decode(&mut self, rows: &[String], r: Option<usize>, target: &str, phase: &'static str, per_lock: usize, lead: usize) -> Go<Vec<String>> {
        let mut rows = rows.to_vec();
        let target = chars(&center(target, self.size.0));
        let slots: Vec<usize> = (0..target.len()).filter(|&i| target[i] != ' ').collect();
        let steps = lead + per_lock * slots.len();
        let scramble = chars(SCRAMBLE);
        for f in 0..=steps {
            let locked = if f >= lead { (f - lead) / per_lock + 1 } else { 0 };
            let mut out = target.clone();
            for (k, &i) in slots.iter().enumerate() {
                if k >= locked {
                    out[i] = self.rng.choice(&scramble);
                }
            }
            set(&mut rows, r, out.iter().collect());
            self.add(&rows, FRAME, phase)?;
        }
        Ok(rows)
    }

    /// Cells switch to the target one by one in random order.
    fn dissolve(&mut self, rows: &[String], target: &[String], phase: &'static str) -> Go<Vec<String>> {
        let frames = 16;
        let (w, h) = self.size;
        let (rows, target) = (fit(rows, self.size), fit(target, self.size));
        let (rc, tc): (Vec<Vec<char>>, Vec<Vec<char>>) = (rows.iter().map(|s| chars(s)).collect(), target.iter().map(|s| chars(s)).collect());
        let mut cells: Vec<(usize, usize)> = (0..h).flat_map(|r| (0..w).map(move |c| (r, c))).filter(|&(r, c)| rc[r][c] != tc[r][c]).collect();
        self.rng.shuffle(&mut cells);
        let mut grid = rc.clone();
        let per = cells.len().div_ceil(frames).max(1);
        let mut i = 0;
        while i < cells.len() {
            for &(r, c) in &cells[i..(i + per).min(cells.len())] {
                grid[r][c] = tc[r][c];
            }
            let shown: Vec<String> = grid.iter().map(|g| g.iter().collect()).collect();
            self.add(&shown, 35, phase)?;
            i += per;
        }
        Ok(target)
    }

    fn blink_cursor(&mut self, rows: &[String], r: Option<usize>, text: &str, ms: u32, phase: &'static str, times: usize) -> Go<Vec<String>> {
        let mut rows = rows.to_vec();
        for _ in 0..times {
            set(&mut rows, r, ljust(&format!("{text}_"), self.size.0));
            self.add(&rows, ms, phase)?;
            set(&mut rows, r, ljust(text, self.size.0));
            self.add(&rows, ms, phase)?;
        }
        Ok(rows)
    }

    /// Hold the screen as it is until `ready`, at most HOLD_LIMIT (none at all when
    /// it's ready at once, so a sequence that isn't held has the frames it always had).
    fn hold(&mut self, ready: &dyn Fn() -> bool, rows: &[String], phase: &'static str) -> Go<()> {
        let mut held = 0;
        while !ready() && held < HOLD_LIMIT {
            self.add(rows, WAIT_TICK, phase)?;
            held += WAIT_TICK;
        }
        Ok(())
    }

    /// Hold the screen (the cursor still blinking) until game-helper catches up.
    fn wait(&mut self, ready: &dyn Fn() -> bool, rows: &[String], phase: &'static str, animate: &mut dyn FnMut(&mut Vec<String>)) -> Go<()> {
        while !ready() && self.waited < WAIT_LIMIT {
            let mut shown = rows.to_vec();
            animate(&mut shown);
            self.add(&shown, WAIT_TICK, phase)?;
            self.waited += WAIT_TICK;
        }
        Ok(())
    }
}

/// The game's launch: `steps` says what is being set (asked as the bar is first
/// drawn, and again as the steps begin, as a note may come in between), and
/// `progress` how many are done; `ready` whether the screen may go on from
/// READY; `stats` gives the rows to dissolve into at the end.
#[allow(clippy::too_many_arguments)]
fn launch(s: &mut Seq, game: &str, steps: Ask<Vec<String>>, progress: Ask<i64>, ready: Ask<bool>, stats: Ask<Vec<String>>, start: Vec<String>) -> Go<()> {
    let (at, w) = (s.at, s.size.0);
    let mut rows = s.wipe(&start, &blank(s.size), "Wipe", false)?;
    rows = s.type_(&rows, at.title, "> LAUNCHING", "Announce", 45)?;
    s.cursor = at.title.map(|r| (r, "> LAUNCHING".into()));
    set(&mut rows, at.bar, bar(0, steps().len(), w));
    s.add(&rows, FRAME, "Announce")?;
    let name = game.to_uppercase();
    rows = s.decode(&rows, at.name, &name, "Decode", 2, 4)?;
    // a name wider than the screen scrolls while the steps run
    let n = name.chars().count();
    let lp = format!("{name}   ");
    let lp_n = lp.chars().count();
    let twice = format!("{lp}{lp}");
    let mut off = 0usize;
    let mut scroll = move |rows: &mut Vec<String>| {
        if n > w {
            set(rows, at.name, slice(&slice(&twice, off % lp_n, usize::MAX), 0, w));
            off += 1;
        }
    };
    let steps = steps();
    for (i, label) in steps.iter().enumerate() {
        let i = i + 1;
        set(&mut rows, at.step, format!("  {label}"));
        set(&mut rows, at.bar, bar(i - 1, steps.len(), w));
        scroll(&mut rows);
        s.add(&rows, 130, "Steps")?;
        let p = progress.clone();
        s.wait(&move || p() >= i as i64, &rows.clone(), "Steps", &mut scroll)?;
        for _ in 0..2 {
            set(&mut rows, at.bar, bar(i, steps.len(), w));
            scroll(&mut rows);
            s.add(&rows, 130, "Steps")?;
        }
    }
    s.cursor = None;
    set(&mut rows, at.title, "> READY".into());
    set(&mut rows, at.step, String::new());
    set(&mut rows, at.name, center(&name, w));
    for _ in 0..3 {
        s.add(&rows, 160, "Ready")?;
        let mut dark = rows.clone();
        set(&mut dark, at.bar, String::new());
        s.add(&dark, 120, "Ready")?;
    }
    s.add(&rows, 700, "Ready")?;
    s.hold(&*ready, &rows, "Ready")?;
    let target = stats();
    s.dissolve(&rows, &target, "Dissolve")?;
    Ok(())
}

/// The game's exit: `steps` says what is being put back (asked as the bar
/// begins), `progress` how many are done, `finished` whether the screen may
/// go on (everything put back, or its time up).
#[allow(clippy::too_many_arguments)]
fn exit(s: &mut Seq, game: &str, steps: Ask<Vec<String>>, minutes: i64, progress: Ask<i64>, finished: Ask<bool>, stats: Ask<Vec<String>>, start: Vec<String>) -> Go<()> {
    let (at, w) = (s.at, s.size.0);
    let mut rows = s.wipe(&start, &blank(s.size), "Wipe", true)?;
    rows = s.type_(&rows, at.title, "> SESSION ENDED", "Announce", 45)?;
    s.cursor = at.title.map(|r| (r, "> SESSION ENDED".into()));
    rows = s.decode(&rows, at.name, &game.to_uppercase(), "Name", 1, 2)?;
    let n = 18;
    for f in 0..=n {
        // counts up, slowing into the total
        let m = (minutes as f64 * (1.0 - (1.0 - f as f64 / n as f64).powf(3.0))).round_ties_even() as i64;
        set(&mut rows, at.step, center(&if f < n { played(m) } else { played(minutes) }, w));
        s.add(&rows, FRAME, "Playtime")?;
    }
    let steps = steps();
    for i in 0..=steps.len() {
        let p = progress.clone();
        s.wait(&move || p() >= i as i64, &rows.clone(), "Restore", &mut |_| {})?;
        set(&mut rows, at.bar, bar(i, steps.len(), w));
        s.add(&rows, 180, "Restore")?;
    }
    let f = finished.clone();
    s.wait(&move || f(), &rows.clone(), "Restore", &mut |_| {})?;
    s.cursor = None;
    rows = s.blink_cursor(&rows, at.title, "> SESSION ENDED", 300, "Hold", 3)?;
    let target = stats();
    s.dissolve(&rows, &target, "Dissolve")?;
    Ok(())
}

/// A sequence playing: ask it for each frame in turn.
pub struct Playing {
    frames: mpsc::Receiver<Frame>,
    resume: mpsc::Sender<()>,
}

impl Playing {
    fn spawn(seed: u64, size: Size, body: impl FnOnce(&mut Seq) -> Go<()> + Send + 'static) -> Playing {
        let (out, frames) = mpsc::sync_channel(0);
        let (resume, resumed) = mpsc::channel();
        std::thread::spawn(move || {
            let mut s = Seq { y: Yield { out, resume: resumed }, rng: PyRandom::new(seed), t: 0, cursor: None, waited: 0, size, at: Places::on(size.1) };
            // nothing runs until the first frame is asked for, as with a generator
            if s.y.resume.recv().is_err() {
                return;
            }
            let _ = body(&mut s);
        });
        Playing { frames, resume }
    }

    /// The game's launch on a screen of `size` (see [launch]).
    #[allow(clippy::too_many_arguments)]
    pub fn launch(seed: u64, size: Size, game: String, steps: Ask<Vec<String>>, progress: Ask<i64>, ready: Ask<bool>, stats: Ask<Vec<String>>, start: Vec<String>) -> Playing {
        Playing::spawn(seed, size, move |s| launch(s, &game, steps, progress, ready, stats, start))
    }

    /// The game's exit on a screen of `size` (see [exit]).
    #[allow(clippy::too_many_arguments)]
    pub fn exit(seed: u64, size: Size, game: String, steps: Ask<Vec<String>>, minutes: i64, progress: Ask<i64>, finished: Ask<bool>, stats: Ask<Vec<String>>, start: Vec<String>) -> Playing {
        Playing::spawn(seed, size, move |s| exit(s, &game, steps, minutes, progress, finished, stats, start))
    }

    /// A screen layout shown as a game starts or quits, in place of the sequences, on a screen
    /// of `size`: it comes in as a sequence does, shows (drawn afresh each tenth of a second,
    /// for its readings) until `until`, then dissolves into the playlist's page.
    pub fn layout(seed: u64, size: Size, draw: Ask<Vec<String>>, until: Ask<bool>, leftward: bool, stats: Ask<Vec<String>>, start: Vec<String>) -> Playing {
        Playing::spawn(seed, size, move |s| {
            let mut rows = s.wipe(&start, &draw(), "Wipe", leftward)?;
            let mut held = 0;
            while !until() && held < HOLD_LIMIT {
                rows = draw();
                s.add(&rows, WAIT_TICK, "Layout")?;
                held += WAIT_TICK;
            }
            let target = stats();
            s.dissolve(&rows, &target, "Dissolve")?;
            Ok(())
        })
    }

    /// The next frame, or None once the sequence has ended.
    pub fn next(&mut self) -> Option<Frame> {
        // the first ask starts it; each after lets it carry on from its last frame
        self.resume.send(()).ok()?;
        self.frames.recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Z10's screen, the service's.
    const Z10: Size = (20, 4);

    fn rows(s: &[&str]) -> Vec<String> {
        s.iter().map(|r| r.to_string()).collect()
    }

    #[test]
    fn one_page_comes_in_after_another() {
        use crate::lcd_page::Transition;
        let old = rows(&["CPU 64°  5% 4365MHz", "GPU 45° 12% 1837MHz", "", ""]);
        let new = rows(&["", "       21:05", "     Fri Oct 9", ""]);
        // Cut: no frames between, the page shows at once
        assert!(page_turn(&old, &new, Transition::Cut, 1, Z10).is_empty());
        // Wipe: the old page first, then the new page sweeping in from the left a column a frame
        let w = page_turn(&old, &new, Transition::Wipe, 1, Z10);
        assert_eq!(w.len(), 20);
        assert_eq!(w[0].rows, fit(&old, Z10));
        assert_eq!(w[1].rows[0], format!(" {}", &fit(&old, Z10)[0][1..]));
        assert_eq!(w[8].rows[1], format!("       2{}", slice(&fit(&old, Z10)[1], 8, 20)));
        assert_eq!(w[19].rows[1], format!("{}{}", slice(&fit(&new, Z10)[1], 0, 19), slice(&fit(&old, Z10)[1], 19, 20)));
        assert!(w.iter().all(|f| f.ms == 25 && f.rows.iter().all(|r| !r.contains('#'))));
        // Dissolve: each frame closer to the new page, every cell either the old or the new
        let d = page_turn(&old, &new, Transition::Dissolve, 7, Z10);
        // (at most 15: the sixteenth step is the page itself)
        assert!(!d.is_empty() && d.len() <= 15 && d.iter().all(|f| f.ms == 35), "{} frames", d.len());
        let differ = |rows: &[String]| (0..4).map(|r| chars(&rows[r]).iter().zip(chars(&fit(&new, Z10)[r])).filter(|(a, b)| **a != *b).count()).sum::<usize>();
        for k in 1..d.len() {
            assert!(differ(&d[k].rows) < differ(&d[k - 1].rows));
        }
        assert_eq!(d, page_turn(&old, &new, Transition::Dissolve, 7, Z10), "the same seed, the same order");
        // Type: a blank screen, then each row's words letter by letter behind a cursor
        let t = page_turn(&old, &new, Transition::Type, 1, Z10);
        assert_eq!(t[0].rows, blank(Z10));
        assert_eq!(t[1].rows[1], cell("       2_", 20));
        assert_eq!(t.last().unwrap().rows[2], cell("     Fri Oct 9_", 20));
        assert_eq!(t.last().unwrap().rows[1], cell("       21:05", 20));
        // 5 letters of the time, 7 of the date (its spaces come with the letters after them)
        assert_eq!(t.len(), 1 + 5 + 7);
    }

    #[test]
    fn a_page_comes_in_on_a_screen_of_any_size() {
        use crate::lcd_page::{Transition, TRANSITIONS};
        let old = rows(&["CPU 64°  5% 4365MHz", "GPU 45° 12% 1837MHz", "", ""]);
        let new = rows(&["", "       21:05", "     Fri Oct 9", ""]);
        for size in [(16, 2), (40, 4), (20, 8), (1, 1), (0, 0)] {
            for how in TRANSITIONS {
                for f in page_turn(&old, &new, how, 3, size) {
                    assert_eq!(f.rows.len(), size.1, "{how:?} on {size:?}");
                    assert!(f.rows.iter().all(|r| r.chars().count() == size.0), "{how:?} on {size:?}: {:?}", f.rows);
                }
            }
            // (a wipe a frame a column)
            assert_eq!(page_turn(&old, &new, Transition::Wipe, 3, size).len(), size.0);
        }
        // on two rows of sixteen, the two rows of each page, cut to sixteen
        assert_eq!(page_turn(&old, &new, Transition::Wipe, 3, (16, 2))[0].rows, ["CPU 64°  5% 4365", "GPU 45° 12% 1837"]);
    }

    #[test]
    fn pythons_random_comes_out_the_same() {
        // python3 -c "import random; r=random.Random(1); print([r.getrandbits(32) for _ in range(3)], [r.choice('abcdefghijklm') for _ in range(5)])"
        let mut r = PyRandom::new(1);
        assert_eq!([r.next_u32(), r.next_u32(), r.next_u32()], [577090037, 2444712010, 3639700191]);
        let abc: Vec<char> = "abcdefghijklm".chars().collect();
        let picks: String = (0..5).map(|_| r.choice(&abc)).collect();
        assert_eq!(picks, "mmbeb");
        let mut r = PyRandom::new((1 << 40) + 5);
        assert_eq!([r.next_u32(), r.next_u32()], [2166296868, 2220160828]);
        let mut r = PyRandom::new(0);
        assert_eq!([r.next_u32(), r.next_u32()], [3626764237, 1654615998]);
    }

    #[test]
    fn python_centers_and_bars_alike() {
        assert_eq!(center("ABC", 20), "        ABC         ");
        assert_eq!(bar(1, 3, 20), "[#####.........] 1/3");
        assert_eq!(bar(0, 0, 20), " ".repeat(20));
        assert_eq!(played(0), "PLAYED UNDER 1 MIN");
        assert_eq!(played(125), "PLAYED 2H 05M");
        // and on other widths: centered in them, the bar as long as leaves room for its count
        assert_eq!(center("ABC", 16), "      ABC       ");
        assert_eq!(center("ABCDEFGHIJ", 4), "ABCD");
        assert_eq!(bar(1, 3, 16), "[###.......] 1/3");
        assert_eq!(bar(3, 3, 40), format!("[{}] 3/3", "#".repeat(34)));
        assert_eq!(bar(1, 3, 3), "[] ");
    }

    #[test]
    fn the_sequences_put_their_rows_where_the_screen_has_them() {
        assert_eq!(Places::on(4), Places { title: Some(0), name: Some(1), step: Some(2), bar: Some(3) });
        assert_eq!(Places::on(8), Places::on(4));
        assert_eq!(Places::on(3), Places { title: Some(0), name: Some(1), step: None, bar: Some(2) });
        assert_eq!(Places::on(2), Places { title: None, name: Some(0), step: None, bar: Some(1) });
        assert_eq!(Places::on(1), Places { title: None, name: Some(0), step: None, bar: None });
        assert_eq!(Places::on(0), Places { title: None, name: None, step: None, bar: None });
        // a launch and an exit played whole on screens of other sizes: every frame the screen's
        // size, the name centered where it goes, the bar on its row; on no screen, frames of nothing
        let steps: Ask<Vec<String>> = Arc::new(|| vec!["MONITOR TO 120 HZ".into(), "HDR ON".into()]);
        let done: Ask<i64> = Arc::new(|| 9);
        let yes: Ask<bool> = Arc::new(|| true);
        for size in [(16, 2), (40, 4), (20, 8), (20, 3), (1, 1), (0, 0)] {
            let stats: Ask<Vec<String>> = Arc::new(move || fit(&rows(&["CPU 64°", "GPU 45°", "RAM", "VRAM"]), size));
            let start = blank(size);
            let at = Places::on(size.1);
            for (kind, mut p) in [
                ("launch", Playing::launch(4, size, "Hades".into(), steps.clone(), done.clone(), yes.clone(), stats.clone(), start.clone())),
                ("exit", Playing::exit(4, size, "Hades".into(), steps.clone(), 75, done.clone(), yes.clone(), stats.clone(), start.clone())),
            ] {
                let mut frames = Vec::new();
                while let Some(f) = p.next() {
                    assert_eq!(f.rows.len(), size.1, "{kind} on {size:?}");
                    assert!(f.rows.iter().all(|r| r.chars().count() == size.0), "{kind} on {size:?}: {:?}", f.rows);
                    frames.push(f);
                }
                assert!(frames.len() > 20, "{kind} on {size:?}: {} frames", frames.len());
                if let (Some(n), true) = (at.name, size.0 >= 5) {
                    assert!(frames.iter().any(|f| f.rows[n] == center("HADES", size.0)), "{kind} on {size:?}: the name never showed");
                }
                if let Some(b) = at.bar {
                    assert!(frames.iter().any(|f| f.rows[b].starts_with('[')), "{kind} on {size:?}: the bar never showed");
                }
                assert_eq!(frames.last().unwrap().rows, stats(), "{kind} on {size:?}: it ends on the page");
            }
        }
    }
}
