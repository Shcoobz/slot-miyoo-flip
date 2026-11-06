//! The Activity report (`design_handoff_extra_shelf`, screen 3a): A on the receipt slides a sheet
//! of the card's play time up over the Extras shelf while the shelf fades and its neighbours step
//! aside; ↑/↓ scroll it and B puts it all back.

use slot_input::{Action, Btn};
use slot_ui::{ease, Draw, Millis, ReportRow, ReportSheet, TexId};

use super::{ease_out, glide_at, system_secs, App, MONTHS};

/// The sheet comes up from below the screen to 36 px from the top, 200 ms after the shelf begins
/// to fade (design: top 520 → 36 over 520 ms); it goes back at once.
const SHEET_FROM: f32 = 520.0;
const SHEET_AT: f32 = 36.0;
const SHEET_MS: f32 = 520.0;
const SHEET_DELAY_MS: f32 = 200.0;
/// The shelf fades over 360 ms, and comes back 300 ms into the close.
const SHELF_MS: f32 = 360.0;
const SHELF_BACK_MS: f32 = 300.0;
/// The neighbours step 260 px further out as they fade, and come back 400 ms into the close.
const ASIDE: f32 = 260.0;
const ASIDE_MS: f32 = 420.0;
const ASIDE_BACK_MS: f32 = 400.0;
/// The legend shows once the sheet is nearly up, and goes at once.
const LEGEND_MS: f32 = 300.0;
const LEGEND_DELAY_MS: f32 = 600.0;
/// The close is over when the neighbours are back.
const CLOSE_MS: f32 = ASIDE_BACK_MS + ASIDE_MS;
/// One press of ↑/↓.
const STEP: f32 = 60.0;
/// The design lists three games last played and six most played.
const LAST_SHOWN: usize = 3;
const MOST_SHOWN: usize = 6;

/// The report and its faces: the sheet is printed again each time it opens, as play goes on.
#[derive(Default)]
pub struct Report {
    shown: Option<Shown>,
    stale: bool,
    /// The printed sheet and its height in texels.
    sheet: Option<(TexId, u32)>,
    frame: Option<TexId>,
    legend: Vec<(TexId, u32)>,
}

#[derive(Copy, Clone, Debug)]
struct Shown {
    /// On the second page, every game's play time.
    per_game: bool,
    opened: Millis,
    closing: Option<Millis>,
    /// How far down the sheet the window is meant to be, in design px, and where its glide there
    /// set off from.
    scroll: f32,
    glide: (f32, Millis),
}

impl Shown {
    /// How far one part of the opening has come at `now`, 0 to 1, before easing: it starts
    /// `delay` ms after A and takes `ms`. A close takes it back from wherever it had got to,
    /// starting `back` ms after B.
    fn part(&self, now: Millis, delay: f32, back: f32, ms: f32) -> f32 {
        let along = |since: Millis| ((since as f32 - delay) / ms).clamp(0.0, 1.0);
        match self.closing {
            None => along(now.saturating_sub(self.opened)),
            Some(at) => {
                let from = along(at.saturating_sub(self.opened));
                let u = ((now.saturating_sub(at) as f32 - back) / ms).clamp(0.0, 1.0);
                from * (1.0 - u)
            }
        }
    }
}

impl App {
    /// A on the receipt.
    pub(super) fn open_report(&mut self) {
        if self.report.shown.is_some() {
            return;
        }
        if self.root.is_none() {
            return self.refuse();
        }
        let now = self.now();
        self.report.shown = Some(Shown {
            per_game: false,
            opened: now,
            closing: None,
            scroll: 0.0,
            glide: (0.0, now),
        });
        self.report.stale = true;
    }

    /// Whether the report has the shelf's buttons: from A until B.
    pub(super) fn report_open(&self) -> bool {
        self.report.shown.is_some_and(|s| s.closing.is_none())
    }

    pub(super) fn report_input(&mut self, action: Action) {
        let now = self.now();
        let most = self
            .report
            .sheet
            .map_or(0.0, |(_, h)| slot_ui::report_scroll_max(h));
        let Some(s) = &mut self.report.shown else {
            return;
        };
        let step = match action {
            Action::GbaDown(Btn::Up) => -STEP,
            Action::GbaDown(Btn::Down) => STEP,
            // A turns to the play time per game; B from there comes back to the report.
            Action::GbaDown(Btn::A) if !s.per_game => {
                s.per_game = true;
                s.scroll = 0.0;
                s.glide = (0.0, now);
                self.report.stale = true;
                return;
            }
            Action::GbaDown(Btn::B) if s.per_game => {
                s.per_game = false;
                s.scroll = 0.0;
                s.glide = (0.0, now);
                self.report.stale = true;
                return;
            }
            Action::GbaDown(Btn::B) => {
                s.closing = Some(now);
                return;
            }
            _ => return,
        };
        let to = (s.scroll + step).clamp(0.0, most);
        if to != s.scroll {
            s.glide = (glide_at(s.glide, s.scroll, now), now);
            s.scroll = to;
        }
    }

    /// Once the close has played out the report is gone, and the shelf's buttons are its own.
    pub(super) fn finish_report(&mut self) {
        let Some(Shown {
            closing: Some(at), ..
        }) = self.report.shown
        else {
            return;
        };
        if self.now().saturating_sub(at) as f32 >= CLOSE_MS {
            self.report.shown = None;
        }
    }

    /// How an Extras item `offset` places from the selected one stands while the report is out:
    /// how far it has stepped aside, and how much of it shows.
    pub(super) fn report_aside(&self, offset: f32) -> (f32, f32) {
        let Some(s) = &self.report.shown else {
            return (0.0, 1.0);
        };
        let now = self.now();
        let shelf = 1.0 - ease(s.part(now, 0.0, SHELF_BACK_MS, SHELF_MS));
        if offset.abs() < 0.5 {
            return (0.0, shelf);
        }
        let k = ease(s.part(now, 0.0, ASIDE_BACK_MS, ASIDE_MS));
        (offset.signum() * ASIDE * k, shelf * (1.0 - k))
    }

    /// The report over the Extras shelf, and its legend.
    pub(super) fn draw_report(&self, out: &mut Vec<Draw>) {
        let Some(s) = &self.report.shown else {
            return;
        };
        let now = self.now();
        let up = ease_out(s.part(now, SHEET_DELAY_MS, 0.0, SHEET_MS));
        let top = SHEET_FROM + (SHEET_AT - SHEET_FROM) * up;
        let scroll = glide_at(s.glide, s.scroll, now);
        let canvas = self.canvas();
        slot_ui::draw_report(
            canvas,
            top,
            self.report.frame,
            self.report.sheet,
            scroll,
            out,
        );
        let legend = ease(s.part(now, LEGEND_DELAY_MS, 0.0, LEGEND_MS));
        // A turns the page only from the report itself.
        self.draw_legend_row(&self.report.legend, &[true, !s.per_game], legend, out);
    }

    /// The sheet to print, once each time the report opens.
    pub fn report_want(&mut self) -> Option<ReportSheet> {
        if !std::mem::take(&mut self.report.stale) {
            return None;
        }
        let root = self.root.as_ref()?;
        let per_game = self.report.shown.is_some_and(|s| s.per_game);
        let offset = i32::from(self.state.utc_offset_min);
        let now = self.power.as_ref().map_or_else(system_secs, |p| p.now());
        let today = slot_store::local_day(now, offset);
        let a = slot_store::activity(&slot_store::read_activity(root), today, offset);
        let time = slot_store::play_time_text;
        let row = |name: &str, value: String| -> ReportRow { (name.to_string(), value) };
        let name = |game: &slot_store::Recent| slot_ui::clean_label(&game.stem).to_uppercase();
        let days = match a.streak {
            1 => "1 DAY".to_string(),
            n => format!("{n} DAYS"),
        };
        Some(ReportSheet {
            date: date_text(today),
            total: vec![
                row("THIS WEEK", time(a.this_week)),
                row("THIS MONTH", time(a.this_month)),
                row("ALL TIME", time(a.all_time)),
            ],
            last: a
                .last_played
                .iter()
                .take(LAST_SHOWN)
                .map(|(game, day)| (name(game), when_text(*day, today)))
                .collect(),
            most: a
                .most_played
                .iter()
                .take(MOST_SHOWN)
                .enumerate()
                .map(|(i, (game, secs))| (format!("{}. {}", i + 1, name(game)), time(*secs)))
                .collect(),
            systems: a
                .by_system
                .iter()
                // The NES and SNES rows only once they have been played, so a card of handheld
                // games keeps its three rows.
                .filter(|(platform, secs)| !platform.on_a_tv() || *secs > 0)
                .map(|(platform, secs)| row(platform.dir_name(), time(*secs)))
                .collect(),
            habits: vec![
                row("SESSIONS", a.sessions.to_string()),
                row("LONGEST", time(a.longest)),
                row("STREAK", days),
            ],
            per_game: per_game.then(|| {
                a.most_played
                    .iter()
                    .enumerate()
                    .map(|(i, (game, secs))| {
                        let day = a
                            .last_played
                            .iter()
                            .find(|(g, _)| g == game)
                            .map_or(String::new(), |(_, day)| when_text(*day, today));
                        (format!("{}. {}", i + 1, name(game)), time(*secs), day)
                    })
                    .collect()
            }),
        })
    }

    pub fn set_report_sheet(&mut self, tex: TexId, h: u32) {
        if let Some((old, _)) = self.report.sheet.replace((tex, h)) {
            self.orphaned.push(old);
        }
    }

    /// The frame and legend are made once, the first time the report opens.
    pub fn wants_report_frame(&self) -> bool {
        self.report.frame.is_none() && self.report.shown.is_some()
    }

    pub fn set_report_frame(&mut self, frame: TexId, legend: Vec<(TexId, u32)>) {
        self.report.frame = Some(frame);
        self.report.legend = legend;
    }
}

/// `OCT 22`.
fn date_text(day: i64) -> String {
    let (_, m, d) = slot_store::civil(day);
    format!("{} {d}", MONTHS[m as usize - 1])
}

/// When a game was last played, as the report says it: TODAY, YESTERDAY, or the date.
fn when_text(day: i64, today: i64) -> String {
    match today - day {
        0 => "TODAY".to_string(),
        1 => "YESTERDAY".to_string(),
        _ => date_text(day),
    }
}
