//! The plate across the foot of an open cartridge that says how long its game has been played and
//! when it last was, `PLAYED 64 H 10 M · yesterday`, as the design's START screens print it.

use fontdue::Font;
use slot_store::Platform;

use crate::board::{open_layout, Placed};
use crate::cart::CartFace;
use crate::draw::{Draw, TexId};
use crate::text;

/// The face is drawn at twice the design's size, with this margin (design px) for its shadow.
const SCALE: f32 = 2.0;
const PAD: f32 = 6.0;
const PLATE_H: f32 = 40.0;
const RADIUS: f32 = 4.0;
const GAP: f32 = 10.0;
/// The least room either side of the words, should a long time outgrow the design's width.
const SIDE: f32 = 12.0;

const GROUND: [u8; 3] = [14, 14, 18];
const GROUND_ALPHA: f32 = 0.86;
const RIM_ALPHA: f32 = 0.12;
const SHADOW_ALPHA: f32 = 0.4;
const SHADOW_DROP: f32 = 2.0;
const SHADOW_BLUR: f32 = 4.0;
const INK: [u8; 3] = [0xf6, 0xf4, 0xef];
const QUIET_INK: [u8; 3] = [0xb9, 0xb6, 0xad];

/// The plate's width and how far its foot stands above the board's, as the design places it on
/// each board at rest.
fn spec(platform: Platform) -> (f32, f32) {
    match platform {
        Platform::Gba => (260.0, 4.0),
        Platform::Gb | Platform::Gbc => (252.0, 21.0),
        // `NES SNES Carts.dc.html` 2c: the plate's foot at y 340, the 340-tall back's at 358.
        Platform::Nes => (252.0, 18.0),
        // 2d: 14 above the 240-tall back's foot, kept in step with the back's 1.25 zoom.
        Platform::Snes => (252.0, 14.0 * crate::shelf::GBA_SHELF_ZOOM),
    }
}

/// The plate for a game played for `time`, last played `when`.
pub fn played_face(platform: Platform, time: &str, when: &str) -> Option<CartFace> {
    let font = text::label_font()?;
    let when = format!("· {when}");
    let spans: [(&str, f32, f32, [u8; 3]); 3] = [
        ("PLAYED", 11.0, 2.0, QUIET_INK),
        (time, 18.0, 0.0, INK),
        (when.as_str(), 13.0, 0.0, QUIET_INK),
    ];
    let widths = spans.map(|(line, px, tracking, _)| span_width(font, line, px, tracking));
    let words = widths.iter().sum::<f32>() + GAP * (spans.len() - 1) as f32;
    let plate_w = spec(platform).0.max(words + 2.0 * SIDE);
    let w = ((plate_w + 2.0 * PAD) * SCALE).ceil() as u32;
    let h = ((PLATE_H + 2.0 * PAD) * SCALE).ceil() as u32;
    let mut face = CartFace {
        rgba: vec![0; (w * h * 4) as usize],
        w,
        h,
    };
    ground(&mut face, plate_w);
    let mut x = PAD + (plate_w - words) / 2.0;
    for ((line, px, tracking, ink), width) in spans.into_iter().zip(widths) {
        type_span(&mut face, font, line, px, tracking, x, ink);
        x += width + GAP;
    }
    Some(face)
}

/// The plate centred across the foot of `board`, growing and moving with it as the cartridge
/// opens.
pub fn draw_played(
    platform: Platform,
    board: Placed,
    face: (TexId, u32, u32),
    alpha: f32,
    out: &mut Vec<Draw>,
) {
    let (tex, w, h) = face;
    let k = board.w / open_layout(platform).board.0;
    let (w, h) = (w as f32 / SCALE * k, h as f32 / SCALE * k);
    let foot = board.y + board.h - spec(platform).1 * k;
    out.push(Draw::Tex {
        x: (board.x + (board.w - w) / 2.0).round(),
        y: (foot - (PLATE_H + PAD) * k).round(),
        w,
        h,
        tex,
        alpha,
    });
}

/// A span's width in design px; CSS letter-spacing also follows the last letter.
fn span_width(font: &Font, line: &str, px: f32, tracking: f32) -> f32 {
    text::line_width(font, line, px, tracking) + tracking
}

/// The dark ground with its faint rim, and the soft shadow it casts below it.
fn ground(face: &mut CartFace, plate_w: f32) {
    let (hx, hy) = (plate_w / 2.0 * SCALE, PLATE_H / 2.0 * SCALE);
    let (cx, cy) = ((PAD + plate_w / 2.0) * SCALE, (PAD + PLATE_H / 2.0) * SCALE);
    let r = RADIUS * SCALE;
    for y in 0..face.h as i32 {
        for x in 0..face.w as i32 {
            let (px, py) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let d = rounded(px, py, hx, hy, r);
            let inside = (0.5 - d).clamp(0.0, 1.0);
            // CSS keeps a box's shadow out from under the box itself.
            let shade = rounded(px, py - SHADOW_DROP * SCALE, hx, hy, r) / (SHADOW_BLUR * SCALE);
            over(
                face,
                x,
                y,
                [0; 3],
                SHADOW_ALPHA * (1.0 - soft(shade)) * (1.0 - inside),
            );
            over(face, x, y, GROUND, GROUND_ALPHA * inside);
            let rim = inside * (d + SCALE + 0.5).clamp(0.0, 1.0);
            over(face, x, y, [255; 3], RIM_ALPHA * rim);
        }
    }
}

/// How far a point is outside a rounded box of half size `hx` by `hy` centred on the origin;
/// negative inside.
fn rounded(px: f32, py: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let (qx, qy) = (px.abs() - hx + r, py.abs() - hy + r);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

/// 1 to 0 as `t` runs from -1 to 1, easing at both ends: a blurred edge's falloff.
fn soft(t: f32) -> f32 {
    let u = ((t + 1.0) / 2.0).clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// One span of the line, its left edge at `left` (design px), centred down the plate as the
/// design's flex row centres it.
fn type_span(
    face: &mut CartFace,
    font: &Font,
    line: &str,
    px: f32,
    tracking: f32,
    left: f32,
    ink: [u8; 3],
) {
    let (px, tracking) = (px * SCALE, tracking * SCALE);
    let layout = text::Layout {
        lines: vec![line.to_string()],
        px,
        tracking,
    };
    let margin = (px * 0.3).ceil();
    let w = (text::line_width(font, line, px, tracking) + 2.0 * margin).ceil() as u32;
    let h = (PLATE_H * SCALE) as u32;
    let cover = text::coverage_in(font, w, h, &layout);
    let x0 = (left * SCALE - margin).round() as i32;
    let y0 = (PAD * SCALE) as i32;
    for (i, a) in cover.into_iter().enumerate() {
        let (x, y) = (x0 + (i as u32 % w) as i32, y0 + (i as u32 / w) as i32);
        over(face, x, y, ink, a as f32 / 255.0);
    }
}

/// `ink` at opacity `a` laid over a pixel of the face, which stays unpremultiplied.
fn over(face: &mut CartFace, x: i32, y: i32, ink: [u8; 3], a: f32) {
    if a <= 0.0 || x < 0 || y < 0 || x as u32 >= face.w || y as u32 >= face.h {
        return;
    }
    let at = ((y as u32 * face.w + x as u32) * 4) as usize;
    let px = &mut face.rgba[at..at + 4];
    let under = px[3] as f32 / 255.0 * (1.0 - a);
    let out = a + under;
    for (c, &k) in px.iter_mut().zip(ink.iter()) {
        *c = ((k as f32 * a + *c as f32 * under) / out).round() as u8;
    }
    px[3] = (out * 255.0).round() as u8;
}
