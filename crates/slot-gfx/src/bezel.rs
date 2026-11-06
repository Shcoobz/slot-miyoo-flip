use crate::surface::{OUT_H, OUT_W};

pub fn framed(panel: (u32, u32)) -> bool {
    panel.0 > 0 && panel.0 * 3 == panel.1 * 4
}

pub fn screen_rect(panel: (u32, u32)) -> (i32, i32, i32, i32) {
    let w = panel.0;
    let h = (w * OUT_H + OUT_W / 2) / OUT_W;
    ((panel.0 - w) as i32 / 2, 0, w as i32, h as i32)
}

pub fn game_rect(panel: (u32, u32), sp_frame: bool) -> (i32, i32, i32, i32) {
    let (x, _, w, h) = screen_rect(panel);
    let gap = panel.1 as i32 - h;
    let y = match sp_frame {
        true => 0,
        false => gap - gap / 2,
    };
    (x, y, w, h)
}

/// The game at the panel's full height, as wide as `aspect` allows, centred. A 4:3 panel shows
/// a Game Boy picture this way instead of inside the GBA-shaped screen.
pub fn tall_rect(panel: (u32, u32), aspect: f32) -> (i32, i32, i32, i32) {
    let h = panel.1;
    let w = ((h as f32 * aspect).round() as u32).min(panel.0);
    ((panel.0 - w) as i32 / 2, 0, w as i32, h as i32)
}

pub fn lifted_rect(panel: (u32, u32), lift: f32) -> (i32, i32, i32, i32) {
    let (x, _, w, h) = screen_rect(panel);
    let t = lift.clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    let drop = ((panel.1 as i32 - h) as f32 * (1.0 - eased)).round() as i32;
    (x, drop, w, h)
}

/// How a TV console's (NES, SNES) picture fills its area.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TvFit {
    /// The 4:3 a television showed, as big as the area allows: the whole 640x480 Flip panel.
    Tv,
    /// The largest whole multiple of the picture's dots, centred, black around it: sharp pixels.
    Sharp,
}

/// A frame's picture in the dots a television showed: SNES hi-res (512 wide) puts two pixels
/// in a dot and interlace (448 tall) two lines in a line, so 512x448 is 256x224 dots.
pub fn tv_dots(frame: (u32, u32)) -> (u32, u32) {
    let w = if frame.0 > 320 { frame.0 / 2 } else { frame.0 };
    let h = if frame.1 > 256 { frame.1 / 2 } else { frame.1 };
    (w.max(1), h.max(1))
}

/// Where a TV console's `frame` sits in `area` (the panel, or the 720x480 canvas), as
/// (x, y from the top, w, h). `Tv` fills 4:3; `Sharp` scales the dots by a whole number:
/// a NES at 2x is 512x480, a SNES 512x448.
pub fn tv_rect(area: (u32, u32), frame: (u32, u32), fit: TvFit) -> (i32, i32, i32, i32) {
    let (aw, ah) = area;
    let (w, h) = match fit {
        TvFit::Tv => {
            let w = (ah * 4 / 3).min(aw);
            (w, (w * 3 / 4).min(ah))
        }
        TvFit::Sharp => {
            let (dw, dh) = tv_dots(frame);
            let s = (aw / dw).min(ah / dh).max(1);
            ((dw * s).min(aw), (dh * s).min(ah))
        }
    };
    (
        (aw.saturating_sub(w) / 2) as i32,
        (ah.saturating_sub(h) / 2) as i32,
        w as i32,
        h as i32,
    )
}
