//! The PLAYED plate across the foot of an open cartridge: how long its game has been played and
//! when it last was, read from the activity log each time a cartridge opens.

use slot_store::{Platform, Recent};
use slot_ui::{Draw, Placed, TexId};

use super::{system_secs, App, MONTHS};

/// The plate for the game the cartridge was last opened on: its words until the face is built,
/// then the face's texture and size.
#[derive(Default)]
pub struct PlayedPlate {
    want: Option<(Platform, String, String)>,
    face: Option<(TexId, u32, u32)>,
}

impl App {
    /// As a cartridge opens, its game's plate is asked for; a game never played has none, so the
    /// last game's plate goes either way.
    pub(super) fn ask_played_plate(&mut self) {
        if let Some((old, _, _)) = self.played_plate.face.take() {
            self.orphaned.push(old);
        }
        self.played_plate.want = self.played_words();
    }

    /// The selected game's platform, time played and when it was last played.
    fn played_words(&self) -> Option<(Platform, String, String)> {
        let root = self.root.as_ref()?;
        let cart = self.shelf().carts.get(self.shelf().index)?;
        let game = Recent {
            platform: cart.platform,
            stem: cart.stem.clone(),
        };
        let offset = i32::from(self.state.utc_offset_min);
        // Under a minute reads "0 M": a game only tried for a moment wears no plate.
        let (secs, last) = slot_store::played(&slot_store::read_activity(root), &game, offset)
            .filter(|(secs, _)| *secs >= 60)?;
        let now = self.power.as_ref().map_or_else(system_secs, |p| p.now());
        let today = slot_store::local_day(now, offset);
        Some((
            cart.platform,
            slot_store::play_time_text(secs),
            when_text(today, last),
        ))
    }

    /// The plate's words, once, for its face to be built.
    pub fn played_plate_want(&mut self) -> Option<(Platform, String, String)> {
        self.played_plate.want.take()
    }

    pub fn set_played_plate(&mut self, tex: TexId, w: u32, h: u32) {
        if let Some((old, _, _)) = self.played_plate.face.replace((tex, w, h)) {
            self.orphaned.push(old);
        }
    }

    /// The plate on the open cartridge's board, fading in as the board lifts.
    pub(super) fn draw_played_plate(
        &self,
        platform: Platform,
        board: Placed,
        lift: f32,
        out: &mut Vec<Draw>,
    ) {
        if let Some(face) = self.played_plate.face {
            slot_ui::draw_played(platform, board, face, lift, out);
        }
    }
}

/// When a game was last played, as the plate says it: the last week by name, older days by date.
fn when_text(today: i64, last: i64) -> String {
    match today - last {
        i64::MIN..=0 => "today".to_string(),
        1 => "yesterday".to_string(),
        n @ 2..=6 => format!("{n} days ago"),
        7..=13 => "last week".to_string(),
        _ => {
            let (_, month, day) = slot_store::civil(last);
            format!("{} {day}", MONTHS[month as usize - 1].to_lowercase())
        }
    }
}
