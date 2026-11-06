//! The Extras shelf (`design_handoff_extra_shelf`, turn 3): Activity as a printer receipt of the
//! week's play, Get lucky as a slate with three dice, Never played as a blister pack. The art is
//! baked from the design; the receipt's bars and times and the pack's count are drawn here.

use std::sync::OnceLock;

use fontdue::{Font, FontSettings};
use slot_gfx::{Canvas, Draw, TexId};

use crate::art::decode_png_bytes;
use crate::board::Placed;
use crate::cart::CartFace;
use crate::folder::Pose;
use crate::text;

const RECEIPT_PNG: &[u8] = include_bytes!("../../../assets/extra_receipt.png");
const SLATE_PNG: &[u8] = include_bytes!("../../../assets/extra_slate.png");
const BLISTER_PNG: &[u8] = include_bytes!("../../../assets/extra_blister.png");
const BUBBLE_PNG: &[u8] = include_bytes!("../../../assets/extra_bubble.png");
const DIE_PNG: [&[u8]; 3] = [
    include_bytes!("../../../assets/extra_die0.png"),
    include_bytes!("../../../assets/extra_die1.png"),
    include_bytes!("../../../assets/extra_die2.png"),
];
/// VT323 (OFL, `VT323-OFL.txt`): the receipt printer's type.
const VT323_TTF: &[u8] = include_bytes!("../assets/VT323-Regular.ttf");

/// The art is twice the design's size, with this margin (in design pixels) for its shadow.
pub(crate) const SCALE: f32 = 2.0;
const RECEIPT_PAD: f32 = 16.0;
const SLATE_PAD: f32 = 16.0;
const BLISTER_PAD: f32 = 40.0;
const BUBBLE_PAD: f32 = 12.0;

pub(crate) const INK: [u8; 3] = [0x2b, 0x2a, 0x28];
pub(crate) const PAPER: [u8; 3] = [0xfb, 0xf8, 0xf0];
const COUNT_INK: [u8; 3] = [0xe6, 0xef, 0xe4];

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Extra {
    Activity,
    Lucky,
    Unplayed,
}

impl Extra {
    pub const ALL: [Extra; 3] = [Extra::Activity, Extra::Lucky, Extra::Unplayed];

    /// The item's box on a 720 canvas when it stands selected: left, top, width, height, turn.
    fn spec(self) -> (f32, f32, f32, f32, f32) {
        match self {
            Extra::Activity => (210.0, 80.0, 300.0, 290.0, 0.0),
            Extra::Lucky => (200.0, 105.0, 320.0, 240.0, 0.0),
            Extra::Unplayed => (210.0, 95.0, 300.0, 260.0, -2.0),
        }
    }

    fn pad(self) -> f32 {
        match self {
            Extra::Activity => RECEIPT_PAD,
            Extra::Lucky => SLATE_PAD,
            Extra::Unplayed => BLISTER_PAD,
        }
    }
}

/// The scale the neighbours stand at, half off the screen.
pub const SIDE_SCALE: f32 = 0.78;

/// Where an item stands `offset` places from the selected one (fractions while scrolling).
pub fn extra_box(canvas: Canvas, item: Extra, offset: f32) -> Placed {
    let (_, top, w, h, _) = item.spec();
    let t = offset.abs().min(1.0);
    let scale = 1.0 - (1.0 - SIDE_SCALE) * t;
    let cx = canvas.w / 2.0 + offset * canvas.w / 2.0;
    let cy = top + h / 2.0;
    Placed {
        x: cx - w * scale / 2.0,
        y: cy - h * scale / 2.0,
        w: w * scale,
        h: h * scale,
    }
}

/// The item's art drawn in `at`, its box: the art reaches past it by its shadow margin.
pub fn draw_extra(item: Extra, at: Placed, tex: TexId, alpha: f32, out: &mut Vec<Draw>) {
    let (_, _, w, _, turn) = item.spec();
    let pad = item.pad() * at.w / w;
    out.push(Draw::Turned {
        x: at.x - pad,
        y: at.y - pad,
        w: at.w + 2.0 * pad,
        h: at.h + 2.0 * pad,
        tex,
        alpha,
        turn: turn.to_radians(),
    });
}

/// The bubble over the blister pack's card, for the rip: its box inside the pack's box `at`.
pub fn bubble_pose(at: Placed) -> Pose {
    let k = at.w / 300.0;
    Pose {
        at: Placed {
            x: at.x + (22.0 - BUBBLE_PAD) * k,
            y: at.y + (56.0 - BUBBLE_PAD) * k,
            w: (256.0 + 2.0 * BUBBLE_PAD) * k,
            h: (160.0 + 2.0 * BUBBLE_PAD) * k,
        },
        turn: (-2.0f32).to_radians(),
    }
}

/// Where each die rests on the slate whose box is `at`: its centre and its art's size.
pub fn die_rest(at: Placed, die: usize) -> Placed {
    const CENTRES: [(f32, f32); 3] = [(76.0, 58.0), (242.0, 66.0), (216.0, 190.0)];
    const SIZES: [f32; 3] = [106.0, 100.5, 95.0];
    let k = at.w / 320.0;
    let (cx, cy) = CENTRES[die % 3];
    let s = SIZES[die % 3] * k;
    Placed {
        x: at.x + cx * k - s / 2.0,
        y: at.y + cy * k - s / 2.0,
        w: s,
        h: s,
    }
}

fn art(png: &'static [u8], cell: &'static OnceLock<Option<CartFace>>) -> Option<CartFace> {
    cell.get_or_init(|| {
        let (rgba, w, h) = decode_png_bytes(png)?;
        (rgba.len() == (w * h * 4) as usize).then_some(CartFace { rgba, w, h })
    })
    .clone()
}

pub fn slate_face() -> Option<CartFace> {
    static P: OnceLock<Option<CartFace>> = OnceLock::new();
    art(SLATE_PNG, &P)
}

pub fn bubble_face() -> Option<CartFace> {
    static P: OnceLock<Option<CartFace>> = OnceLock::new();
    art(BUBBLE_PNG, &P)
}

pub fn die_face(die: usize) -> Option<CartFace> {
    static P: [OnceLock<Option<CartFace>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    art(DIE_PNG[die % 3], &P[die % 3])
}

pub(crate) fn receipt_font() -> Option<&'static Font> {
    static F: OnceLock<Option<Font>> = OnceLock::new();
    F.get_or_init(|| Font::from_bytes(VT323_TTF, FontSettings::default()).ok())
        .as_ref()
}

/// One line of type laid over `face`, whose art has `pad` (design px) round a box; `x` is the
/// left or right edge as `right` says, `top` the top of the line, both in the box's pixels. Where
/// `under` is given, pixels inside it take `inside` instead of `ink`.
#[allow(clippy::too_many_arguments)]
fn type_line(
    face: &mut CartFace,
    font: &Font,
    line: &str,
    px: f32,
    pad: f32,
    x: f32,
    top: f32,
    right: bool,
    ink: [u8; 3],
) {
    let px = px * SCALE;
    let layout = text::Layout {
        lines: vec![line.to_string()],
        px,
        tracking: 0.0,
    };
    let line_w = text::line_width(font, line, px, 0.0);
    let margin = (px * 0.3).ceil();
    let w = (line_w + 2.0 * margin).ceil() as u32;
    let h = (px * 1.3).ceil() as u32;
    let cover = text::coverage_in(font, w, h, &layout);
    let anchor = (pad + x) * SCALE;
    let left = if right {
        anchor - line_w - margin
    } else {
        anchor - margin
    };
    let top = (pad + top) * SCALE;
    for (i, a) in cover.into_iter().enumerate() {
        let (cx, cy) = (
            left as i32 + (i as u32 % w) as i32,
            top as i32 + (i as u32 / w) as i32,
        );
        blend(face, cx, cy, a, ink);
    }
}

pub(crate) fn blend(face: &mut CartFace, x: i32, y: i32, a: u8, ink: [u8; 3]) {
    if a == 0 || x < 0 || y < 0 || x as u32 >= face.w || y as u32 >= face.h {
        return;
    }
    let at = ((y as u32 * face.w + x as u32) * 4) as usize;
    let a = a as f32 / 255.0;
    for (k, &c) in ink.iter().enumerate() {
        let d = face.rgba[at + k] as f32;
        face.rgba[at + k] = (c as f32 * a + d * (1.0 - a)).round() as u8;
    }
}

/// The week as the receipt prints it: the days' range, each day's play, the total.
pub struct ReceiptWeek<'a> {
    pub range: &'a str,
    pub days: [u32; 7],
    pub labels: [String; 7],
    pub total: &'a str,
}

/// The receipt with this week printed on it.
pub fn receipt_face(week: &ReceiptWeek) -> Option<CartFace> {
    static P: OnceLock<Option<CartFace>> = OnceLock::new();
    let mut face = art(RECEIPT_PNG, &P)?;
    let font = receipt_font()?;
    let pad = RECEIPT_PAD;
    // The paper hangs 14 px below the printer; the design measures from the paper's top.
    let paper = 14.0;
    type_line(
        &mut face,
        font,
        week.range,
        20.0,
        pad,
        276.0,
        paper + 12.0,
        true,
        INK,
    );
    type_line(
        &mut face,
        font,
        week.total,
        20.0,
        pad,
        276.0,
        paper + 224.0,
        true,
        INK,
    );
    const BAR_W: f32 = 26.0;
    const BARS_TOP: f32 = 52.0;
    const BARS_H: f32 = 134.0;
    let most = week.days.iter().copied().max().unwrap_or(0).max(1);
    for (day, secs) in week.days.iter().enumerate() {
        let x = 26.0 + day as f32 * (248.0 - BAR_W) / 6.0;
        let h = (*secs as f32 / most as f32 * BARS_H).max(6.0);
        let bottom = paper + BARS_TOP + BARS_H;
        let bar = Placed {
            x,
            y: bottom - h,
            w: BAR_W,
            h,
        };
        fill(&mut face, pad, bar, INK);
        if *secs > 0 {
            bar_label(&mut face, font, &week.labels[day], pad, bar);
        }
    }
    Some(face)
}

fn fill(face: &mut CartFace, pad: f32, r: Placed, ink: [u8; 3]) {
    let (x0, y0) = (((pad + r.x) * SCALE) as i32, ((pad + r.y) * SCALE) as i32);
    let (x1, y1) = (
        ((pad + r.x + r.w) * SCALE) as i32,
        ((pad + r.y + r.h) * SCALE) as i32,
    );
    for y in y0..y1 {
        for x in x0..x1 {
            blend(face, x, y, 255, ink);
        }
    }
}

/// A bar's time, reading up the bar from 5 px above its foot: paper-coloured on the bar, ink
/// where it runs past a short bar's top.
fn bar_label(face: &mut CartFace, font: &Font, line: &str, pad: f32, bar: Placed) {
    let px = 20.0 * SCALE;
    let layout = text::Layout {
        lines: vec![line.to_string()],
        px,
        tracking: 0.0,
    };
    let len = text::line_width(font, line, px, 0.0).ceil() as u32 + 2;
    let thick = (bar.w * SCALE) as u32;
    let cover = text::coverage_in(font, len, thick, &layout);
    let foot = ((pad + bar.y + bar.h) * SCALE) as i32 - (5.0 * SCALE) as i32;
    let left = ((pad + bar.x) * SCALE) as i32;
    let bar_top = ((pad + bar.y) * SCALE) as i32;
    let cover_w = len as i32;
    for (i, a) in cover.into_iter().enumerate() {
        let (u, v) = ((i as i32) % cover_w, (i as i32) / cover_w);
        // Turned a quarter anticlockwise: along the line runs up the bar.
        let x = left + v;
        let y = foot - u;
        let ink = if y >= bar_top { PAPER } else { INK };
        blend(face, x, y, a, ink);
    }
}

/// The blister pack with how many games wait in its pool.
pub fn blister_face(waiting: usize) -> Option<CartFace> {
    static P: OnceLock<Option<CartFace>> = OnceLock::new();
    let mut face = art(BLISTER_PNG, &P)?;
    let font = text::label_font()?;
    let line = format!("{waiting} GAMES WAITING");
    let px = 14.0 * SCALE;
    let tracking = 1.0 * SCALE;
    let layout = text::Layout {
        lines: vec![line.clone()],
        px,
        tracking,
    };
    let w = text::line_width(font, &line, px, tracking).ceil() as u32 + 8;
    let h = (px * 1.4).ceil() as u32;
    let cover = text::coverage_in(font, w, h, &layout);
    let left = ((BLISTER_PAD + 150.0) * SCALE) as i32 - w as i32 / 2;
    let top = ((BLISTER_PAD + 226.0) * SCALE) as i32;
    for (i, a) in cover.into_iter().enumerate() {
        blend(
            &mut face,
            left + (i as u32 % w) as i32,
            top + (i as u32 / w) as i32,
            a,
            COUNT_INK,
        );
    }
    Some(face)
}
