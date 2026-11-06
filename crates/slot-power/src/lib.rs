macro_rules! eprintln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

mod cpu;
mod device;
mod mixer;
mod motor;
mod power;
mod sim;

pub use cpu::{cpu_step, CpuCap};
pub use device::{
    card_kernel_lines, card_mount_in, has_bit, motor_change, read_only, read_only_in,
    record_first_frame, rumble_node, stage_repair, status_led, take_repair_result,
    trace_first_frame, uptime_seconds, CardMount, DevicePlatform,
};
pub use mixer::{
    assist_volume, control_level, find_control, parse_cget, volume_db, ControlRange, Mixer,
    HEADPHONE_CONTROLS, SPEAKER_CONTROLS, VOLUME_FLOOR_DB,
};
pub use motor::Motor;
pub use power::Power;
pub use sim::SimPlatform;

use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Charge {
    Unknown,
    Discharging,
    Charging,
    Full,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Battery {
    pub percent: u8,
    pub charge: Charge,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RepairResult {
    Repaired,
    Clean,
    Failed,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum LedState {
    Off,
    Running,
    Low,
    Charging,
    Charged,
}

pub trait Platform: Send {
    fn set_backlight(&mut self, step: u8);
    fn battery(&self) -> Option<Battery>;
    fn charge(&self) -> Charge;
    fn set_led(&mut self, state: LedState);
    fn poweroff(&mut self) -> !;
    fn restart(&mut self) -> !;
    fn root(&self) -> &Path;
    fn now(&self) -> i64;
    fn set_clock(&mut self, secs: i64);
    fn set_rumble(&mut self, strength: u16);

    fn relink_adb(&mut self) -> bool {
        false
    }

    fn headphones(&self) -> bool {
        false
    }

    /// A computer has set up the device's USB connection (adb, a copy over the cable); a plain
    /// charger never does.
    fn on_a_computer(&self) -> bool {
        false
    }

    /// Sends the sound to the headphones or the speaker where the codec does not on its own.
    fn route_audio(&mut self, _headphones: bool) {}

    /// Sets the output's volume `0..=100` on the codec itself where it has a control for it, 0
    /// switching the amplifier off. Returns at once; the last of quick changes wins.
    fn set_volume(&mut self, _volume: u8, _headphones: bool) {}

    /// What the picture is multiplied by at brightness `step`, for a step darker than the
    /// backlight goes: 1 where the backlight reaches it.
    fn picture_dim(&self, _step: u8) -> f32 {
        1.0
    }

    /// True while the codec carries the volume, so the samples go out at full scale.
    fn hardware_volume(&self) -> bool {
        false
    }

    /// Caps the CPU's top clock at `khz`, or the nearest step under it, where slot may; the
    /// governor still picks the clock under the cap.
    fn cap_cpu(&mut self, _khz: u32) {}

    /// Suspends to RAM until the lid opens or the power key is pressed, or `wake_after` passes.
    /// Returns once awake again; false when this device cannot suspend, without having tried.
    fn suspend(&mut self, _wake_after: Option<Duration>) -> bool {
        false
    }

    fn card_read_only(&self) -> bool {
        false
    }

    fn card_log(&self) -> Option<PathBuf> {
        None
    }

    fn card_kernel_lines(&self) -> Vec<String> {
        Vec::new()
    }

    fn repair_card(&mut self) -> ! {
        self.restart()
    }

    fn take_repair_result(&mut self) -> Option<RepairResult> {
        None
    }
}

pub trait LidPolicy {
    fn on_close(&mut self);
    fn on_open(&mut self);
    fn timeout(&self) -> Duration;
}
