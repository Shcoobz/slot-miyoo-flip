//! START on a folder: its settings written on a large sheet of the folder's paper, with the rubber
//! band as the pointer (`design_handoff_shelf_folders`, turn 4a), and the name entry Rename opens,
//! a letter grid like the ones Game Boy games name a player with.

use slot_gfx::Canvas;
use slot_store::Platform;

use crate::board::Placed;
use crate::cart::CartFace;
use crate::folder::{paper_art, Pose, ART_SCALE, INK, PAPER_PAD};
use crate::text;

/// The loop the band turns into around the selected row or letter.
const LOOP_INK: [u8; 3] = [0xa9, 0x44, 0x2e];
const LOOP_STROKE: f32 = 5.0;
const LOOP_RADIUS: f32 = 22.0;

/// The menu sheet on a 720 canvas: its top, size and turn; it is centred across.
const MENU_TOP: f32 = 120.0;
const MENU_W: f32 = 328.0;
const MENU_H: f32 = 238.0;
const MENU_TURN: f32 = -1.5;
const MENU_NAME_PX: f32 = 34.0;
const MENU_NAME_TOP: f32 = 12.0;
const ROW_TOP: f32 = 66.0;
const ROW_H: f32 = 40.0;
const ROW_SIDE: f32 = 28.0;
const ROW_PX: f32 = 22.0;
const LOOP_INSET: f32 = 22.0;
pub(crate) const ARROW: f32 = 7.0;
const ARROW_GAP: f32 = 6.0;
/// The labels' column; the values take the rest.
const LABEL_W: f32 = 104.0;
pub const MENU_ROWS_SHOWN: usize = 4;

/// The name entry's sheet, and its letters.
const ENTRY_TOP: f32 = 18.0;
const ENTRY_W: f32 = 520.0;
const ENTRY_H: f32 = 330.0;
const ENTRY_TURN: f32 = -1.0;
const SLOTS_TOP: f32 = 18.0;
const SLOT_W: f32 = 26.0;
const SLOT_PX: f32 = 30.0;
const GRID_TOP: f32 = 76.0;
const CELL_W: f32 = 52.0;
const CELL_H: f32 = 38.0;
const KEY_PX: f32 = 26.0;
const LOOP_PAD: f32 = 3.0;
pub const NAME_MAX: usize = 16;
pub const GRID_COLS: usize = 9;
pub const GRID_ROWS: usize = 5;
const GRID: [&str; GRID_ROWS] = [
    "ABCDEFGHI",
    "JKLMNOPQR",
    "STUVWXYZ ",
    "012345678",
    "9-'!&.,()",
];
/// The row under the grid: the letters switch and END, each three cells wide.
const BOTTOM_CELLS: [usize; 2] = [0, 6];
const WIDE_CELLS: f32 = 3.0;

#[derive(Clone, PartialEq, Debug)]
pub struct MenuRow {
    pub label: String,
    pub value: String,
    /// ◀ ▶ round the value: Left and Right change it.
    pub arrows: bool,
}

/// The sheet's box (without its shadow margin) and turn on `canvas`.
fn sheet(canvas: Canvas, top: f32, w: f32, h: f32) -> Placed {
    Placed {
        x: canvas.centred(w),
        y: top,
        w,
        h,
    }
}

pub(crate) fn padded_pose(at: Placed, turn: f32) -> Pose {
    Pose {
        at: Placed {
            x: at.x - PAPER_PAD,
            y: at.y - PAPER_PAD,
            w: at.w + 2.0 * PAPER_PAD,
            h: at.h + 2.0 * PAPER_PAD,
        },
        turn: turn.to_radians(),
    }
}

pub fn menu_pose(canvas: Canvas) -> Pose {
    padded_pose(sheet(canvas, MENU_TOP, MENU_W, MENU_H), MENU_TURN)
}

pub fn entry_pose(canvas: Canvas) -> Pose {
    padded_pose(sheet(canvas, ENTRY_TOP, ENTRY_W, ENTRY_H), ENTRY_TURN)
}

/// A box drawn on a turned sheet: `local` is in the sheet's own pixels.
pub(crate) fn on_sheet(sheet: Placed, turn: f32, local: Placed) -> Pose {
    let (s, c) = turn.to_radians().sin_cos();
    let (dx, dy) = (
        local.x + local.w / 2.0 - sheet.w / 2.0,
        local.y + local.h / 2.0 - sheet.h / 2.0,
    );
    let (cx, cy) = (
        sheet.x + sheet.w / 2.0 + c * dx - s * dy,
        sheet.y + sheet.h / 2.0 + s * dx + c * dy,
    );
    Pose {
        at: Placed {
            x: cx - local.w / 2.0,
            y: cy - local.h / 2.0,
            w: local.w,
            h: local.h,
        },
        turn: turn.to_radians(),
    }
}

/// The band's loop round the row standing `slot` rows below the first one shown; a fraction
/// glides it between two.
pub fn menu_loop_pose(canvas: Canvas, slot: f32) -> Pose {
    let local = Placed {
        x: LOOP_INSET,
        y: ROW_TOP + slot * ROW_H,
        w: MENU_W - 2.0 * LOOP_INSET,
        h: ROW_H,
    };
    grown_pose(on_sheet(
        sheet(canvas, MENU_TOP, MENU_W, MENU_H),
        MENU_TURN,
        local,
    ))
}

/// The loop's texture reaches past its box by half its stroke and a pixel of soft edge.
pub(crate) fn grown_pose(p: Pose) -> Pose {
    let by = LOOP_STROKE / 2.0 + 1.0;
    Pose {
        at: Placed {
            x: p.at.x - by,
            y: p.at.y - by,
            w: p.at.w + 2.0 * by,
            h: p.at.h + 2.0 * by,
        },
        turn: p.turn,
    }
}

/// The loop for a menu row, a letter, or a wide key.
pub fn loop_face(w: f32, h: f32) -> CartFace {
    let by = LOOP_STROKE / 2.0 + 1.0;
    let (fw, fh) = ((w + 2.0 * by) * ART_SCALE, (h + 2.0 * by) * ART_SCALE);
    let (fw, fh) = (fw.ceil() as u32, fh.ceil() as u32);
    let r = (LOOP_RADIUS.min(h / 2.0)) * ART_SCALE;
    let (hw, hh) = (w * ART_SCALE / 2.0, h * ART_SCALE / 2.0);
    let half = LOOP_STROKE * ART_SCALE / 2.0;
    let mut rgba = Vec::with_capacity((fw * fh * 4) as usize);
    for y in 0..fh {
        for x in 0..fw {
            let px = x as f32 + 0.5 - fw as f32 / 2.0;
            let py = y as f32 + 0.5 - fh as f32 / 2.0;
            let qx = px.abs() - (hw - r);
            let qy = py.abs() - (hh - r);
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
            let edge = outside + qx.max(qy).min(0.0) - r;
            let cover = (half + 0.5 - edge.abs()).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[LOOP_INK[0], LOOP_INK[1], LOOP_INK[2], (cover * 255.0) as u8]);
        }
    }
    CartFace { rgba, w: fw, h: fh }
}

pub fn menu_loop_face() -> CartFace {
    loop_face(MENU_W - 2.0 * LOOP_INSET, ROW_H)
}

pub fn key_loop_face(wide: bool) -> CartFace {
    let w = CELL_W * if wide { WIDE_CELLS } else { 1.0 } - 2.0 * LOOP_PAD;
    loop_face(w, CELL_H - 2.0 * LOOP_PAD)
}

/// Bilinear resize, for paper stretched to a sheet of another shape.
pub(crate) fn stretched(src: &CartFace, w: u32, h: u32) -> CartFace {
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let (sw, sh) = (src.w as f32, src.h as f32);
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * sh / h as f32 - 0.5).clamp(0.0, sh - 1.0);
        let (y0, ty) = (fy.floor() as u32, fy.fract());
        let y1 = (y0 + 1).min(src.h - 1);
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * sw / w as f32 - 0.5).clamp(0.0, sw - 1.0);
            let (x0, tx) = (fx.floor() as u32, fx.fract());
            let x1 = (x0 + 1).min(src.w - 1);
            let at =
                |xx: u32, yy: u32, k: usize| src.rgba[((yy * src.w + xx) * 4) as usize + k] as f32;
            for k in 0..4 {
                let top = at(x0, y0, k) * (1.0 - tx) + at(x1, y0, k) * tx;
                let bottom = at(x0, y1, k) * (1.0 - tx) + at(x1, y1, k) * tx;
                rgba[((y * w + x) * 4) as usize + k] =
                    (top * (1.0 - ty) + bottom * ty).round() as u8;
            }
        }
    }
    CartFace { rgba, w, h }
}

pub(crate) fn blank_sheet(paper: Platform, w: f32, h: f32) -> Option<CartFace> {
    let art = paper_art(paper)?;
    let fw = ((w + 2.0 * PAPER_PAD) * ART_SCALE) as u32;
    let fh = ((h + 2.0 * PAPER_PAD) * ART_SCALE) as u32;
    Some(stretched(&art, fw, fh))
}

#[derive(Copy, Clone)]
pub(crate) enum Align {
    Left,
    Centre,
    Right,
}

/// The hand a sheet is written in: the marker for the menu, the label font for the name entry,
/// whose capitals and small letters must look different.
#[derive(Copy, Clone)]
pub(crate) enum Hand {
    Marker,
    Print,
}

/// Writes one line, shrunk to fit `max_w` and cut short with dots past its smallest size. `x` is
/// the left edge, the centre or the right edge, as aligned; all in the sheet's own pixels.
/// Returns the width written, in sheet pixels.
pub(crate) fn ink(
    face: &mut CartFace,
    hand: Hand,
    line: &str,
    px: f32,
    max_w: f32,
    x: f32,
    top: f32,
    align: Align,
) -> f32 {
    let font = match hand {
        Hand::Marker => text::marker_font(),
        Hand::Print => text::label_font(),
    };
    let Some(font) = font else {
        return 0.0;
    };
    let max_w = max_w * ART_SCALE;
    let mut px = px * ART_SCALE;
    let min_px = px * 0.7;
    let mut shown = line.to_string();
    while px > min_px && text::line_width(font, &shown, px, 0.0) > max_w {
        px -= 1.0;
    }
    while text::line_width(font, &shown, px, 0.0) > max_w && shown.chars().count() > 1 {
        let keep: String = shown.trim_end_matches('.').chars().collect();
        let mut keep: Vec<char> = keep.chars().collect();
        keep.pop();
        shown = keep.into_iter().collect::<String>().trim_end().to_string() + "..";
    }
    let layout = text::Layout {
        lines: vec![shown.clone()],
        px,
        tracking: 0.0,
    };
    let line_w = text::line_width(font, &shown, px, 0.0);
    // Marker strokes reach past their advance: leave room on both sides.
    let pad = (px * 0.5).ceil();
    let width = (line_w + 2.0 * pad).ceil() as u32;
    let band_h = (px * 1.5).ceil() as u32;
    let cover = text::coverage_in(font, width, band_h, &layout);
    let anchor = (PAPER_PAD + x) * ART_SCALE;
    let left = match align {
        Align::Left => anchor - pad,
        Align::Centre => anchor - width as f32 / 2.0,
        Align::Right => anchor - line_w - pad,
    }
    .max(0.0) as u32;
    let top = ((PAPER_PAD + top) * ART_SCALE - (band_h as f32 - px * 1.4) / 2.0).max(0.0) as u32;
    for (i, a) in cover.into_iter().enumerate() {
        let (cx, cy) = (left + i as u32 % width, top + i as u32 / width);
        if a == 0 || cx >= face.w || cy >= face.h {
            continue;
        }
        let at = ((cy * face.w + cx) * 4) as usize;
        let a = a as f32 / 255.0;
        for (k, &c) in INK.iter().enumerate() {
            let d = face.rgba[at + k] as f32;
            face.rgba[at + k] = (c as f32 * a + d * (1.0 - a)).round() as u8;
        }
    }
    line_w / ART_SCALE
}

/// A solid triangle pointing left or right, centred on (`x`, `y`) in sheet pixels.
pub(crate) fn arrow(face: &mut CartFace, x: f32, y: f32, right: bool) {
    let (cx, cy) = ((PAPER_PAD + x) * ART_SCALE, (PAPER_PAD + y) * ART_SCALE);
    let r = ARROW * ART_SCALE;
    for py in (cy - r) as i32..=(cy + r) as i32 {
        for px in (cx - r) as i32..=(cx + r) as i32 {
            let (dx, dy) = (px as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
            let along = if right { dx } else { -dx };
            // Inside when the half-height at this depth covers dy.
            let reach = (r - along) * 0.6;
            let cover = (reach - dy.abs() + 0.5).clamp(0.0, 1.0) * (along + r).clamp(0.0, 1.0);
            if cover <= 0.0 || px < 0 || py < 0 || px as u32 >= face.w || py as u32 >= face.h {
                continue;
            }
            let at = ((py as u32 * face.w + px as u32) * 4) as usize;
            for (k, &c) in INK.iter().enumerate() {
                let d = face.rgba[at + k] as f32;
                face.rgba[at + k] = (c as f32 * cover + d * (1.0 - cover)).round() as u8;
            }
        }
    }
}

/// The menu sheet: the folder's name at the top, then the rows from `first`, as many as fit.
pub fn folder_menu_face(
    paper: Platform,
    name: &str,
    rows: &[MenuRow],
    first: usize,
) -> Option<CartFace> {
    let mut face = blank_sheet(paper, MENU_W, MENU_H)?;
    let wide = MENU_W - 2.0 * ROW_SIDE;
    ink(
        &mut face,
        Hand::Marker,
        name,
        MENU_NAME_PX,
        wide,
        MENU_W / 2.0,
        MENU_NAME_TOP,
        Align::Centre,
    );
    let line_top = (ROW_H - ROW_PX * 1.4) / 2.0;
    let value_room = wide - LABEL_W - 2.0 * (2.0 * ARROW + ARROW_GAP);
    for (slot, row) in rows.iter().skip(first).take(MENU_ROWS_SHOWN).enumerate() {
        let top = ROW_TOP + slot as f32 * ROW_H;
        let mid = top + ROW_H / 2.0;
        ink(
            &mut face,
            Hand::Marker,
            &row.label,
            ROW_PX,
            LABEL_W,
            ROW_SIDE,
            top + line_top,
            Align::Left,
        );
        let right = MENU_W - ROW_SIDE;
        let value_right = match row.arrows {
            true => right - 2.0 * ARROW - ARROW_GAP,
            false => right,
        };
        let w = ink(
            &mut face,
            Hand::Marker,
            &row.value,
            ROW_PX,
            value_room,
            value_right,
            top + line_top,
            Align::Right,
        );
        if row.arrows {
            arrow(&mut face, value_right - w - ARROW_GAP - ARROW, mid, false);
            arrow(&mut face, right - ARROW, mid, true);
        }
    }
    if first > 0 {
        scroll_mark(&mut face, ROW_TOP - 8.0, true);
    }
    if rows.len() > first + MENU_ROWS_SHOWN {
        scroll_mark(
            &mut face,
            ROW_TOP + MENU_ROWS_SHOWN as f32 * ROW_H + 6.0,
            false,
        );
    }
    Some(face)
}

/// A small triangle at the sheet's centre, pointing up or down, where rows are out of sight.
fn scroll_mark(face: &mut CartFace, y: f32, up: bool) {
    let (cx, cy) = (
        (PAPER_PAD + MENU_W / 2.0) * ART_SCALE,
        (PAPER_PAD + y) * ART_SCALE,
    );
    let r = 5.0 * ART_SCALE;
    for py in (cy - r) as i32..=(cy + r) as i32 {
        for px in (cx - 2.0 * r) as i32..=(cx + 2.0 * r) as i32 {
            let (dx, dy) = (px as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
            let depth = if up { dy + r } else { r - dy };
            let cover = ((depth * 1.4 - dx.abs()) + 0.5).clamp(0.0, 1.0)
                * (r - dy.abs() + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 || px < 0 || py < 0 || px as u32 >= face.w || py as u32 >= face.h {
                continue;
            }
            let at = ((py as u32 * face.w + px as u32) * 4) as usize;
            for (k, &c) in INK.iter().enumerate() {
                let d = face.rgba[at + k] as f32;
                face.rgba[at + k] =
                    (c as f32 * cover * 0.7 + d * (1.0 - cover * 0.7)).round() as u8;
            }
        }
    }
}

/// What a press of A does where the cursor stands in the name entry.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EntryKey {
    Char(char),
    Letters,
    End,
}

/// The cursor: column and row of the grid, or row `GRID_ROWS` for the switch (0) and END (1).
pub type Cursor = (usize, usize);

pub const END_KEY: Cursor = (1, GRID_ROWS);

pub fn entry_key(lower: bool, (col, row): Cursor) -> EntryKey {
    if row == GRID_ROWS {
        return if col == 0 {
            EntryKey::Letters
        } else {
            EntryKey::End
        };
    }
    let c = GRID[row].chars().nth(col).unwrap_or(' ');
    EntryKey::Char(if lower { c.to_ascii_lowercase() } else { c })
}

/// The cursor after a step of the D-pad; it wraps at every edge.
pub fn step_cursor((col, row): Cursor, dx: i32, dy: i32) -> Cursor {
    let rows = GRID_ROWS as i32 + 1;
    if dy != 0 {
        let to = (row as i32 + dy).rem_euclid(rows) as usize;
        return match (row == GRID_ROWS, to == GRID_ROWS) {
            (false, true) => (usize::from(col >= GRID_COLS / 2), to),
            (true, false) => (BOTTOM_CELLS[col.min(1)] + 1, to),
            _ => (col, to),
        };
    }
    if row == GRID_ROWS {
        return ((col as i32 + dx).rem_euclid(2) as usize, row);
    }
    ((col as i32 + dx).rem_euclid(GRID_COLS as i32) as usize, row)
}

fn key_box((col, row): Cursor) -> (Placed, bool) {
    let left = (ENTRY_W - GRID_COLS as f32 * CELL_W) / 2.0;
    let wide = row == GRID_ROWS;
    let x = left
        + CELL_W
            * if wide {
                BOTTOM_CELLS[col.min(1)] as f32
            } else {
                col as f32
            };
    let y = GRID_TOP + row as f32 * CELL_H + if wide { 8.0 } else { 0.0 };
    let w = CELL_W * if wide { WIDE_CELLS } else { 1.0 };
    (Placed { x, y, w, h: CELL_H }, wide)
}

/// Where the loop stands round the key under the cursor, and whether that key is a wide one.
pub fn entry_loop_pose(canvas: Canvas, cursor: Cursor) -> (Pose, bool) {
    let (cell, wide) = key_box(cursor);
    let local = Placed {
        x: cell.x + LOOP_PAD,
        y: cell.y + LOOP_PAD,
        w: cell.w - 2.0 * LOOP_PAD,
        h: cell.h - 2.0 * LOOP_PAD,
    };
    let sheet = sheet(canvas, ENTRY_TOP, ENTRY_W, ENTRY_H);
    (grown_pose(on_sheet(sheet, ENTRY_TURN, local)), wide)
}

/// The name entry sheet: the name so far over a line of slots, the letters, the switch and END.
pub fn name_entry_face(paper: Platform, name: &str, lower: bool) -> Option<CartFace> {
    let mut face = blank_sheet(paper, ENTRY_W, ENTRY_H)?;
    let slots_left = (ENTRY_W - NAME_MAX as f32 * SLOT_W) / 2.0;
    let chars: Vec<char> = name.chars().collect();
    for slot in 0..NAME_MAX {
        let centre = slots_left + (slot as f32 + 0.5) * SLOT_W;
        if let Some(c) = chars.get(slot) {
            ink(
                &mut face,
                Hand::Print,
                &c.to_string(),
                SLOT_PX,
                SLOT_W,
                centre,
                SLOTS_TOP,
                Align::Centre,
            );
        }
        if slot >= chars.len() {
            ink(
                &mut face,
                Hand::Print,
                "_",
                SLOT_PX * 0.8,
                SLOT_W,
                centre,
                SLOTS_TOP + 14.0,
                Align::Centre,
            );
        }
    }
    for row in 0..GRID_ROWS {
        for col in 0..GRID_COLS {
            let EntryKey::Char(c) = entry_key(lower, (col, row)) else {
                continue;
            };
            let (cell, _) = key_box((col, row));
            let shown = if c == ' ' {
                "SP".to_string()
            } else {
                c.to_string()
            };
            let px = if c == ' ' { KEY_PX * 0.6 } else { KEY_PX };
            let top = cell.y + (CELL_H - px * 1.4) / 2.0;
            ink(
                &mut face,
                Hand::Print,
                &shown,
                px,
                CELL_W,
                cell.x + CELL_W / 2.0,
                top,
                Align::Centre,
            );
        }
    }
    for (col, word) in [(0, if lower { "ABC" } else { "abc" }), (1, "END")] {
        let (cell, _) = key_box((col, GRID_ROWS));
        let top = cell.y + (CELL_H - KEY_PX * 1.4) / 2.0;
        ink(
            &mut face,
            Hand::Print,
            word,
            KEY_PX,
            cell.w,
            cell.x + cell.w / 2.0,
            top,
            Align::Centre,
        );
    }
    Some(face)
}
