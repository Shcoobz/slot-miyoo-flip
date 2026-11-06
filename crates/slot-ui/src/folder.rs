//! A folder of games on the shelf: its first three carts held by a rubber band, with the folder's
//! name on a scrap of squared paper under the band (`design_handoff_shelf_folders`, turn 7).

use std::sync::OnceLock;

use slot_gfx::{Draw, TexId};
use slot_store::Platform;

use crate::art::decode_png_bytes;
use crate::board::Placed;
use crate::cart::CartFace;
use crate::text;

const PAPER_GBA_PNG: &[u8] = include_bytes!("../../../assets/folder_paper_gba.png");
const PAPER_GB_PNG: &[u8] = include_bytes!("../../../assets/folder_paper_gb.png");
const PAPER_GBC_PNG: &[u8] = include_bytes!("../../../assets/folder_paper_gbc.png");
const BAND_PNG: &[u8] = include_bytes!("../../../assets/folder_band.png");
const TITLE_PNG: &[u8] = include_bytes!("../../../assets/folder_title.png");

/// The art is drawn at twice the design's size, with room for its drop shadow on every side.
pub(crate) const ART_SCALE: f32 = 2.0;
pub(crate) const PAPER_PAD: f32 = 12.0;
const TITLE_PAD: f32 = 12.0;
/// `folder_band.png` holds a 324x12 band at x 8, y 6 of 340x24.
const BAND_ART: (f32, f32, f32, f32) = (8.0, 6.0, 324.0, 12.0);

pub(crate) const INK: [u8; 3] = [0x25, 0x22, 0x1d];
const NAME_MIN_PX: f32 = 18.0;
const TITLE_PX: f32 = 24.0;

/// Where a stack's parts sit, in the design's 720x480 canvas around one cart box.
struct StackSpec {
    /// The cart box the stack stands in: a GBA cart 360x212, a Game Boy pak 312x337, an NES cart
    /// 290x340, an SNES cart 360x240.
    cart_box: Placed,
    /// Back, middle and front cart: centre x, centre y, scale, turn in degrees.
    carts: [(f32, f32, f32, f32); 3],
    /// The band's left, top, width and turn; it is 12 tall.
    band: (f32, f32, f32, f32),
    /// The paper's left, top, width, height and turn, before its nudge of (-9, 5).
    paper: (f32, f32, f32, f32, f32),
    name_px: f32,
    name_top: f32,
}

const GBA_STACK: StackSpec = StackSpec {
    cart_box: Placed {
        x: 180.0,
        y: 134.0,
        w: 360.0,
        h: 212.0,
    },
    carts: [
        (369.0, 205.5, 0.806, 7.0),
        (347.0, 227.5, 0.806, -5.0),
        (360.0, 252.5, 0.833, 0.0),
    ],
    band: (200.0, 246.0, 324.0, -3.0),
    paper: (240.0, 150.0, 240.0, 152.0, -8.0),
    name_px: 42.0,
    name_top: 26.0,
};

const GB_STACK: StackSpec = StackSpec {
    cart_box: Placed {
        x: 204.0,
        y: 43.0,
        w: 312.0,
        h: 337.0,
    },
    carts: [
        (374.0, 200.0, 0.80, 6.0),
        (344.0, 214.0, 0.80, -5.0),
        (360.0, 232.0, 0.85, 0.0),
    ],
    band: (204.0, 226.0, 316.0, -2.0),
    paper: (238.0, 82.0, 244.0, 240.0, -7.0),
    name_px: 46.0,
    name_top: 58.0,
};

/// An NES folder stands like a Game Boy one, in the 290x340 box the design gives the NES cart
/// (x 215, y 40): the carts at the same centres and scales, the band across the box at the same
/// height, and the same paper, centred.
const NES_STACK: StackSpec = StackSpec {
    cart_box: Placed {
        x: 215.0,
        y: 40.0,
        w: 290.0,
        h: 340.0,
    },
    carts: [
        (374.0, 198.0, 0.80, 6.0),
        (344.0, 212.0, 0.80, -5.0),
        (360.0, 230.0, 0.85, 0.0),
    ],
    band: (215.0, 225.0, 290.0, -2.0),
    paper: (238.0, 80.0, 244.0, 240.0, -7.0),
    name_px: 46.0,
    name_top: 58.0,
};

/// An SNES folder stands like a GBA one, in the 360x240 box the design gives the SNES cart
/// (x 180, y 106): the GBA stack's centres stretched down by 240/212, the same paper.
const SNES_STACK: StackSpec = StackSpec {
    cart_box: Placed {
        x: 180.0,
        y: 106.0,
        w: 360.0,
        h: 240.0,
    },
    carts: [
        (369.0, 187.0, 0.806, 7.0),
        (347.0, 212.0, 0.806, -5.0),
        (360.0, 240.0, 0.833, 0.0),
    ],
    band: (200.0, 233.0, 324.0, -3.0),
    paper: (240.0, 124.0, 240.0, 152.0, -8.0),
    name_px: 42.0,
    name_top: 26.0,
};

const PAPER_NUDGE: (f32, f32) = (-9.0, 5.0);

fn spec(platform: Platform) -> &'static StackSpec {
    match platform {
        Platform::Gba => &GBA_STACK,
        Platform::Gb | Platform::Gbc => &GB_STACK,
        Platform::Nes => &NES_STACK,
        Platform::Snes => &SNES_STACK,
    }
}

/// The textures one folder on the shelf is drawn from.
#[derive(Copy, Clone, Default, PartialEq, Debug)]
pub struct Stack {
    /// The folder's first three carts, front (the cover) first.
    pub carts: [Option<TexId>; 3],
    pub paper: Option<TexId>,
    pub band: Option<TexId>,
}

/// Where a part of the stack is drawn: its box (before turning) and its turn in radians.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Pose {
    pub at: Placed,
    pub turn: f32,
}

/// The stack's parts standing in `slot`, the box the shelf gives a cart of the cover's platform.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct StackPoses {
    /// Front (the cover), middle, back.
    pub carts: [Pose; 3],
    pub paper: Pose,
    pub band: Pose,
}

pub fn stack_poses(platform: Platform, slot: Placed) -> StackPoses {
    let s = spec(platform);
    let k = slot.w / s.cart_box.w;
    let at = |x: f32, y: f32| {
        (
            slot.x + (x - s.cart_box.x) * k,
            slot.y + (y - s.cart_box.y) * k,
        )
    };
    let cart = |(cx, cy, scale, deg): (f32, f32, f32, f32)| {
        let (x, y) = at(cx, cy);
        let (w, h) = (s.cart_box.w * scale * k, s.cart_box.h * scale * k);
        Pose {
            at: Placed {
                x: x - w / 2.0,
                y: y - h / 2.0,
                w,
                h,
            },
            turn: deg.to_radians(),
        }
    };
    let (px, py, pw, ph, paper_deg) = s.paper;
    let (x, y) = at(
        px + PAPER_NUDGE.0 - PAPER_PAD,
        py + PAPER_NUDGE.1 - PAPER_PAD,
    );
    let paper = Pose {
        at: Placed {
            x,
            y,
            w: (pw + 2.0 * PAPER_PAD) * k,
            h: (ph + 2.0 * PAPER_PAD) * k,
        },
        turn: paper_deg.to_radians(),
    };
    let (bx, by, bw, band_deg) = s.band;
    let (ax, ay, aw, ah) = BAND_ART;
    let stretch = bw / aw;
    let (x, y) = at(bx - ax * stretch, by - ay);
    let band = Pose {
        at: Placed {
            x,
            y,
            w: (aw + 2.0 * ax) * stretch * k,
            h: (ah + 2.0 * ay) * k,
        },
        turn: band_deg.to_radians(),
    };
    StackPoses {
        carts: [cart(s.carts[2]), cart(s.carts[1]), cart(s.carts[0])],
        paper,
        band,
    }
}

pub fn draw_pose(pose: Pose, tex: TexId, alpha: f32, out: &mut Vec<Draw>) {
    out.push(Draw::Turned {
        x: pose.at.x,
        y: pose.at.y,
        w: pose.at.w,
        h: pose.at.h,
        tex,
        alpha,
        turn: pose.turn,
    });
}

/// Draws the stack standing in `slot`, the box the shelf gives a cart of the cover's platform.
pub fn draw_stack(
    platform: Platform,
    slot: Placed,
    stack: &Stack,
    alpha: f32,
    out: &mut Vec<Draw>,
) {
    let poses = stack_poses(platform, slot);
    for k in (0..3).rev() {
        if let Some(tex) = stack.carts[k] {
            draw_pose(poses.carts[k], tex, alpha, out);
        }
    }
    if let Some(tex) = stack.paper {
        draw_pose(poses.paper, tex, alpha, out);
    }
    if let Some(tex) = stack.band {
        draw_pose(poses.band, tex, alpha, out);
    }
}

fn art(png: &'static [u8], cell: &'static OnceLock<Option<CartFace>>) -> Option<CartFace> {
    cell.get_or_init(|| {
        let (rgba, w, h) = decode_png_bytes(png)?;
        (rgba.len() == (w * h * 4) as usize).then_some(CartFace { rgba, w, h })
    })
    .clone()
}

/// The paper for a folder of `platform` with `name` written on it: GBA 6a, Game Boy 6b, Game Boy
/// Color 6c.
/// The torn paper itself, without a name: GBA 6a, Game Boy 6b, Game Boy Color 6c.
pub(crate) fn paper_art(paper: Platform) -> Option<CartFace> {
    static GBA: OnceLock<Option<CartFace>> = OnceLock::new();
    static GB: OnceLock<Option<CartFace>> = OnceLock::new();
    static GBC: OnceLock<Option<CartFace>> = OnceLock::new();
    match paper {
        Platform::Gba => art(PAPER_GBA_PNG, &GBA),
        Platform::Gb => art(PAPER_GB_PNG, &GB),
        Platform::Gbc => art(PAPER_GBC_PNG, &GBC),
        // Auto paper: an NES folder takes the Game Boy's cut paper, an SNES folder the GBA's
        // light one, as their stacks do.
        Platform::Nes => art(PAPER_GB_PNG, &GB),
        Platform::Snes => art(PAPER_GBA_PNG, &GBA),
    }
}

/// The paper for a folder of `platform` with `name` written on it, torn like `paper`'s.
pub fn folder_paper_face(platform: Platform, paper: Platform, name: &str) -> Option<CartFace> {
    let s = spec(platform);
    let (pw, ph) = (s.paper.2, s.paper.3);
    let mut face = paper_art(paper)?;
    if paper_art(platform).is_some_and(|own| (own.w, own.h) != (face.w, face.h)) {
        let w = ((pw + 2.0 * PAPER_PAD) * ART_SCALE) as u32;
        let h = ((ph + 2.0 * PAPER_PAD) * ART_SCALE) as u32;
        face = crate::folder_menu::stretched(&face, w, h);
    }
    let top = (PAPER_PAD + s.name_top) * ART_SCALE;
    write_name(
        &mut face,
        name,
        s.name_px * ART_SCALE,
        top,
        pw * ART_SCALE * 0.88,
    );
    Some(face)
}

/// The widest the name strip grows for a long name, in canvas pixels.
const TITLE_MAX_W: f32 = 300.0;

/// The small paper an opened folder keeps at the top of the screen, with its name. A name too
/// long for the strip at its smallest size stretches the strip to fit, up to `TITLE_MAX_W`.
pub fn folder_title_face(name: &str) -> Option<CartFace> {
    static TITLE: OnceLock<Option<CartFace>> = OnceLock::new();
    let mut face = art(TITLE_PNG, &TITLE)?;
    let px = TITLE_PX * ART_SCALE;
    let margin = 2.0 * (TITLE_PAD + 8.0) * ART_SCALE;
    if let Some(font) = text::marker_font() {
        let small = NAME_MIN_PX * ART_SCALE;
        let need = text::line_width(font, &name.to_uppercase(), small, text::tracking_for(small));
        let room = face.w as f32 - margin;
        if need > room {
            let most = (TITLE_MAX_W + 2.0 * TITLE_PAD) * ART_SCALE;
            let w = (need + margin).ceil().min(most) as u32;
            face = crate::folder_menu::stretched(&face, w, face.h);
        }
    }
    let line_h = px * 1.4;
    let top = (face.h as f32 - line_h) / 2.0;
    let max_w = face.w as f32 - margin;
    write_name(&mut face, name, px, top, max_w);
    Some(face)
}

pub fn folder_band_face() -> Option<CartFace> {
    static BAND: OnceLock<Option<CartFace>> = OnceLock::new();
    art(BAND_PNG, &BAND)
}

/// A title strip's size in canvas pixels, from its face's width (wider for a long name).
pub fn folder_title_size(face_w: u32) -> (f32, f32) {
    (face_w as f32 / ART_SCALE, 40.0 + 2.0 * TITLE_PAD)
}

/// Writes `name` in marker ink, one line, centred across the face with its top at `top`.
fn write_name(face: &mut CartFace, name: &str, px: f32, top: f32, max_w: f32) {
    let Some(font) = text::marker_font() else {
        return;
    };
    let layout = text::fit(font, name, max_w, 1, px, NAME_MIN_PX * ART_SCALE);
    let line_h = (layout.px * 1.4).ceil() as u32;
    let top = top.max(0.0) as u32;
    let band = line_h.min(face.h.saturating_sub(top));
    let cover = text::coverage_in(font, face.w, band, &layout);
    for (i, a) in cover.into_iter().enumerate() {
        if a == 0 {
            continue;
        }
        let at = ((top * face.w) as usize + i) * 4;
        let a = a as f32 / 255.0;
        for (k, &c) in INK.iter().enumerate() {
            let d = face.rgba[at + k] as f32;
            face.rgba[at + k] = (c as f32 * a + d * (1.0 - a)).round() as u8;
        }
        face.rgba[at + 3] = face.rgba[at + 3].max((a * 255.0) as u8);
    }
}
