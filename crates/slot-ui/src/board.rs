use slot_gfx::Canvas;
use slot_store::{Cart, Core, Platform};

use crate::bitmaps::{self, Mould, NES_CART_H, NES_CART_W, SNES_CART_H, SNES_CART_W};
use crate::cart::{clean_label, mould_of, open_board, open_gb_board, CartFace, CART_H, CART_W};
use crate::shelf::{foot_y, shelf_box};
use crate::silhouette::GbShell;
use crate::slot_chrome::ease;
use crate::text;

pub const BOARD_W: u32 = CART_W;
pub const BOARD_H: u32 = CART_H;

/// The black ROM chip on `assets/board.png`: x 61..132, y 48..138.
pub const ROM_X: u32 = 61;
pub const ROM_Y: u32 = 48;
pub const ROM_W: u32 = 71;
pub const ROM_H: u32 = 90;

const MARK_LINE_CHARS: usize = 10;
const MARK_LINES: usize = 3;
const MARK_PAD: u32 = 4;
const MARK_PX: f32 = 10.0;
const MARK_MIN_PX: f32 = 6.0;
const MARK_INK: [u8; 3] = [0xbd, 0xbd, 0xbd];

pub fn rom_marking(stem: &str) -> Vec<String> {
    let mut title: Vec<String> = Vec::new();
    for word in clean_label(stem).to_uppercase().split_whitespace() {
        match title.last_mut() {
            Some(line) if line.len() + 1 + word.len() <= MARK_LINE_CHARS => {
                line.push(' ');
                line.push_str(word);
            }
            _ => title.push(word.to_string()),
        }
    }
    if title.len() > MARK_LINES {
        let tail = title.split_off(MARK_LINES - 1).join(" ");
        title.push(tail);
    }
    title
}

pub fn rom_marking_face(stem: &str) -> CartFace {
    marking_face(stem, ROM_W, ROM_H)
}

/// The Game Boy board's ROM chip, `gb_board_open.png` x 192..244, y 166..257: the marking is
/// set in a 94x52 box and turned to read up the chip's long side.
const GB_ROM_LONG: u32 = 94;
const GB_ROM_SHORT: u32 = 52;
const GB_ROM_X: u32 = 218 - GB_ROM_SHORT / 2;
const GB_ROM_Y: u32 = 211 - GB_ROM_LONG / 2;

/// The NES board's ROM chip, where the design's open view (2c) puts it on the 290x340 sheet: the
/// board at x 20, y 132 and the chip at x 168, y 16 on it, 64x58.
const NES_ROM: (u32, u32, u32, u32) = (188, 148, 64, 58);
/// The SNES board's ROM chip on the 360x240 sheet (2d): the board at x 30, y 36 and the chip at
/// x 22, y 22 on it, 104x56.
const SNES_ROM: (u32, u32, u32, u32) = (52, 58, 104, 56);

fn marking_face(stem: &str, w: u32, h: u32) -> CartFace {
    let title = rom_marking(stem);
    let mut face = CartFace {
        rgba: vec![0; (w * h * 4) as usize],
        w,
        h,
    };
    let Some(font) = text::label_font() else {
        return face;
    };
    let max_w = (w - 2 * MARK_PAD) as f32;
    let line_h = (MARK_PX * 1.25).ceil() as u32;
    let mut top = h.saturating_sub(line_h * title.len() as u32) / 2;
    for line in &title {
        let layout = text::fit(font, line, max_w, 1, MARK_PX, MARK_MIN_PX);
        ink_band(&mut face, top, line_h, &layout, MARK_INK, 1.0);
        top += line_h;
    }
    face
}

pub fn board_face(cart: &Cart) -> CartFace {
    let mould = mould_of(cart);
    let (mut face, mark, (x, y)) = match mould {
        Mould::Gba => {
            let face = CartFace {
                rgba: bitmaps::board().to_vec(),
                w: BOARD_W,
                h: BOARD_H,
            };
            (face, rom_marking_face(&cart.stem), (ROM_X, ROM_Y))
        }
        Mould::Notched | Mould::Rounded => {
            let shell = match mould {
                Mould::Rounded => GbShell::Rounded,
                _ => GbShell::Notched,
            };
            let mark = turned_left(&marking_face(&cart.stem, GB_ROM_LONG, GB_ROM_SHORT));
            (open_gb_board(shell), mark, (GB_ROM_X, GB_ROM_Y))
        }
        Mould::Nes | Mould::Snes => {
            let (x, y, w, h) = match mould {
                Mould::Nes => NES_ROM,
                _ => SNES_ROM,
            };
            (open_board(mould), marking_face(&cart.stem, w, h), (x, y))
        }
    };
    over(&mut face, &mark, x, y);
    face
}

fn ink_band(
    face: &mut CartFace,
    top: u32,
    band: u32,
    layout: &text::Layout,
    ink: [u8; 3],
    strength: f32,
) {
    let band = band.min(face.h.saturating_sub(top));
    for (i, a) in text::coverage(face.w, band, layout).into_iter().enumerate() {
        let a = (a as f32 * strength).round() as u8;
        if a == 0 {
            continue;
        }
        let at = ((top * face.w) as usize + i) * 4;
        let (a, d_a) = (a as f32 / 255.0, face.rgba[at + 3] as f32 / 255.0);
        let out_a = a + d_a * (1.0 - a);
        for (k, &c) in ink.iter().enumerate() {
            let d_rgb = face.rgba[at + k] as f32 / 255.0;
            let out_rgb = (c as f32 / 255.0 * a + d_rgb * d_a * (1.0 - a)) / out_a;
            face.rgba[at + k] = (out_rgb * 255.0).round() as u8;
        }
        face.rgba[at + 3] = (out_a * 255.0).round() as u8;
    }
}

/// A quarter turn anticlockwise: the face's left edge ends up at the bottom.
fn turned_left(src: &CartFace) -> CartFace {
    let (w, h) = (src.h, src.w);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let s = (((x * src.w) + (src.w - 1 - y)) * 4) as usize;
            let d = ((y * w + x) * 4) as usize;
            rgba[d..d + 4].copy_from_slice(&src.rgba[s..s + 4]);
        }
    }
    CartFace { rgba, w, h }
}

fn over(dst: &mut CartFace, src: &CartFace, x: u32, y: u32) {
    for row in 0..src.h {
        for col in 0..src.w {
            let s = ((row * src.w + col) * 4) as usize;
            let a = src.rgba[s + 3] as u32;
            let (dx, dy) = (x + col, y + row);
            if a == 0 || dx >= dst.w || dy >= dst.h {
                continue;
            }
            let d = ((dy * dst.w + dx) * 4) as usize;
            for k in 0..3 {
                dst.rgba[d + k] =
                    ((src.rgba[s + k] as u32 * a + dst.rgba[d + k] as u32 * (255 - a)) / 255) as u8;
            }
            dst.rgba[d + 3] = dst.rgba[d + 3].max(a as u8);
        }
    }
}

fn bitmap_face(rgba: &[u8], w: u32, h: u32) -> CartFace {
    CartFace {
        rgba: rgba.to_vec(),
        w,
        h,
    }
}

pub fn board_x(canvas: Canvas) -> f32 {
    canvas.centred(BOARD_W as f32)
}

pub const BOARD_Y: f32 = 150.0;

pub const TURN_PAD: u32 = 2;

pub const LID_TURN: f32 = -5.0 * std::f32::consts::PI / 180.0;
pub const CHIP_TIP: f32 = 4.0 * std::f32::consts::PI / 180.0;
pub const HOP_LIFT: f32 = 10.0;

pub const SLIDE_SHARE: f32 = 160.0 / 420.0;
pub const SLIDE_UP: f32 = CART_H as f32 / 3.0;

pub fn slide_of(progress: f32) -> f32 {
    ease((progress / SLIDE_SHARE).clamp(0.0, 1.0))
}

pub fn lift_of(progress: f32) -> f32 {
    ease(((progress - SLIDE_SHARE) / (1.0 - SLIDE_SHARE)).clamp(0.0, 1.0))
}

pub const SOCKET_U: [f32; 2] = [99.5, 171.5];
pub const SOCKET_V: f32 = 59.1;
pub const SOCKET_W: u32 = 64;
pub const SOCKET_H: u32 = 46;

pub const CHIP_U: [f32; 2] = [101.0, 173.0];
pub const CHIP_V: f32 = 60.6;
pub const CHIP_W: u32 = 59;
pub const CHIP_H: u32 = 41;

pub const SHADOW_W: u32 = 66;
pub const SHADOW_H: u32 = 12;

const NAME_PX: f32 = 17.0;
const NAME_MIN_PX: f32 = 8.0;
const SOCKET_INK: [u8; 3] = [0xee, 0xf5, 0xe6];
const CHIP_INK: [u8; 3] = [0xf2, 0xf2, 0xf2];

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Placed {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub fn shelf_cart(canvas: Canvas) -> Placed {
    shelf_cart_at(canvas.centred(shelf_box(Platform::Gba).0), 1.0)
}

pub fn shelf_cart_at(x: f32, scale: f32) -> Placed {
    shelf_cart_of(Platform::Gba, x, scale)
}

/// Where the shelf draws the selected cart of this platform, at the left edge and scale the shelf
/// gives it.
pub fn shelf_cart_of(platform: Platform, x: f32, scale: f32) -> Placed {
    let (cw, ch) = shelf_box(platform);
    let (w, h) = (cw * scale, ch * scale);
    Placed {
        x,
        y: foot_y(ch) - h,
        w,
        h,
    }
}

pub fn board_at(canvas: Canvas, progress: f32) -> Placed {
    board_from(canvas, shelf_cart(canvas), progress)
}

pub fn board_from(canvas: Canvas, shelf: Placed, progress: f32) -> Placed {
    GBA_OPEN.board_from(canvas, shelf, progress)
}

/// The board and the back shell fill the cart they sit in, same size as the lid. Letterboxing them
/// to the board bitmap's own aspect made both look shorter the moment the cart opened.
pub fn board_in(cart: Placed) -> Placed {
    cart
}

pub fn lid_at(canvas: Canvas, progress: f32) -> (Placed, f32) {
    lid_from(canvas, shelf_cart(canvas), progress)
}

pub fn lid_from(canvas: Canvas, shelf: Placed, progress: f32) -> (Placed, f32) {
    GBA_OPEN.lid_from(canvas, shelf, progress)
}

const UNITS_W: f32 = 240.0;
const UNITS_H: f32 = 135.0;

pub fn on_board(board: Placed, u: f32, v: f32) -> (f32, f32) {
    (
        board.x + u * board.w / UNITS_W,
        board.y + v * board.h / UNITS_H,
    )
}

pub fn board_zoom(board: Placed) -> f32 {
    board.w / BOARD_W as f32
}

pub fn grown(p: Placed, by: f32) -> Placed {
    Placed {
        x: p.x - by,
        y: p.y - by,
        w: p.w + 2.0 * by,
        h: p.h + 2.0 * by,
    }
}

fn lerp(a: Placed, b: Placed, t: f32) -> Placed {
    Placed {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
        w: a.w + (b.w - a.w) * t,
        h: a.h + (b.h - a.h) * t,
    }
}

pub fn padded(face: &CartFace, pad: u32) -> CartFace {
    let (w, h) = (face.w + 2 * pad, face.h + 2 * pad);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let stride = (face.w * 4) as usize;
    for row in 0..face.h {
        let from = (row * face.w * 4) as usize;
        let to = (((row + pad) * w + pad) * 4) as usize;
        rgba[to..to + stride].copy_from_slice(&face.rgba[from..from + stride]);
    }
    CartFace { rgba, w, h }
}

pub fn socket_face(core: Core) -> CartFace {
    let mut face = bitmap_face(bitmaps::socket(), SOCKET_W, SOCKET_H);
    if let Some(layout) = name_layout(core, (SOCKET_W - 12) as f32) {
        ink_band(&mut face, 4, 37, &layout, SOCKET_INK, 0.5);
    }
    face
}

pub fn chip_face(core: Option<Core>) -> CartFace {
    let mut face = bitmap_face(bitmaps::chip(), CHIP_W, CHIP_H);
    if let Some(layout) = core.and_then(|c| name_layout(c, (CHIP_W - 8) as f32)) {
        ink_band(&mut face, 4, 34, &layout, CHIP_INK, 1.0);
    }
    padded(&face, TURN_PAD)
}

pub fn chip_shadow_face() -> CartFace {
    let (w, h) = (SHADOW_W, SHADOW_H);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 + 0.5 - w as f32 / 2.0) / (w as f32 / 2.0);
            let dy = (y as f32 + 0.5 - h as f32 / 2.0) / (h as f32 / 2.0);
            let fall = (1.0 - (dx * dx + dy * dy)).max(0.0);
            rgba.extend_from_slice(&[0, 0, 0, (fall * fall * 255.0) as u8]);
        }
    }
    CartFace { rgba, w, h }
}

fn name_layout(core: Core, max_w: f32) -> Option<text::Layout> {
    let font = text::label_font()?;
    let tracking = |px: f32| (px * 0.10).round();
    let mut px = NAME_PX;
    while px > NAME_MIN_PX && text::line_width(font, core.text(), px, tracking(px)) > max_w {
        px -= 1.0;
    }
    Some(text::Layout {
        lines: vec![core.text().to_string()],
        px,
        tracking: tracking(px),
    })
}

/// Where an opened cart's parts come to rest in the core picker. Socket, chip and hop positions are
/// in board units: `units` spans the whole board.
#[derive(Copy, Clone, Debug)]
pub struct OpenLayout {
    /// The board's size at rest, and the width of its bitmap, which sockets and chips are drawn
    /// against.
    pub board: (f32, f32),
    pub art_w: f32,
    pub board_y: f32,
    pub units: (f32, f32),
    pub socket_u: [f32; 2],
    pub socket_v: f32,
    pub chip_u: [f32; 2],
    pub chip_v: f32,
    pub hop: f32,
    /// How far the lid slides up out of the cart before it lifts away.
    pub slide: f32,
    /// The chip's shadow centre, from the chip's top left.
    pub shadow_at: (f32, f32),
    pub lid: LidRest,
    pub lid_shadow: (f32, f32, f32),
}

/// The lid's resting size and turn. `x` is its left edge on a 720-wide canvas; `None` centres it
/// over the board.
#[derive(Copy, Clone, Debug)]
pub struct LidRest {
    pub x: Option<f32>,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub turn: f32,
}

/// A GBA cart opens like a Game Boy pak: the lid set aside on the left, the board at the size the
/// shelf shows the cart, so it does not shrink as it opens.
pub const GBA_OPEN: OpenLayout = OpenLayout {
    board: (GBA_OPEN_W, GBA_OPEN_H),
    art_w: BOARD_W as f32,
    board_y: CORE_LEGEND_TOP - GBA_OPEN_H - 12.0,
    units: (UNITS_W, UNITS_H),
    socket_u: SOCKET_U,
    socket_v: SOCKET_V,
    chip_u: CHIP_U,
    chip_v: CHIP_V,
    hop: HOP_LIFT,
    slide: SLIDE_UP,
    shadow_at: (19.0, 29.4),
    lid: LidRest {
        x: Some(35.0),
        y: CORE_LEGEND_TOP - 12.0 - GBA_OPEN_H / 2.0 - GBA_LID_H / 2.0,
        w: GBA_LID_W,
        h: GBA_LID_H,
        turn: LID_TURN,
    },
    lid_shadow: (175.0, 19.0, 28.0),
};

const GBA_OPEN_W: f32 = BOARD_W as f32 * crate::shelf::GBA_SHELF_ZOOM;
const GBA_OPEN_H: f32 = BOARD_H as f32 * crate::shelf::GBA_SHELF_ZOOM;
const GBA_LID_W: f32 = 150.0;
const GBA_LID_H: f32 = GBA_LID_W * CART_H as f32 / CART_W as f32;
/// Where the core picker's legend starts; the board rests just above it.
const CORE_LEGEND_TOP: f32 = 364.0;

/// A Game Boy pak opens upright: the board 1:1 under the lid's old place, the lid set aside on its
/// left. Units are board pixels.
pub const GB_OPEN: OpenLayout = OpenLayout {
    board: (GB_BOARD_W, GB_BOARD_H),
    art_w: GB_BOARD_W,
    board_y: 20.0,
    units: (GB_BOARD_W, GB_BOARD_H),
    socket_u: [54.0, 120.0],
    socket_v: 166.0,
    chip_u: [56.5, 122.5],
    chip_v: 168.5,
    hop: HOP_LIFT * BOARD_W as f32 / UNITS_W,
    slide: GB_BOARD_H / 3.0,
    shadow_at: (CHIP_W as f32 / 2.0, 44.0),
    lid: LidRest {
        x: Some(35.0),
        y: 109.0,
        w: 150.0,
        h: 162.0,
        turn: -6.0 * std::f32::consts::PI / 180.0,
    },
    lid_shadow: (175.0, 19.0, 28.0),
};

const GB_BOARD_W: f32 = crate::cart::GB_CART_W as f32;
const GB_BOARD_H: f32 = crate::cart::GB_CART_H as f32;
const LID_GAP: f32 = 19.0;

/// An NES cart opens like a Game Boy pak (`NES SNES Carts.dc.html`, 2c): the back shell and its
/// board 1:1 where the cart stood, the lid at half size on the left, turned -6°. Units are pixels
/// of the 290x340 board sheet, whose board sits at x 20, y 132; the sockets at x 18 and 90, y 18 on
/// it, the chip 2.5 in from a socket's corner.
pub const NES_OPEN: OpenLayout = OpenLayout {
    board: (NES_BOARD_W, NES_BOARD_H),
    art_w: NES_BOARD_W,
    board_y: 18.0,
    units: (NES_BOARD_W, NES_BOARD_H),
    socket_u: [38.0, 110.0],
    socket_v: 150.0,
    chip_u: [40.5, 112.5],
    chip_v: 152.5,
    hop: HOP_LIFT * BOARD_W as f32 / UNITS_W,
    slide: NES_BOARD_H / 3.0,
    shadow_at: (CHIP_W as f32 / 2.0, 44.0),
    lid: LidRest {
        x: Some(44.0),
        y: 112.0,
        w: NES_BOARD_W / 2.0,
        h: NES_BOARD_H / 2.0,
        turn: -6.0 * std::f32::consts::PI / 180.0,
    },
    lid_shadow: (180.0, 19.0, 28.0),
};

const NES_BOARD_W: f32 = NES_CART_W as f32;
const NES_BOARD_H: f32 = NES_CART_H as f32;

/// An SNES cart opens like a GBA cart (2d): the back shell and its board at the size the shelf
/// shows the cart (450x300, its 360x240 face at the GBA zoom), so it does not shrink as it opens,
/// resting just above the legend, and the lid at half the face's size on the left, turned -6°.
/// Units are pixels of the 360x240 board sheet, whose board sits at x 30, y 36; the sockets at
/// x 146 and 218, y 24 on it.
pub const SNES_OPEN: OpenLayout = OpenLayout {
    board: (SNES_OPEN_W, SNES_OPEN_H),
    art_w: SNES_CART_W as f32,
    board_y: CORE_LEGEND_TOP - SNES_OPEN_H - 12.0,
    units: (SNES_CART_W as f32, SNES_CART_H as f32),
    socket_u: [176.0, 248.0],
    socket_v: 60.0,
    chip_u: [178.5, 250.5],
    chip_v: 62.5,
    hop: HOP_LIFT * BOARD_W as f32 / UNITS_W,
    slide: SNES_CART_H as f32 / 3.0,
    shadow_at: (CHIP_W as f32 / 2.0, 44.0),
    lid: LidRest {
        x: Some(20.0),
        y: CORE_LEGEND_TOP - 12.0 - SNES_OPEN_H / 2.0 - SNES_LID_H / 2.0,
        w: SNES_LID_W,
        h: SNES_LID_H,
        turn: -6.0 * std::f32::consts::PI / 180.0,
    },
    lid_shadow: (190.0, 19.0, 8.0),
};

const SNES_OPEN_W: f32 = SNES_CART_W as f32 * crate::shelf::GBA_SHELF_ZOOM;
const SNES_OPEN_H: f32 = SNES_CART_H as f32 * crate::shelf::GBA_SHELF_ZOOM;
const SNES_LID_W: f32 = SNES_CART_W as f32 / 2.0;
const SNES_LID_H: f32 = SNES_CART_H as f32 / 2.0;

pub fn open_layout(platform: Platform) -> &'static OpenLayout {
    match platform {
        Platform::Gba => &GBA_OPEN,
        Platform::Gb | Platform::Gbc => &GB_OPEN,
        Platform::Nes => &NES_OPEN,
        Platform::Snes => &SNES_OPEN,
    }
}

impl OpenLayout {
    /// The board's and the lid's left edges. The design centres the board on a 720 canvas with the
    /// lid beside it; where that does not fit, the pair is centred together.
    fn lefts(&self, canvas: Canvas) -> (f32, f32) {
        let board = canvas.centred(self.board.0);
        match self.lid.x {
            None => (board, canvas.centred(self.lid.w)),
            Some(x) if board >= x + self.lid.w + LID_GAP => (board, x),
            Some(_) => {
                let lid = canvas.centred(self.lid.w + LID_GAP + self.board.0);
                (lid + self.lid.w + LID_GAP, lid)
            }
        }
    }

    pub fn board_rest(&self, canvas: Canvas) -> Placed {
        Placed {
            x: self.lefts(canvas).0,
            y: self.board_y,
            w: self.board.0,
            h: self.board.1,
        }
    }

    pub fn lid_rest(&self, canvas: Canvas) -> Placed {
        Placed {
            x: self.lefts(canvas).1,
            y: self.lid.y,
            w: self.lid.w,
            h: self.lid.h,
        }
    }

    pub fn board_from(&self, canvas: Canvas, shelf: Placed, progress: f32) -> Placed {
        lerp(board_in(shelf), self.board_rest(canvas), lift_of(progress))
    }

    pub fn lid_from(&self, canvas: Canvas, shelf: Placed, progress: f32) -> (Placed, f32) {
        // The lid slides out of the cart, but at most two thirds of the way up to where it comes
        // to rest, so a cart standing high on the shelf does not overshoot and drop back.
        let rest = self.lid_rest(canvas);
        let room = shelf.y - rest.y;
        let slide = match room > 0.0 {
            true => self.slide.min(room * 2.0 / 3.0),
            false => self.slide,
        };
        let slid = Placed {
            y: shelf.y - slide * slide_of(progress),
            ..shelf
        };
        let lift = lift_of(progress);
        (
            lerp(slid, self.lid_rest(canvas), lift),
            self.lid.turn * lift,
        )
    }

    pub fn on_board(&self, board: Placed, u: f32, v: f32) -> (f32, f32) {
        (
            board.x + u * board.w / self.units.0,
            board.y + v * board.h / self.units.1,
        )
    }

    /// Board bitmap pixels to canvas pixels.
    pub fn zoom(&self, board: Placed) -> f32 {
        board.w / self.art_w
    }
}
