//! START on Get lucky or Never played: which systems the pick draws from, written on a sheet of
//! squared paper that slides up over the item, with the size of the pool that leaves
//! (`design_handoff_extra_shelf`, turn 3b and 3c). It is the folder menu's sheet, cut to one row.

use slot_gfx::Canvas;
use slot_store::Platform;

use crate::board::Placed;
use crate::cart::CartFace;
use crate::folder::{Pose, ART_SCALE, PAPER_PAD};
use crate::folder_menu::{
    arrow, blank_sheet, grown_pose, ink, loop_face, on_sheet, padded_pose, Align, Hand, ARROW,
};
use crate::text;

/// The sheet on a 720 canvas: its size and turn, and its top open and closed (below the screen);
/// it is centred across.
const SHEET_W: f32 = 328.0;
const SHEET_H: f32 = 200.0;
const SHEET_TURN: f32 = -1.5;
const OPEN_TOP: f32 = 60.0;
const CLOSED_TOP: f32 = 520.0;
/// The design's paper is the light squared one, whatever the item.
const PAPER: Platform = Platform::Gba;
const SIDE: f32 = 28.0;
const TITLE_TOP: f32 = 14.0;
const TITLE_PX: f32 = 26.0;
/// The System row, which the loop stands round.
const ROW_TOP: f32 = 48.0;
const ROW_H: f32 = 40.0;
const LABEL_PX: f32 = 20.0;
const LABEL_W: f32 = 104.0;
const VALUE_PX: f32 = 18.0;
const VALUE_GAP: f32 = 8.0;
const LOOP_INSET: f32 = 18.0;
/// The pool's size under the row, in the label font and a paler ink than the marker's.
const COUNT_TOP: f32 = 100.0;
const COUNT_PX: f32 = 14.0;
const COUNT_TRACKING: f32 = 1.0;
const COUNT_INK: [u8; 3] = [0x6b, 0x68, 0x62];

/// The sheet's box (without its shadow margin), `open` of the way up from below the screen.
fn sheet(canvas: Canvas, open: f32) -> Placed {
    Placed {
        x: canvas.centred(SHEET_W),
        y: CLOSED_TOP + (OPEN_TOP - CLOSED_TOP) * open,
        w: SHEET_W,
        h: SHEET_H,
    }
}

pub fn extras_menu_pose(canvas: Canvas, open: f32) -> Pose {
    padded_pose(sheet(canvas, open), SHEET_TURN)
}

/// The band's loop round the System row, riding up with the sheet.
pub fn extras_loop_pose(canvas: Canvas, open: f32) -> Pose {
    let local = Placed {
        x: LOOP_INSET,
        y: ROW_TOP,
        w: SHEET_W - 2.0 * LOOP_INSET,
        h: ROW_H,
    };
    grown_pose(on_sheet(sheet(canvas, open), SHEET_TURN, local))
}

pub fn extras_loop_face() -> CartFace {
    loop_face(SHEET_W - 2.0 * LOOP_INSET, ROW_H)
}

/// The sheet: the item's name, "System ◀ `system` ▶", and how many games are in the pool.
pub fn extras_menu_face(title: &str, system: &str, pool: usize) -> Option<CartFace> {
    let mut face = blank_sheet(PAPER, SHEET_W, SHEET_H)?;
    let wide = SHEET_W - 2.0 * SIDE;
    ink(
        &mut face,
        Hand::Marker,
        title,
        TITLE_PX,
        wide,
        SIDE,
        TITLE_TOP,
        Align::Left,
    );
    ink(
        &mut face,
        Hand::Marker,
        "System",
        LABEL_PX,
        LABEL_W,
        SIDE,
        ROW_TOP + (ROW_H - LABEL_PX * 1.4) / 2.0,
        Align::Left,
    );
    let right = SHEET_W - SIDE;
    let value_right = right - 2.0 * ARROW - VALUE_GAP;
    let w = ink(
        &mut face,
        Hand::Marker,
        system,
        VALUE_PX,
        wide - LABEL_W - 2.0 * (2.0 * ARROW + VALUE_GAP),
        value_right,
        ROW_TOP + (ROW_H - VALUE_PX * 1.4) / 2.0,
        Align::Right,
    );
    let mid = ROW_TOP + ROW_H / 2.0;
    arrow(&mut face, value_right - w - VALUE_GAP - ARROW, mid, false);
    arrow(&mut face, right - ARROW, mid, true);
    let games = if pool == 1 { "GAME" } else { "GAMES" };
    print_count(&mut face, &format!("{pool} {games} IN THE POOL"));
    Some(face)
}

/// Prints `line` at the count's place, left aligned at the sheet's side margin.
fn print_count(face: &mut CartFace, line: &str) {
    let Some(font) = text::label_font() else {
        return;
    };
    let (px, tracking) = (COUNT_PX * ART_SCALE, COUNT_TRACKING * ART_SCALE);
    let layout = text::Layout {
        lines: vec![line.to_string()],
        px,
        tracking,
    };
    let line_w = text::line_width(font, line, px, tracking);
    // The coverage centres the line, so a margin either side leaves it starting at the margin.
    let margin = (px * 0.3).ceil();
    let (w, h) = (
        (line_w + 2.0 * margin).ceil() as u32,
        (px * 1.4).ceil() as u32,
    );
    let cover = text::coverage_in(font, w, h, &layout);
    let left = ((PAPER_PAD + SIDE) * ART_SCALE - margin).max(0.0) as u32;
    let top = ((PAPER_PAD + COUNT_TOP) * ART_SCALE) as u32;
    for (i, a) in cover.into_iter().enumerate() {
        let (x, y) = (left + i as u32 % w, top + i as u32 / w);
        if a == 0 || x >= face.w || y >= face.h {
            continue;
        }
        let at = ((y * face.w + x) * 4) as usize;
        let a = a as f32 / 255.0;
        for (k, &c) in COUNT_INK.iter().enumerate() {
            let d = face.rgba[at + k] as f32;
            face.rgba[at + k] = (c as f32 * a + d * (1.0 - a)).round() as u8;
        }
    }
}
