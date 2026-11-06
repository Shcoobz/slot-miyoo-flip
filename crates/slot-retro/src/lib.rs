macro_rules! eprintln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

mod core;
mod ffi;
mod libretro;
mod link;
mod mock;
mod rumble;
mod sensor;

pub use core::{AvInfo, ButtonMask, CoreError, RetroCore, GBA_H, GBA_W, MAX_FRAME_H, MAX_FRAME_W};
pub use ffi::{
    NETPACKET_BROADCAST, NETPACKET_FLUSH_HINT, NETPACKET_RELIABLE, NETPACKET_UNRELIABLE,
    NETPACKET_UNSEQUENCED,
};
pub use libretro::LibretroCore;
pub use link::{Link, LinkChannel, LoopbackLink};
pub use mock::MockCore;
pub use rumble::Rumble;
pub use sensor::{
    steer_gyro, stick_rate, stick_tilt, tune_gyro, turn_towards, GyroTuning, FOLLOW_S,
    FULL_RATE_DPS, FULL_TILT_DEG, GYRO_DEADZONE,
};
