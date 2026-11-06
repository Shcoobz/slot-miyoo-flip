//! Battery savings that follow what slot is doing: the backlight dims when nobody has touched
//! the device for a while, the CPU's top clock comes down while a light core plays, and on the
//! Flip the codec carries the volume, so the samples go out untouched and mute turns the
//! amplifier off.

use slot_store::Core;
use slot_ui::Millis;

use super::{App, Phase};

/// The backlight step a dimmed screen drops to: the lowest that is still lit.
const DIM_STEP: u8 = 1;

/// The CPU caps, in kHz. Starting values, to be tuned by measuring on the device: a light core
/// must still hold full speed under its cap with rewind recording; fast-forward lifts the cap.
pub const CPU_LIGHT_KHZ: u32 = 1_200_000;
pub const CPU_FULL_KHZ: u32 = 1_800_000;

/// What the savings last asked of the device, so each is sent only when it changes.
#[derive(Default)]
pub(super) struct Saving {
    dimmed: bool,
    cpu_khz: Option<u32>,
    volume: Option<(u8, bool)>,
    /// A game button is held down: play, though it sends no new press.
    buttons_held: bool,
}

/// The top clock `core` needs to hold full speed: Gambatte, the NES cores, Snes9x 2005 and gpSP
/// (a dynarec, far under a frame's time on the Flip) are light; mGBA and Snes9x get the whole CPU.
pub fn cpu_cap(core: Core) -> u32 {
    match core {
        Core::Gambatte | Core::Fceumm | Core::Nestopia | Core::Snes9x2005 | Core::Gpsp => {
            CPU_LIGHT_KHZ
        }
        Core::Mgba | Core::Snes9x => CPU_FULL_KHZ,
    }
}

impl App {
    pub(super) fn sync_power_saving(&mut self) {
        self.sync_dim();
        self.sync_cpu_cap();
        self.sync_codec_volume();
    }

    /// Whether game buttons are held down now; a long hold keeps the screen lit.
    pub fn set_buttons_held(&mut self, held: bool) {
        self.saving.buttons_held = held;
    }

    /// The screen is dimmed for want of input.
    pub fn dimmed(&self) -> bool {
        self.saving.dimmed
    }

    /// The level the samples are scaled to: what the codec cannot take off itself while it
    /// carries the volume (`slot_power::assist_volume`, full scale from 40 up), and silence when
    /// muted either way, so a mute lands before the amplifier goes off.
    pub fn sample_volume(&self) -> u8 {
        match self.output_volume() {
            0 => 0,
            v if self.power.as_ref().is_some_and(|p| p.hardware_volume()) => {
                slot_power::assist_volume(v)
            }
            v => v,
        }
    }

    /// Dim Screen: idle long enough on the shelf or in a game, with no button held and no
    /// fast-forward running. It comes before Auto Sleep, whose doze lights the screen back up
    /// for the lid to restore.
    fn dim_due(&self) -> bool {
        let secs = self.state.dim_screen_s;
        secs > 0
            && !self.saving.buttons_held
            && self.ff_badge().is_none()
            && !self.shutting_down()
            && matches!(self.phase, Phase::Shelf | Phase::Playing { .. })
            && self.now().saturating_sub(self.pressed_at) >= Millis::from(secs) * 1000
    }

    /// Lowers the backlight while dimming is due and puts the chosen brightness back once it is
    /// not; the brightness setting itself never changes.
    fn sync_dim(&mut self) {
        let due = self.dim_due();
        if due == self.saving.dimmed {
            return;
        }
        self.saving.dimmed = due;
        let step = match due {
            true => DIM_STEP.min(self.state.brightness),
            false => self.state.brightness,
        };
        if let Some(power) = &mut self.power {
            power.set_backlight(step);
        }
    }

    /// The cap for what runs now: the playing core's, the whole CPU on the shelf, in menus
    /// outside a game, and while fast-forwarding.
    fn wanted_cpu_khz(&self) -> u32 {
        match self.phase {
            Phase::Playing { .. } if self.ff_badge().is_none() => cpu_cap(self.core),
            _ => CPU_FULL_KHZ,
        }
    }

    fn sync_cpu_cap(&mut self) {
        let khz = self.wanted_cpu_khz();
        if self.saving.cpu_khz == Some(khz) {
            return;
        }
        let Some(power) = &mut self.power else {
            return;
        };
        power.cap_cpu(khz);
        self.saving.cpu_khz = Some(khz);
    }

    /// Hands the codec each new volume, mute or output; it sets them off this thread.
    fn sync_codec_volume(&mut self) {
        let want = (self.output_volume(), self.headphones);
        if self.saving.volume == Some(want) {
            return;
        }
        let Some(power) = &mut self.power else {
            return;
        };
        power.set_volume(want.0, want.1);
        self.saving.volume = Some(want);
    }
}
