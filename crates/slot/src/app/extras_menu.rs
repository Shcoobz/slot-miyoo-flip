//! START on Get lucky or Never played: a sheet that sets which systems the pick draws from and
//! counts the games that leaves (`design_handoff_extra_shelf`, turn 3b and 3c). The item stays
//! where it stands, its neighbours step aside, and the sheet slides up over it.

use slot_gfx::Compositor;
use slot_input::{Action, Btn};
use slot_store::Platform;
use slot_ui::{draw_empty_slot, draw_pose, draw_slot_name, Draw, Millis, TexId};

use super::ease_out;

/// The sheet slides in and out over this long (design: 420 ms).
const MENU_MS: f32 = 420.0;
/// How far the neighbours step aside while the sheet is up.
const NEIGHBOUR_SHIFT: f32 = 260.0;
/// The settings in the order ◀ ▶ step through them: all systems, then each one.
const SYSTEMS: [Option<Platform>; 6] = [
    None,
    Some(Platform::Gba),
    Some(Platform::Gb),
    Some(Platform::Gbc),
    Some(Platform::Nes),
    Some(Platform::Snes),
];

#[derive(Default)]
pub(super) struct ExtrasMenu {
    /// The item it was opened on, when, and when it began to close.
    open: Option<(slot_ui::Extra, Millis, Option<Millis>)>,
    sheet: Option<TexId>,
    /// The sheet needs drawing again: it opened, or its setting changed.
    stale: bool,
    ring: Option<TexId>,
    legend: Vec<(TexId, u32)>,
}

impl super::App {
    /// START opens the menu on the two items that pick a game, once no pick is under way.
    pub(super) fn can_open_extras_menu(&self) -> bool {
        self.on_extras()
            && self.extra_motion.is_none()
            && self.selected_extra() != slot_ui::Extra::Activity
    }

    pub(super) fn open_extras_menu(&mut self) {
        self.extras_menu.open = Some((self.selected_extra(), self.now(), None));
        self.extras_menu.stale = true;
    }

    /// Whether the sheet is on the screen: rising, up, or still sliding away. It takes all input
    /// meanwhile.
    pub(super) fn extras_menu_shown(&self) -> bool {
        self.on_extras() && self.extras_menu_openness().is_some()
    }

    /// How far up the sheet stands, eased both ways from the start of its move; None once it
    /// has slid away.
    fn extras_menu_openness(&self) -> Option<f32> {
        let (_, opened, closing) = self.extras_menu.open?;
        let now = self.now();
        match closing {
            Some(at) => {
                let t = now.saturating_sub(at) as f32 / MENU_MS;
                (t < 1.0).then(|| 1.0 - ease_out(t))
            }
            None => Some(ease_out(
                (now.saturating_sub(opened) as f32 / MENU_MS).min(1.0),
            )),
        }
    }

    pub(super) fn extras_menu_input(&mut self, action: Action) {
        let Some((item, _, None)) = self.extras_menu.open else {
            return;
        };
        match action {
            Action::GbaDown(Btn::Left) => self.change_extras_system(item, -1),
            Action::GbaDown(Btn::Right) => self.change_extras_system(item, 1),
            Action::GbaDown(Btn::B) | Action::GbaDown(Btn::Start) => {
                let now = self.now();
                if let Some(open) = &mut self.extras_menu.open {
                    open.2 = Some(now);
                }
            }
            _ => {}
        }
    }

    /// Steps the item's system through All, GBA, GB, GBC, NES and SNES and writes it to the
    /// card; the sheet's count and the blister pack's are drawn again for the new pool.
    fn change_extras_system(&mut self, item: slot_ui::Extra, by: i32) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let key = Self::extra_key(item);
        let now = slot_store::extras_system(&root, key);
        let at = SYSTEMS.iter().position(|s| *s == now).unwrap_or(0);
        let to = SYSTEMS[(at as i32 + by).rem_euclid(SYSTEMS.len() as i32) as usize];
        if let Err(e) = slot_store::write_extras_system(&root, key, to) {
            eprintln!("slot: extras: {key}: system: {e}");
            return self.refuse();
        }
        self.extras_menu.stale = true;
        self.extras_stale = true;
    }

    /// Builds the sheet again when it is stale, and its loop and legend the first time it opens,
    /// as `cart_faces` does for the folder menu.
    pub fn sync_extras_menu(&mut self, compositor: &mut Compositor) {
        let Some((item, _, _)) = self.extras_menu.open else {
            return;
        };
        if self.extras_menu.ring.is_none() {
            let ring = slot_ui::extras_loop_face();
            self.extras_menu.ring = Some(compositor.create_texture(ring.w, ring.h, &ring.rgba));
            self.extras_menu.legend = slot_ui::folder_button_legends()
                .into_iter()
                .map(|f| (compositor.create_texture(f.w, f.h, &f.rgba), f.w))
                .collect();
        }
        if !std::mem::take(&mut self.extras_menu.stale) {
            return;
        }
        let title = match item {
            slot_ui::Extra::Unplayed => "Never played",
            _ => "Get lucky",
        };
        let system = self
            .root
            .as_ref()
            .and_then(|root| slot_store::extras_system(root, Self::extra_key(item)));
        let system = system.map_or("All", |p| p.dir_name());
        let pool = self.extra_pool(item).len();
        let Some(face) = slot_ui::extras_menu_face(title, system, pool) else {
            return;
        };
        let tex = compositor.create_texture(face.w, face.h, &face.rgba);
        if let Some(old) = self.extras_menu.sheet.replace(tex) {
            self.orphaned.push(old);
        }
    }

    /// The Extras row with the neighbours stepped aside, the sheet and its loop over the item,
    /// and B Back and ◀ ▶ Change where the slot's name was.
    pub(super) fn draw_extras_menu(&self, out: &mut Vec<Draw>) {
        let Some(open) = self.extras_menu_openness() else {
            return;
        };
        let canvas = self.canvas();
        let at = self.extra_scroll.round() as i64;
        for pos in [at - 1, at + 1, at] {
            let slot = pos.rem_euclid(3) as usize;
            let item = slot_ui::Extra::ALL[slot];
            let Some(tex) = self.extra_faces[slot] else {
                continue;
            };
            let side = (pos - at) as f32;
            let mut place = slot_ui::extra_box(canvas, item, pos as f32 - self.extra_scroll);
            place.x += side * NEIGHBOUR_SHIFT * open;
            let alpha = 1.0 - side.abs() * open;
            slot_ui::draw_extra(item, place, tex, alpha, out);
            self.draw_extra_parts(item, place, alpha, out);
        }
        draw_empty_slot(canvas, self.slot_faces, out);
        draw_slot_name(
            canvas,
            self.shelf_platform,
            self.slot_name_alpha() * (1.0 - open),
            out,
        );
        if let Some(tex) = self.extras_menu.sheet {
            draw_pose(slot_ui::extras_menu_pose(canvas, open), tex, 1.0, out);
        }
        if let Some(tex) = self.extras_menu.ring {
            draw_pose(slot_ui::extras_loop_pose(canvas, open), tex, 1.0, out);
        }
        self.draw_legend_row(&self.extras_menu.legend, &[true, true, false], open, out);
    }
}
