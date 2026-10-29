//! Settings' About (ui/pages/about.slint): its words and facts from src/about.rs, the Blob
//! drawn behind them while About shows (src/blob.rs), a click poking it, Build with us and
//! the reads, and the Credits dialog with every author and model.

use super::*;
use crate::about::{self, Credits};
use crate::{AboutCredit, AboutFact, AboutLine};

/// How many of the authors, and of the models, About's foot names before "and N more".
const FIRST: usize = 3;

/// The Blob's frame is drawn a pixel to each of the page's units, however dense the screen:
/// a 1380 by 936 page's frame takes 37 ms of one core's time (src/blob.rs, blob_frame_time),
/// 30 frames a second about one core; a denser screen shows it a little softer.
const BLOB_PER_UNIT: f32 = 1.0;

/// About's own state: the credits (and their reading while they come), the Blob's clock
/// and pokes, the size of the frame drawn last, what the page was last handed, and whether
/// Credits is open.
#[derive(Default)]
pub(super) struct AboutState {
    credits: Option<Credits>,
    reading: Option<mpsc::Receiver<Result<Credits, String>>>,
    asked: bool,
    since: Option<Instant>,
    pokes: Vec<(Duration, [f32; 2])>,
    drawn: Option<(usize, usize, bool)>,
    shown: Option<(Vec<FactKey>, String, String)>,
    credits_open: bool,
}

/// A fact as handed to the page, to tell when it changes.
type FactKey = (String, Vec<(String, String)>, String, i32);

fn commits(n: u64) -> String {
    format!("{} commit{}", grouped(n), if n == 1 { "" } else { "s" })
}

/// A number with its thousands apart, as the history's numbers are read.
fn grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The facts at About's foot: its authors (the first few by commits, and how many more),
/// the models that wrote with them, what it's built on, and whose it is. Before the
/// credits come, or where there are none, the authors are its manifest's.
pub(crate) fn facts(credits: Option<&Credits>) -> Vec<FactKey> {
    let more = |n: usize| if n > FIRST { format!("and {} more", n - FIRST) } else { String::new() };
    let mut out = Vec::new();
    match credits {
        Some(c) => {
            out.push(("Authors".into(), c.authors.iter().take(FIRST).map(|a| (a.0.clone(), commits(a.1))).collect(), more(c.authors.len()), 1));
            if !c.models.is_empty() {
                out.push(("Models".into(), c.models.iter().take(FIRST).map(|m| (m.0.clone(), commits(m.1))).collect(), more(c.models.len()), 1));
            }
            out.push(("Built on".into(), vec![(format!("{} open-source libraries", grouped(c.libraries)), String::new()), (format!("{} typefaces", c.typefaces), String::new())], "View authors and licenses".into(), 2));
        }
        None => {
            out.push(("Authors".into(), about::authors().into_iter().take(FIRST).map(|a| (a.to_string(), String::new())).collect(), String::new(), 0));
            out.push(("Built on".into(), Vec::new(), "View authors and licenses".into(), 2));
        }
    }
    out.push(("Copyright".into(), vec![(format!("© {}", about::copyright()), String::new())], String::new(), 0));
    out
}

/// The GPLv3's notice in two lines as even as they come, so neither is left short.
pub(crate) fn even_lines(words: &str) -> Vec<String> {
    let w: Vec<&str> = words.split(' ').collect();
    let width = |s: &str| crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, s);
    (1..w.len())
        .map(|k| (w[..k].join(" "), w[k..].join(" ")))
        .min_by(|a, b| width(&a.0).max(width(&a.1)).total_cmp(&width(&b.0).max(width(&b.1))))
        .map_or_else(|| vec![words.to_string()], |(a, b)| vec![a, b])
}

impl Front {
    /// Whether About is in front: Settings open on its last tab.
    pub(crate) fn about_shows(&self) -> bool {
        self.ui.get_place() == "settings" && self.settings_tab == crate::ecto_settings::Tab::About
    }

    /// Whether About's Blob moves: About in front, and the system not asking for less motion.
    pub(crate) fn blob_moves(&self) -> bool {
        self.about_shows() && !crate::motion::reduced()
    }

    /// About's Blob, a frame drawn at its page's size (the screen's `scale` pixels a unit,
    /// at most BLOB_PER_UNIT): while it moves, each time it's asked; held still, once for
    /// each size. Whether one was drawn.
    pub(crate) fn draw_blob(&mut self, scale: f32) -> bool {
        if !self.about_shows() {
            return false;
        }
        let moving = !crate::motion::reduced();
        let (pw, ph) = (self.ui.get_content_width(), self.ui.get_about_height());
        let per = scale.min(BLOB_PER_UNIT);
        let (w, h) = (((pw * per).round() as usize).max(1), ((ph * per).round() as usize).max(1));
        if !moving && self.about.drawn == Some((w, h, false)) {
            return false;
        }
        let start = *self.about.since.get_or_insert_with(Instant::now);
        let real = crate::clock::animations(start);
        self.about.pokes.retain(|(at, _)| real.saturating_sub(*at) < crate::blob::POKE_LIFE);
        let pokes: Vec<crate::blob::Poke> = if moving { self.about.pokes.iter().map(|(at, p)| crate::blob::Poke { at: *p, age: real.saturating_sub(*at).as_secs_f32() }).collect() } else { Vec::new() };
        let ground = crate::tokens::ink::BASE;
        let look = crate::blob::Look::about(ground.map(|c| c as f32 / 255.0));
        // (held still, the Blob stands as it is a few seconds in, its highlight on it)
        let at = if moving { real.as_secs_f32() } else { 3.0 };
        let px = crate::blob::frame(&look, at, &pokes, w, h);
        let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&px, w as u32, h as u32);
        self.ui.set_about_blob(slint::Image::from_rgba8(buf));
        self.about.drawn = Some((w, h, moving));
        true
    }

    /// A click on About, at a place on its page `w` by `h`: a poke where it landed.
    pub(super) fn poke_blob(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if crate::motion::reduced() || w <= 0.0 || h <= 0.0 {
            return;
        }
        let start = *self.about.since.get_or_insert_with(Instant::now);
        let at = [x / w * 2.0 - 1.0, 1.0 - y / h * 2.0];
        self.about.pokes.push((crate::clock::animations(start), at));
        // (the Blob takes four at once; the oldest goes)
        if self.about.pokes.len() > 4 {
            self.about.pokes.remove(0);
        }
    }

    /// About's words and facts handed to the page, when they change; the credits asked for
    /// the first time About shows, and the system asked again whether to hold still.
    pub(super) fn show_about(&mut self) {
        if !self.about.asked {
            self.about.asked = true;
            self.about.reading = Some(about::credits());
            crate::motion::look();
        }
        self.credits_came();
        let facts = facts(self.about.credits.as_ref());
        let trouble = self.about_said.as_ref().map(|(_, e)| e.clone()).unwrap_or_default();
        let eyebrow = format!("Version {}", about::VERSION).to_uppercase();
        let now = (facts, trouble, eyebrow);
        if self.about.shown.as_ref() == Some(&now) {
            return;
        }
        let (facts, trouble, eyebrow) = &now;
        let model = |v: Vec<AboutFact>| ModelRc::new(VecModel::from(v));
        self.ui.set_about_facts(model(
            facts
                .iter()
                .map(|(head, lines, link, to)| AboutFact {
                    head: head.to_uppercase().into(),
                    lines: ModelRc::new(VecModel::from(lines.iter().map(|(t, a)| AboutLine { text: t.into(), aside: a.into() }).collect::<Vec<_>>())),
                    link: link.into(),
                    link_to: *to,
                })
                .collect(),
        ));
        let lines = |v: Vec<String>| ModelRc::new(VecModel::from(v.into_iter().map(SharedString::from).collect::<Vec<_>>()));
        self.ui.set_about_eyebrow(eyebrow.into());
        self.ui.set_about_lead(lines(about::LEAD.iter().map(|l| l.to_string()).collect()));
        self.ui.set_about_legal(lines(even_lines(&about::legal())));
        self.ui.set_about_trouble(trouble.into());
        self.about.shown = Some(now);
    }

    /// The credits, once they've been read: About and Credits shown with them. Whether
    /// they came now.
    pub(super) fn credits_came(&mut self) -> bool {
        let Some(rx) = &self.about.reading else { return false };
        let got = match rx.try_recv() {
            Ok(r) => r.ok(),
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => None,
        };
        self.about.reading = None;
        self.about.credits = got;
        true
    }

    /// Credits: every author and every model, by commits, and how many.
    pub(super) fn open_credits(&mut self) {
        let Some(c) = self.about.credits.clone() else { return };
        let share = |n: u64| {
            let p = 100.0 * n as f64 / c.commits.max(1) as f64;
            if p < 1.0 { "under 1% of commits".to_string() } else { format!("{}% of commits", p.round()) }
        };
        let authors: Vec<AboutCredit> = c.authors.iter().map(|a| AboutCredit { name: a.0.clone().into(), commits: commits(a.1).into(), more: format!("+{} / \u{2212}{}", grouped(a.2), grouped(a.3)).into() }).collect();
        let models: Vec<AboutCredit> = c.models.iter().map(|m| AboutCredit { name: m.0.clone().into(), commits: commits(m.1).into(), more: share(m.1).into() }).collect();
        let width = |v: &[AboutCredit], f: fn(&AboutCredit) -> &SharedString| v.iter().map(|r| crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, f(r)).ceil() + 1.0).fold(0.0, f32::max);
        let all: Vec<AboutCredit> = authors.iter().chain(&models).cloned().collect();
        self.ui.set_about_commits_w(width(&all, |r| &r.commits));
        self.ui.set_about_more_w(width(&all, |r| &r.more));
        let kinds = |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
        self.ui.set_about_credits_sub(format!("{} and {}, over {}.", kinds(authors.len(), "author"), kinds(models.len(), "model"), commits(c.commits)).into());
        self.ui.set_about_authors(ModelRc::new(VecModel::from(authors)));
        self.ui.set_about_models(ModelRc::new(VecModel::from(models)));
        self.ui.set_about_credits_open(true);
        self.about.credits_open = true;
    }

    pub(super) fn close_credits(&mut self) {
        self.ui.set_about_credits_open(false);
        self.about.credits_open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn made_up(people: usize, models: usize) -> Credits {
        Credits {
            authors: (0..people).map(|i| (format!("Person {i}"), 100 - i as u64, 1000, 10)).collect(),
            models: (0..models).map(|i| (format!("Model {i}"), 50 - i as u64)).collect(),
            commits: 120,
            libraries: 1234,
            typefaces: 7,
        }
    }

    /// About's foot names the first three authors and models by commits and says how many
    /// more there are; what it's built on comes from the credits' counts, never typed in.
    #[test]
    fn the_foot_names_the_first_three_and_counts_the_rest() {
        let f = facts(Some(&made_up(50, 2)));
        let heads: Vec<&str> = f.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(heads, ["Authors", "Models", "Built on", "Copyright"]);
        assert_eq!(f[0].1.len(), 3);
        assert_eq!(f[0].1[0], ("Person 0".to_string(), "100 commits".to_string()));
        assert_eq!((f[0].2.as_str(), f[0].3), ("and 47 more", 1));
        assert_eq!(f[1].2, "", "two models need no more");
        assert_eq!(f[2].1[0].0, "1,234 open-source libraries");
        assert_eq!(f[2].1[1].0, "7 typefaces");
        // (no models, no column for them)
        assert!(facts(Some(&made_up(2, 0))).iter().all(|x| x.0 != "Models"));
        // (before the credits come, the manifest's authors)
        let none = facts(None);
        assert!(about::authors().iter().all(|a| none[0].1.iter().any(|l| l.0 == *a)));
    }

    #[test]
    fn numbers_are_grouped_by_thousands() {
        assert_eq!(grouped(7), "7");
        assert_eq!(grouped(1234), "1,234");
        assert_eq!(grouped(224722), "224,722");
        assert_eq!(grouped(1000000), "1,000,000");
        assert_eq!(commits(1), "1 commit");
    }

    /// About gives what the GPLv3 asks an interactive program to show (its section 0): a
    /// copyright notice, that there's no warranty, that it may be shared under the license,
    /// and how to read the license (Read the license, about.slint, opening Read 0).
    #[test]
    fn about_gives_the_gplv3s_legal_notices() {
        let legal = about::legal();
        assert!(legal.contains("no warranty") && legal.contains("share and change Ectomatter") && legal.contains(about::license().0), "{legal}");
        let copyright = format!("© {}", about::copyright());
        for c in [None, Some(&made_up(2, 1))] {
            assert!(facts(c).iter().any(|f| f.1.iter().any(|l| l.0 == copyright)), "no copyright on the page");
        }
        let page = crate::boundaries::tests::read("apps/ectomatter/ui/pages/about.slint");
        assert!(page.contains("text: \"Read the license\";") && page.contains("clicked => { root.license(); }") && page.contains("for line in root.legal"), "the page doesn't show the notice or its Read");
        let front = crate::boundaries::tests::read("apps/ectomatter/src/front/mod.rs");
        assert!(front.contains("Ask::AboutLicense => 0"), "Read the license doesn't open the license");
    }

    /// The license's notice comes in two lines, neither much shorter than the other.
    #[test]
    fn the_legal_notice_comes_in_two_even_lines() {
        let l = even_lines(&about::legal());
        assert_eq!(l.len(), 2);
        assert_eq!(l.join(" "), about::legal());
        let w = |s: &str| crate::text::width(crate::tokens::font::REGULAR, crate::tokens::font::NOTE, s);
        assert!((w(&l[0]) - w(&l[1])).abs() < w("license, "), "{l:?}");
    }
}
