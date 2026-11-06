//! The Activity report (`design_handoff_extra_shelf`, screen 3a): the sheet A on the receipt slides
//! up over the Extras shelf. Its type is printed once, all of it, onto one tall face; the screen
//! shows a window of it that ↑/↓ move, under a frame of shadow and torn foot that stays put.

use fontdue::Font;
use slot_gfx::{Canvas, Draw, TexId};

use crate::bitmaps::ButtonLegend;
use crate::cart::CartFace;
use crate::extras::{blend, receipt_font, INK, PAPER, SCALE};
use crate::plate::UndoFace;
use crate::quick_menu::{button_face, with_hint};
use crate::text;

/// The sheet's size on screen, in design pixels.
pub const REPORT_W: f32 = 600.0;
pub const REPORT_H: f32 = 316.0;

/// The torn foot's teeth: 8 px deep, one every 6% of the width.
const TOOTH: f32 = 8.0;
const TOOTH_EVERY: f32 = 0.06;

/// drop-shadow(0 10px 14px rgba(0,0,0,.5)): a blur of 14 px is a gaussian of half that.
const SHADOW_PAD: f32 = 36.0;
const SHADOW_DROP: f32 = 10.0;
const SHADOW_SIGMA: f32 = 7.0;
const SHADOW_ALPHA: f32 = 0.5;

/// padding: 18px 32px 26px; two columns 40 px apart, grid rows 4 px apart.
const PAD_TOP: f32 = 18.0;
const PAD_SIDE: f32 = 32.0;
const PAD_FOOT: f32 = 26.0;
const COLUMN_GAP: f32 = 40.0;
const COLUMN_W: f32 = (REPORT_W - 2.0 * PAD_SIDE - COLUMN_GAP) / 2.0;
const ROW_GAP: f32 = 4.0;

const ROW_PX: f32 = 19.0;
/// Between a row's name and its value, at the closest.
const NAME_GAP: f32 = 16.0;
const RULE: f32 = 2.0;
const DASH: f32 = 6.0;

const MUTED: [u8; 3] = [0x6b, 0x68, 0x62];

/// The report's three kinds of type.
#[derive(Copy, Clone)]
enum Style {
    /// ACTIVITY REPORT and the date: 22 px, spaced 2 px.
    Title,
    /// A section's name: 17 px, spaced 2 px, greyed.
    Heading,
    Row,
}

impl Style {
    /// Size, letter spacing and ink.
    fn spec(self) -> (f32, f32, [u8; 3]) {
        match self {
            Style::Title => (22.0, 2.0, INK),
            Style::Heading => (17.0, 2.0, MUTED),
            Style::Row => (ROW_PX, 0.0, INK),
        }
    }
}

/// A row: what it is on the left, how much or when on the right.
pub type ReportRow = (String, String);

/// What the report prints, already in its words.
pub struct ReportSheet {
    /// Today, as `OCT 22`.
    pub date: String,
    pub total: Vec<ReportRow>,
    pub last: Vec<ReportRow>,
    pub most: Vec<ReportRow>,
    pub systems: Vec<ReportRow>,
    pub habits: Vec<ReportRow>,
    /// When set, the sheet is the second page A turns to: every game played, most first, each
    /// with its time and when it was last played.
    pub per_game: Option<Vec<(String, String, String)>>,
}

/// One line of type, `x` its left or right edge as `right` says and `top` the top of its line,
/// in design pixels on the sheet.
struct Line {
    text: String,
    px: f32,
    tracking: f32,
    x: f32,
    top: f32,
    right: bool,
    ink: [u8; 3],
}

/// What goes on the sheet: lines of type, and the dashed rules under the title and between the
/// two halves (their tops).
#[derive(Default)]
struct Marks {
    lines: Vec<Line>,
    rules: Vec<f32>,
}

/// The height of a line of type at `px`: the font's own line, as CSS's `normal` takes it.
fn line_h(font: &Font, px: f32) -> f32 {
    font.horizontal_line_metrics(px * SCALE)
        .map_or(px, |m| m.new_line_size / SCALE)
}

fn width(font: &Font, line: &str, px: f32, tracking: f32) -> f32 {
    text::line_width(font, line, px * SCALE, tracking * SCALE) / SCALE
}

/// A name cut short with an ellipsis to fit `room`.
fn fit(font: &Font, name: &str, room: f32) -> String {
    if width(font, name, ROW_PX, 0.0) <= room {
        return name.to_string();
    }
    let mut cut: String = name.to_string();
    while !cut.is_empty() {
        cut.pop();
        let shown = format!("{}...", cut.trim_end());
        if width(font, &shown, ROW_PX, 0.0) <= room {
            return shown;
        }
    }
    String::new()
}

impl Marks {
    fn line(&mut self, text: &str, style: Style, x: f32, top: f32, right: bool) {
        let (px, tracking, ink) = style.spec();
        self.lines.push(Line {
            text: text.to_string(),
            px,
            tracking,
            x,
            top,
            right,
            ink,
        });
    }

    /// A column of sections from `top`, each a heading over its rows, `gap` apart; a section
    /// after the first stands 10 px further off. Returns where the column ends.
    fn column(
        &mut self,
        font: &Font,
        x: f32,
        top: f32,
        sections: &[(&str, &[ReportRow])],
        gap: f32,
    ) -> f32 {
        let mut y = top;
        for (n, (heading, rows)) in sections.iter().enumerate() {
            if n > 0 {
                y += gap + 10.0;
            }
            self.line(heading, Style::Heading, x, y, false);
            y += line_h(font, Style::Heading.spec().0);
            for (name, value) in rows.iter() {
                y += gap;
                let room = COLUMN_W - NAME_GAP - width(font, value, ROW_PX, 0.0);
                self.line(&fit(font, name, room), Style::Row, x, y, false);
                self.line(value, Style::Row, x + COLUMN_W, y, true);
                y += line_h(font, ROW_PX);
            }
        }
        y
    }

    /// The report laid out as the design's grid; returns its full height.
    /// The second page: a title, then one full-width row per game, its time and its day set
    /// right. Returns its full height.
    fn lay_out_games(
        &mut self,
        font: &Font,
        date: &str,
        games: &[(String, String, String)],
    ) -> f32 {
        let (left, end) = (PAD_SIDE, REPORT_W - PAD_SIDE);
        let mut y = PAD_TOP;
        self.line("PLAY TIME PER GAME", Style::Title, left, y, false);
        self.line(date, Style::Title, end, y, true);
        y += line_h(font, Style::Title.spec().0) + 8.0;
        self.rules.push(y);
        y += RULE + ROW_GAP;
        // The day sits in a column of its own, as wide as the widest, so the times line up.
        let day_w = games
            .iter()
            .map(|(_, _, day)| width(font, day, ROW_PX, 0.0))
            .fold(0.0, f32::max);
        let time_right = end - day_w - NAME_GAP;
        for (name, time, day) in games {
            y += 4.0;
            let room = time_right - left - NAME_GAP - width(font, time, ROW_PX, 0.0);
            self.line(&fit(font, name, room), Style::Row, left, y, false);
            self.line(time, Style::Row, time_right, y, true);
            self.line(day, Style::Heading, end, y, true);
            y += line_h(font, ROW_PX);
        }
        y + ROW_GAP + 8.0 + PAD_FOOT
    }

    fn lay_out(&mut self, font: &Font, sheet: &ReportSheet) -> f32 {
        if let Some(games) = &sheet.per_game {
            return self.lay_out_games(font, &sheet.date, games);
        }
        let (left, right) = (PAD_SIDE, PAD_SIDE + COLUMN_W + COLUMN_GAP);
        let end = REPORT_W - PAD_SIDE;
        let mut y = PAD_TOP;
        self.line("ACTIVITY REPORT", Style::Title, left, y, false);
        self.line(&sheet.date, Style::Title, end, y, true);
        y += line_h(font, Style::Title.spec().0) + 8.0;
        self.rules.push(y);
        y += RULE + ROW_GAP;
        let a = self.column(
            font,
            left,
            y,
            &[
                ("TOTAL TIME", sheet.total.as_slice()),
                ("LAST PLAYED", sheet.last.as_slice()),
            ],
            6.0,
        );
        let b = self.column(
            font,
            right,
            y,
            &[("MOST PLAYED", sheet.most.as_slice())],
            6.0,
        );
        y = a.max(b) + ROW_GAP + 6.0;
        self.rules.push(y);
        y += RULE + ROW_GAP;
        let a = self.column(
            font,
            left,
            y,
            &[("BY SYSTEM", sheet.systems.as_slice())],
            4.0,
        );
        let b = self.column(font, right, y, &[("HABITS", sheet.habits.as_slice())], 4.0);
        a.max(b) + ROW_GAP + 8.0 + PAD_FOOT
    }
}

fn print(face: &mut CartFace, font: &Font, line: &Line) {
    let px = line.px * SCALE;
    let tracking = line.tracking * SCALE;
    let line_w = text::line_width(font, &line.text, px, tracking);
    let margin = (px * 0.3).ceil();
    let w = (line_w + 2.0 * margin).ceil() as u32;
    let h = (line_h(font, line.px) * SCALE).ceil() as u32;
    let layout = text::Layout {
        lines: vec![line.text.clone()],
        px,
        tracking,
    };
    let cover = text::coverage_in(font, w, h, &layout);
    // Letter spacing also follows the last letter, so right-set spaced type stands off by it.
    let left = match line.right {
        true => line.x * SCALE - line_w - tracking - margin,
        false => line.x * SCALE - margin,
    };
    let top = line.top * SCALE;
    for (i, a) in cover.into_iter().enumerate() {
        let (x, y) = (i as u32 % w, i as u32 / w);
        blend(
            face,
            left as i32 + x as i32,
            top as i32 + y as i32,
            a,
            line.ink,
        );
    }
}

/// A 2 px dashed rule across the content's width, its top at `top`.
fn rule(face: &mut CartFace, top: f32) {
    let (x0, x1) = (PAD_SIDE * SCALE, (REPORT_W - PAD_SIDE) * SCALE);
    let (y0, y1) = ((top * SCALE) as i32, ((top + RULE) * SCALE) as i32);
    let mut x = x0;
    while x < x1 {
        let dash_end = (x + DASH * SCALE).min(x1);
        for y in y0..y1 {
            for px in x as i32..dash_end as i32 {
                blend(face, px, y, 255, INK);
            }
        }
        x += 2.0 * DASH * SCALE;
    }
}

/// The whole report, as tall as its rows need and never shorter than the sheet.
pub fn report_face(sheet: &ReportSheet) -> Option<CartFace> {
    let font = receipt_font()?;
    let mut marks = Marks::default();
    let h = marks.lay_out(font, sheet).max(REPORT_H);
    let (w, h) = ((REPORT_W * SCALE) as u32, (h * SCALE).ceil() as u32);
    let rgba = [PAPER[0], PAPER[1], PAPER[2], 255].repeat((w * h) as usize);
    let mut face = CartFace { rgba, w, h };
    for top in &marks.rules {
        rule(&mut face, *top);
    }
    for line in &marks.lines {
        print(&mut face, font, line);
    }
    Some(face)
}

/// How far the report scrolls, its face `face_h` texels tall: to its last row and no further.
pub fn report_scroll_max(face_h: u32) -> f32 {
    (face_h as f32 / SCALE - REPORT_H).max(0.0)
}

/// erf, to Abramowitz and Stegun 7.1.26 (within 1.5e-7): the edge of a gaussian blur.
fn erf(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let poly = ((((1.061_405_4 * t - 1.453_152) * t + 1.421_413_7) * t - 0.284_496_74) * t
        + 0.254_829_6)
        * t;
    (1.0 - poly * (-x * x).exp()).copysign(x)
}

/// How much of a box from `a` to `b` a gaussian blur leaves at `x`.
fn blurred(x: f32, a: f32, b: f32) -> f32 {
    let edge = |d: f32| 0.5 * (1.0 + erf(d / (SHADOW_SIGMA * std::f32::consts::SQRT_2)));
    edge(x - a) - edge(x - b)
}

/// How far down the torn foot reaches at `f` across the sheet: 1 at a tooth's tip, 0 between.
fn tooth(f: f32) -> f32 {
    if f < 0.01 {
        return f / 0.01;
    }
    let g = ((1.0 - f) / TOOTH_EVERY).fract();
    1.0 - (2.0 * g - 1.0).abs()
}

/// The paper the report's window lies on: its soft shadow and torn foot, `SHADOW_PAD` all round.
pub fn report_frame_face() -> CartFace {
    let pad = SHADOW_PAD * SCALE;
    let (bw, bh) = (REPORT_W * SCALE, REPORT_H * SCALE);
    let (w, h) = ((bw + 2.0 * pad) as u32, (bh + 2.0 * pad) as u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
        let (x, y) = ((i as u32 % w) as f32 - pad, (i as u32 / w) as f32 - pad);
        let (dx, dy) = ((x + 0.5) / SCALE, (y + 0.5) / SCALE);
        let shade = SHADOW_ALPHA
            * blurred(dx, 0.0, REPORT_W)
            * blurred(dy, SHADOW_DROP, REPORT_H + SHADOW_DROP);
        let paper = match x >= 0.0 && x < bw && y >= 0.0 {
            true => {
                let foot = (REPORT_H - TOOTH * (1.0 - tooth((x + 0.5) / bw))) * SCALE;
                (foot - y).clamp(0.0, 1.0)
            }
            false => 0.0,
        };
        let a = paper + shade * (1.0 - paper);
        if a <= 0.0 {
            continue;
        }
        for (c, ink) in px.iter_mut().zip(PAPER) {
            *c = (ink as f32 * paper / a).round() as u8;
        }
        px[3] = (a * 255.0).round() as u8;
    }
    CartFace { rgba, w, h }
}

/// The open report's legend: B, Back.
/// B Back, and A turning to the play time per game.
pub fn report_legends() -> Vec<UndoFace> {
    vec![
        with_hint(button_face(ButtonLegend::B), "Back", None, true),
        with_hint(
            button_face(ButtonLegend::A),
            "Play time per game",
            None,
            false,
        ),
    ]
}

/// The report with its top at `top`, centred, showing its rows from `scroll` down; `sheet` is its
/// face and that face's height in texels.
pub fn draw_report(
    canvas: Canvas,
    top: f32,
    frame: Option<TexId>,
    sheet: Option<(TexId, u32)>,
    scroll: f32,
    out: &mut Vec<Draw>,
) {
    let x = canvas.centred(REPORT_W);
    if let Some(tex) = frame {
        out.push(Draw::Tex {
            x: x - SHADOW_PAD,
            y: top - SHADOW_PAD,
            w: REPORT_W + 2.0 * SHADOW_PAD,
            h: REPORT_H + 2.0 * SHADOW_PAD,
            tex,
            alpha: 1.0,
        });
    }
    let Some((tex, face_h)) = sheet else {
        return;
    };
    // The window stops where the teeth begin; the frame's paper carries on to their tips.
    let shown = REPORT_H - TOOTH;
    let tall = face_h as f32 / SCALE;
    out.push(Draw::Part {
        x,
        y: top,
        w: REPORT_W,
        h: shown,
        tex,
        alpha: 1.0,
        uv: [0.0, scroll / tall, 1.0, shown / tall],
    });
}
