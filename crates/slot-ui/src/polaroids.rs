use slot_gfx::{Canvas, Draw, TexId};
use slot_power::Battery;
use slot_store::{parse_stamp, StateEntry};

use crate::art;
use crate::footer::{draw_corners, Printed};
use crate::hud::{PLATE, PLATE_H};
use crate::plate::{hint_quad, hint_row, hint_width, Hint, HINT_GAP, HINT_H, TITLE_H, TITLE_W};

pub const PHOTO_W: u32 = 240;
pub const PHOTO_H: u32 = 160;

const BLANK: [u8; 3] = [0x3a, 0x3a, 0x3e];

pub const DOT: f32 = 6.0;
const DOT_GAP: f32 = 10.0;
const DOT_DIM: f32 = 0.35;

const MARGIN: f32 = 16.0;

pub const LEGEND: [(&str, &str); 3] = [("B", "Back"), ("Y", "Delete"), ("A", "Load")];
pub const GAMES_LEGEND: [(&str, &str); 4] = [
    ("B", "Back"),
    ("Y", "Remove"),
    ("X", "States"),
    ("A", "Resume"),
];
const WAYS_OUT: usize = 2;
const UNDO_KEY: &str = "X";

pub struct PhotoFace {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
    /// A Game Boy picture at (40, 8) in the black frame; the shelf shows only that part.
    pub game_boy: bool,
}

/// Where a Game Boy picture sits in the GBA-sized frame a thumb is taken from.
const GB_PICTURE: (u32, u32, u32, u32) = (40, 8, 160, 144);

/// The Game Boy picture cut out of a thumb whose border around it is black, else None.
fn game_boy_picture(src: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let (x0, y0, pw, ph) = GB_PICTURE;
    if (w, h) != (PHOTO_W, PHOTO_H) {
        return None;
    }
    let dark = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        src[i] as u32 + src[i + 1] as u32 + src[i + 2] as u32 <= 24
    };
    let sides = (0..h).all(|y| (0..x0).chain(x0 + pw..w).all(|x| dark(x, y)));
    let caps = (x0..x0 + pw).all(|x| (0..y0).chain(y0 + ph..h).all(|y| dark(x, y)));
    if !(sides && caps) {
        return None;
    }
    let mut out = Vec::with_capacity((pw * ph * 4) as usize);
    for y in y0..y0 + ph {
        let at = ((y * w + x0) * 4) as usize;
        out.extend_from_slice(&src[at..at + (pw * 4) as usize]);
    }
    Some(out)
}

/// The thumb as taken, its size, and whether it holds a Game Boy picture in its black border. It
/// is never resampled here: the shelf draws the Game Boy part alone at 10:9 and a NES or SNES
/// picture whole at 4:3, and the game screen shows it like a running game, so each picture is
/// scaled once, pixel for pixel.
fn photo_rgba(thumb: &std::path::Path) -> Option<(Vec<u8>, u32, u32, bool)> {
    let (src, w, h) = art::decode(thumb)?;
    let game_boy = game_boy_picture(&src, w, h).is_some();
    Some((src, w, h, game_boy))
}

pub fn photo_face(entry: &StateEntry) -> PhotoFace {
    let (rgba, w, h, game_boy) = photo_rgba(&entry.thumb).unwrap_or_else(|| {
        let blank = std::iter::repeat_n(
            [BLANK[0], BLANK[1], BLANK[2], 255],
            (PHOTO_W * PHOTO_H) as usize,
        )
        .flatten()
        .collect();
        (blank, PHOTO_W, PHOTO_H, false)
    });
    PhotoFace {
        rgba,
        w,
        h,
        game_boy,
    }
}

pub struct Polaroids {
    pub entries: Vec<StateEntry>,
    pub index: usize,
    faces: Vec<TexId>,
    /// Which faces hold a Game Boy picture in a GBA-sized frame.
    game_boy: Vec<bool>,
    /// Each face's size: a GBA frame, or a NES or SNES picture at its own.
    sizes: Vec<(u32, u32)>,
    title: Option<TexId>,
    hint_faces: Vec<TexId>,
    undo: Option<String>,
    legend: &'static [(&'static str, &'static str)],
}

impl Polaroids {
    pub fn new(entries: Vec<StateEntry>) -> Self {
        Polaroids {
            entries,
            index: 0,
            faces: Vec::new(),
            game_boy: Vec::new(),
            sizes: Vec::new(),
            title: None,
            hint_faces: Vec::new(),
            undo: None,
            legend: &LEGEND,
        }
    }

    pub fn games(entries: Vec<StateEntry>) -> Self {
        Polaroids {
            legend: &GAMES_LEGEND,
            ..Self::new(entries)
        }
    }

    pub fn set_game_boy_faces(&mut self, game_boy: Vec<bool>) {
        self.game_boy = game_boy;
    }

    pub fn set_face_sizes(&mut self, sizes: Vec<(u32, u32)>) {
        self.sizes = sizes;
    }

    pub fn set_faces(&mut self, faces: Vec<TexId>) {
        self.faces = faces;
    }

    pub fn set_title_face(&mut self, face: Option<TexId>) {
        self.title = face;
    }

    pub fn set_hint_faces(&mut self, faces: Vec<TexId>) {
        self.hint_faces = faces;
    }

    pub fn set_undo(&mut self, label: Option<&str>) {
        self.undo = label.map(str::to_string);
    }

    pub fn hints(&self) -> Vec<Hint> {
        let mut out = hint_row(self.legend);
        if let Some(label) = &self.undo {
            out.push(Hint {
                key: UNDO_KEY,
                label: label.clone(),
            });
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn selected(&self) -> Option<&StateEntry> {
        self.entries.get(self.index)
    }

    pub fn dots(&self) -> (usize, usize) {
        (self.entries.len(), self.index)
    }

    pub fn title(&self, now: &str) -> String {
        match self.selected() {
            Some(e) => Self::relative_time(&e.stamp, now),
            None => String::new(),
        }
    }

    pub fn remove_selected(&mut self) {
        if self.index >= self.entries.len() {
            return;
        }
        self.entries.remove(self.index);
        if self.index < self.faces.len() {
            self.faces.remove(self.index);
        }
        if self.index < self.game_boy.len() {
            self.game_boy.remove(self.index);
        }
        if self.index < self.sizes.len() {
            self.sizes.remove(self.index);
        }
        self.index = self.index.min(self.entries.len().saturating_sub(1));
    }

    pub fn left(&mut self) {
        self.index = self.index.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.index = (self.index + 1).min(self.entries.len().saturating_sub(1));
    }

    pub fn draw(
        &self,
        battery: Option<Battery>,
        battery_percent: Printed,
        bolt: Option<TexId>,
        clock: Printed,
        out: &mut Vec<Draw>,
    ) {
        self.draw_photo(Canvas::FRAMED, out);
        self.draw_chrome(Canvas::FRAMED, battery, battery_percent, bolt, clock, out);
    }

    pub fn draw_chrome(
        &self,
        canvas: Canvas,
        battery: Option<Battery>,
        battery_percent: Printed,
        bolt: Option<TexId>,
        clock: Printed,
        out: &mut Vec<Draw>,
    ) {
        self.draw_top(canvas, battery, battery_percent, bolt, clock, out);
        self.draw_bottom(canvas, out);
    }

    pub fn draw_photo(&self, canvas: Canvas, out: &mut Vec<Draw>) {
        let (w, h) = (canvas.w, canvas.h);
        out.push(match self.faces.get(self.index) {
            Some(tex) => Draw::Shot { tex: *tex },
            None => blank(0.0, 0.0, w, h),
        });
    }

    /// The photo as a print between the plates, for when no game screen is up.
    pub fn draw_print(&self, canvas: Canvas, out: &mut Vec<Draw>) {
        let game_boy = self.game_boy.get(self.index).copied().unwrap_or(false);
        let size = self
            .sizes
            .get(self.index)
            .copied()
            .unwrap_or((PHOTO_W, PHOTO_H));
        let (_, _, gw, gh) = GB_PICTURE;
        // A NES or SNES picture was drawn for a 4:3 television, whatever its pixel count.
        let (pw, ph) = match (game_boy, size == (PHOTO_W, PHOTO_H)) {
            (true, _) => (gw as f32, gh as f32),
            (false, true) => (PHOTO_W as f32, PHOTO_H as f32),
            (false, false) => (4.0, 3.0),
        };
        let room = canvas.h - 2.0 * PLATE_H;
        let w = canvas.w.min(room * pw / ph);
        let h = w * ph / pw;
        let (x, y) = (canvas.centred(w), PLATE_H + (room - h) / 2.0);
        out.push(match (self.faces.get(self.index), game_boy) {
            (Some(tex), true) => {
                let (gx, gy, _, _) = GB_PICTURE;
                let (fw, fh) = (PHOTO_W as f32, PHOTO_H as f32);
                Draw::Part {
                    x,
                    y,
                    w,
                    h,
                    tex: *tex,
                    alpha: 1.0,
                    uv: [gx as f32 / fw, gy as f32 / fh, pw / fw, ph / fh],
                }
            }
            (Some(tex), false) => Draw::Tex {
                x,
                y,
                w,
                h,
                tex: *tex,
                alpha: 1.0,
            },
            (None, _) => blank(x, y, w, h),
        });
    }

    fn draw_top(
        &self,
        canvas: Canvas,
        battery: Option<Battery>,
        battery_percent: Printed,
        bolt: Option<TexId>,
        clock: Printed,
        out: &mut Vec<Draw>,
    ) {
        out.push(plate(canvas, 0.0));
        if let Some(tex) = self.title {
            out.push(Draw::Tex {
                x: canvas.centred(TITLE_W as f32),
                y: (PLATE_H - TITLE_H as f32) / 2.0,
                w: TITLE_W as f32,
                h: TITLE_H as f32,
                tex,
                alpha: 1.0,
            });
        }

        draw_corners(canvas, battery, battery_percent, bolt, clock, out);
    }

    fn draw_bottom(&self, canvas: Canvas, out: &mut Vec<Draw>) {
        let top = canvas.h - PLATE_H;
        out.push(plate(canvas, top));
        let y = top + (PLATE_H - HINT_H as f32) / 2.0;
        let hints = self.hints();
        let widths: Vec<f32> = hints
            .iter()
            .map(|h| hint_width(h.key, &h.label) as f32)
            .collect();
        let tail = &widths[WAYS_OUT..];
        let tail_w = tail.iter().sum::<f32>() + HINT_GAP * tail.len().saturating_sub(1) as f32;
        let mut x = MARGIN;
        for (i, w) in widths.iter().enumerate() {
            if i == WAYS_OUT {
                x = canvas.w - MARGIN - tail_w;
            }
            out.push(hint_quad(x, y, *w, self.hint_faces.get(i).copied()));
            x += w + HINT_GAP;
        }
        self.draw_dots(canvas, top, out);
    }

    fn draw_dots(&self, canvas: Canvas, top: f32, out: &mut Vec<Draw>) {
        let n = self.entries.len();
        let row = n as f32 * DOT + (n as f32 - 1.0).max(0.0) * DOT_GAP;
        let mut x = canvas.centred(row);
        for i in 0..n {
            out.push(Draw::Rect {
                x,
                y: top + (PLATE_H - DOT) / 2.0,
                w: DOT,
                h: DOT,
                colour: [1.0, 1.0, 1.0, if i == self.index { 1.0 } else { DOT_DIM }],
            });
            x += DOT + DOT_GAP;
        }
    }

    pub fn relative_time(stamp: &str, now: &str) -> String {
        const RELATIVE_WINDOW: i64 = 12 * 3600;
        let (Some(then), Some(parsed)) = (parse_stamp(stamp), parse_stamp(now)) else {
            return stamp.to_string();
        };
        let delta = parsed - then;
        if delta < 60 {
            return "just now".into();
        }
        if delta < 3600 {
            return format!("{} min ago", delta / 60);
        }
        if delta < RELATIVE_WINDOW {
            return format!("{} hr ago", delta / 3600);
        }
        format!("{} {}:{}", &stamp[..10], &stamp[11..13], &stamp[14..16])
    }
}

fn blank(x: f32, y: f32, w: f32, h: f32) -> Draw {
    Draw::Rect {
        x,
        y,
        w,
        h,
        colour: [
            BLANK[0] as f32 / 255.0,
            BLANK[1] as f32 / 255.0,
            BLANK[2] as f32 / 255.0,
            1.0,
        ],
    }
}

fn plate(canvas: Canvas, y: f32) -> Draw {
    Draw::Rect {
        x: 0.0,
        y,
        w: canvas.w,
        h: PLATE_H,
        colour: PLATE,
    }
}
