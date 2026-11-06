//! The in-game menu's Cheats page: the playing game's cheats from its `.cht`, each On or Off.
//! A game has as many cheats as its file lists, which the menu's fixed pages of `QuickRow`s
//! cannot hold, so the page is one row, `QuickRow::CheatList`, and this module keeps which
//! cheat it points at and draws the list the way the menu draws its own rows.

use std::ops::Range;

use slot_gfx::Compositor;
use slot_input::{Action, Btn};
use slot_store::{Cheat, Platform};
use slot_ui::{
    quick_value_face, Draw, Millis, QuickRow, QuickValue, TexId, MENU_PAD, QUICK_EDGE,
    QUICK_LABEL_X, QUICK_PITCH, QUICK_VISIBLE,
};

/// The menu's own spacing, as `slot_ui::QuickMenu` lays its rows out.
const LEGEND_Y: f32 = 427.0;
const TYPE_DROP: f32 = 4.0;
const POINTER_GAP: f32 = 14.0;
const POINTER_BLEED: f32 = 4.0;
const CARET_GAP: f32 = 14.0;
const SCRIM: f32 = 0.65;
/// A description longer than this is cut short, so it stays clear of the On/Off beside it.
const LABEL_CHARS: usize = 22;
/// How long the pointer rests on a cut-short cheat before its whole description scrolls by.
const MARQUEE_WAIT_MS: Millis = 1_000;
/// How fast it scrolls, and how long it holds at the end before starting over.
const MARQUEE_PX_PER_S: f32 = 60.0;
const MARQUEE_HOLD_MS: Millis = 1_200;
/// The room between the end of the description and the ◀ before the On/Off.
const MARQUEE_GAP: f32 = 12.0;

type Face = [(TexId, u32, u32); 2];

#[derive(Default)]
pub(super) struct Cheats {
    /// The game they belong to, for writing the player's choices back.
    game: Option<(Platform, String)>,
    list: Vec<Cheat>,
    /// The cheat the page points at.
    at: usize,
    /// A cheat was switched and the core has not been told yet.
    pending: bool,
    /// Dim and lit descriptions of the rows on screen, by their place in `list`. Only those
    /// are kept: a `.cht` can list hundreds.
    faces: Vec<(usize, Face)>,
    /// The whole description of the cheat pointed at, lit, when the row cuts it short.
    full: Option<(usize, (TexId, u32, u32))>,
    /// When the pointer came to rest on the cheat it points at.
    since: Millis,
}

/// How far a description `over` pixels wider than its room has scrolled `rested` ms after the
/// pointer came to it: still for `MARQUEE_WAIT_MS`, then along to its end, held there, and back
/// to the start to go again.
pub(super) fn marquee_offset(over: f32, rested: Millis) -> f32 {
    if over <= 0.0 || rested < MARQUEE_WAIT_MS {
        return 0.0;
    }
    let run = (over / MARQUEE_PX_PER_S * 1000.0) as Millis;
    let lap = run + MARQUEE_HOLD_MS + MARQUEE_WAIT_MS;
    let t = (rested - MARQUEE_WAIT_MS) % lap;
    match t {
        t if t < run => t as f32 / 1000.0 * MARQUEE_PX_PER_S,
        t if t < run + MARQUEE_HOLD_MS => over,
        _ => 0.0,
    }
}

/// The description as the row shows it, cut short with an ellipsis when it is long.
fn shown(desc: &str) -> String {
    let desc = desc.trim();
    if desc.chars().count() <= LABEL_CHARS {
        return desc.to_string();
    }
    let cut: String = desc.chars().take(LABEL_CHARS - 3).collect();
    format!("{}...", cut.trim_end())
}

fn enabled(list: &[Cheat]) -> Vec<(u32, String)> {
    list.iter()
        .filter(|c| c.enabled)
        .map(|c| (c.index, c.code.clone()))
        .collect()
}

fn push(out: &mut Vec<Draw>, tex: TexId, x: f32, y: f32, w: u32, h: u32) {
    out.push(Draw::Tex {
        x: x.round(),
        y: y.round(),
        w: w as f32,
        h: h as f32,
        tex,
        alpha: 1.0,
    });
}

impl super::App {
    /// Reads the seated game's cheats as it opens, and hands back the ones that are on for the
    /// core to start with.
    pub fn load_cheats(&mut self) -> Vec<(u32, String)> {
        let game = self.seated_cart().map(|c| (c.platform, c.stem.clone()));
        let list = match (self.root.as_deref(), &game) {
            (Some(root), Some((platform, stem))) => slot_store::read_cheats(root, *platform, stem),
            _ => Vec::new(),
        };
        let old = std::mem::take(&mut self.cheats.faces);
        self.orphaned
            .extend(old.into_iter().flat_map(|(_, f)| f.map(|(tex, _, _)| tex)));
        self.orphaned
            .extend(self.cheats.full.take().map(|(_, (tex, _, _))| tex));
        self.cheats = Cheats {
            game,
            list,
            ..Cheats::default()
        };
        // Two linked units must run the same machine: no cheats while a link is up.
        match self.link_active() {
            true => Vec::new(),
            false => enabled(&self.cheats.list),
        }
    }

    /// The cheats that are on, once after each switch, for the core to take the whole set.
    pub fn take_cheats(&mut self) -> Option<Vec<(u32, String)>> {
        let pending = std::mem::take(&mut self.cheats.pending);
        (pending && !self.link_active()).then(|| enabled(&self.cheats.list))
    }

    /// The Cheats row shows only for a game with a `.cht`.
    pub(super) fn has_cheats(&self) -> bool {
        !self.cheats.list.is_empty() && !self.link_active()
    }

    pub(super) fn open_cheats(&mut self) {
        if self.has_cheats() {
            self.cheats.at = 0;
            self.cheats.since = self.now();
            self.in_game_menu = Some(QuickRow::CheatList);
        }
    }

    /// Up and Down pick a cheat, wrapping as the menu's pages do; Left, Right and A switch it;
    /// B goes back to the Cheats row.
    pub(super) fn cheat_list_input(&mut self, action: Action) {
        let count = self.cheats.list.len();
        match action {
            Action::GbaDown(Btn::Up) if count > 0 => {
                self.cheats.at = (self.cheats.at + count - 1) % count;
                self.cheats.since = self.now();
            }
            Action::GbaDown(Btn::Down) if count > 0 => {
                self.cheats.at = (self.cheats.at + 1) % count;
                self.cheats.since = self.now();
            }
            Action::GbaDown(Btn::Left | Btn::Right | Btn::A) => self.switch_cheat(),
            Action::GbaDown(Btn::B) => self.in_game_menu = Some(QuickRow::Cheats),
            Action::QuickMenu => self.tap_in_game_menu(None),
            Action::Polaroids => self.double_tap_states(),
            _ => {}
        }
    }

    fn switch_cheat(&mut self) {
        let Some(cheat) = self.cheats.list.get_mut(self.cheats.at) else {
            return;
        };
        cheat.enabled = !cheat.enabled;
        self.cheats.pending = true;
        let (Some(root), Some((platform, stem))) = (&self.root, &self.cheats.game) else {
            return;
        };
        let written = slot_store::write_cheat_choices(root, *platform, stem, &self.cheats.list);
        if let Err(e) = &written {
            eprintln!("slot: cheats: {e}");
        }
        self.card_writes.wrote("cheats", &written);
    }

    /// The cheats on screen: a page of them around the one pointed at.
    fn cheat_window(&self) -> Range<usize> {
        let count = self.cheats.list.len();
        let fits = QUICK_VISIBLE.min(count);
        let at = self.cheats.at.min(count.saturating_sub(1));
        let start = at.saturating_sub(fits / 2).min(count - fits);
        start..start + fits
    }

    /// Builds the descriptions of the cheats on screen while the page is open, and lets go of
    /// the rest.
    pub fn sync_cheat_faces(&mut self, compositor: &mut Compositor) {
        let window = match self.in_game_menu {
            Some(QuickRow::CheatList) => self.cheat_window(),
            _ => 0..0,
        };
        let (keep, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut self.cheats.faces)
            .into_iter()
            .partition(|(i, _)| window.contains(i));
        self.orphaned
            .extend(gone.into_iter().flat_map(|(_, f)| f.map(|(tex, _, _)| tex)));
        self.cheats.faces = keep;
        let open = !window.is_empty();
        for i in window {
            if self.cheats.faces.iter().any(|(at, _)| *at == i) {
                continue;
            }
            let text = shown(&self.cheats.list[i].desc);
            let face = [false, true].map(|lit| {
                let f = quick_value_face(&text, lit);
                (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h)
            });
            self.cheats.faces.push((i, face));
        }
        let long = open
            .then_some(self.cheats.at)
            .filter(|&at| shown(&self.cheats.list[at].desc) != self.cheats.list[at].desc.trim());
        if self.cheats.full.map(|(at, _)| at) != long {
            self.orphaned
                .extend(self.cheats.full.take().map(|(_, (tex, _, _))| tex));
            self.cheats.full = long.map(|at| {
                let f = quick_value_face(self.cheats.list[at].desc.trim(), true);
                (at, (compositor.create_texture(f.w, f.h, &f.rgba), f.w, f.h))
            });
        }
    }

    /// The page over the game: each cheat's description on the left, its On/Off on the right,
    /// with the pointer and the ◀ ▶ carets on the one picked.
    pub(super) fn draw_cheat_list(&self, footer_top: f32, out: &mut Vec<Draw>) {
        let canvas = self.canvas();
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: canvas.w,
            h: footer_top,
            colour: [0.0, 0.0, 0.0, SCRIM],
        });
        let Some(faces) = self.quick_menu_faces.as_ref() else {
            return;
        };
        let window = self.cheat_window();
        let top = (LEGEND_Y - QUICK_PITCH * window.len() as f32) / 2.0;
        let (right, pad) = (canvas.w - QUICK_EDGE, MENU_PAD as f32);
        let x = QUICK_LABEL_X - pad;
        let [(_, lw, _), (_, rw, _)] = faces.carets;
        let on_w = faces.values.iter().map(|v| v[1].1).max().unwrap_or(0);
        // Where the widest On/Off's ◀ begins, as the lit row places it below.
        let caret = right - rw as f32 - 2.0 * CARET_GAP - on_w as f32 - lw as f32 + 2.0 * pad;
        let room = (caret - MARQUEE_GAP - x).max(1.0);
        let rested = self.now().saturating_sub(self.cheats.since);
        for (row, i) in window.enumerate() {
            let lit = i == self.cheats.at;
            let y = top + QUICK_PITCH * row as f32 + TYPE_DROP;
            let full = self.cheats.full.filter(|(at, _)| lit && *at == i);
            match full {
                Some((_, (tex, w, h))) if rested >= MARQUEE_WAIT_MS => {
                    let shown_w = room.min(w as f32);
                    let offset = marquee_offset(w as f32 - shown_w, rested);
                    out.push(Draw::Part {
                        x: x.round(),
                        y: y.round(),
                        w: shown_w,
                        h: h as f32,
                        tex,
                        alpha: 1.0,
                        uv: [offset / w as f32, 0.0, shown_w / w as f32, 1.0],
                    });
                }
                _ => {
                    if let Some((_, face)) = self.cheats.faces.iter().find(|(at, _)| *at == i) {
                        let (tex, w, h) = face[lit as usize];
                        push(out, tex, x, y, w, h);
                    }
                }
            }
            if lit {
                let (tex, pw, ph) = faces.pointer;
                push(
                    out,
                    tex,
                    x + pad - POINTER_GAP - pw as f32,
                    y - POINTER_BLEED,
                    pw,
                    ph,
                );
            }
            let value = QuickValue::flag(self.cheats.list[i].enabled);
            let Some(&(tex, w, h)) = faces.values.get(value.index()).map(|v| &v[lit as usize])
            else {
                continue;
            };
            if !lit {
                push(out, tex, right + pad - w as f32, y, w, h);
                continue;
            }
            let [(left_tex, lw, lh), (right_tex, rw, rh)] = faces.carets;
            let rx = right - rw as f32;
            push(out, right_tex, rx, y, rw, rh);
            let vx = rx - CARET_GAP + pad - w as f32;
            push(out, tex, vx, y, w, h);
            push(out, left_tex, vx + pad - CARET_GAP - lw as f32, y, lw, lh);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_name_waits_a_second_then_scrolls_to_its_end_holds_and_starts_over() {
        let over = 120.0;
        assert_eq!(marquee_offset(over, 0), 0.0);
        assert_eq!(marquee_offset(over, MARQUEE_WAIT_MS - 1), 0.0);
        let half = marquee_offset(over, MARQUEE_WAIT_MS + 1_000);
        assert!(
            (half - MARQUEE_PX_PER_S).abs() < 1.0,
            "a second in it is at {half}"
        );
        let end = MARQUEE_WAIT_MS + 2_000 + MARQUEE_HOLD_MS / 2;
        assert_eq!(marquee_offset(over, end), over);
        let again = MARQUEE_WAIT_MS + 2_000 + MARQUEE_HOLD_MS + 10;
        assert_eq!(marquee_offset(over, again), 0.0);
        assert_eq!(
            marquee_offset(0.0, 10_000),
            0.0,
            "a name that fits never moves"
        );
    }
}
