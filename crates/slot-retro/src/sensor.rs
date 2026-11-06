//! A gyroscope steered by a stick, for cartridges such as WarioWare: Twisted! on handhelds
//! that have no gyroscope of their own. The input thread stores the stick, the core reads it.
//! By default the stick sets how fast the console turns, so holding it spins like a crank. As an
//! alternative it sets the angle the console is held at, and the gyroscope reports the turn
//! towards it.

use std::ffi::c_uint;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use crate::ffi::{SENSOR_GYROSCOPE_DISABLE, SENSOR_GYROSCOPE_ENABLE, SENSOR_GYROSCOPE_Z};

/// Stick travel ignored around the centre, which the Flip's sticks rest well away from.
pub const GYRO_DEADZONE: i32 = 6000;
const STICK_MAX: i32 = 32767;
/// The turn rate at full deflection in rate mode, in degrees per second.
pub const FULL_RATE_DPS: f32 = 270.0;
/// The angle a fully deflected stick holds the console at in angle mode, in degrees.
pub const FULL_TILT_DEG: f32 = 30.0;
/// Seconds the console takes to close most of the way to the stick's angle.
pub const FOLLOW_S: f32 = 0.25;

static FULL_RATE: AtomicU32 = AtomicU32::new(0);
static FULL_RATE_RIGHT: AtomicU32 = AtomicU32::new(0);
static CURVE: AtomicU32 = AtomicU32::new(0);
static FULL_TILT: AtomicU32 = AtomicU32::new(0);
static FOLLOW: AtomicU32 = AtomicU32::new(0);
static ANGLE_MODE: AtomicBool = AtomicBool::new(false);

fn tuned(cell: &AtomicU32, default: f32) -> f32 {
    match cell.load(Ordering::Relaxed) {
        0 => default,
        bits => f32::from_bits(bits),
    }
}

/// How the gyroscope answers the stick: in rate mode the full-stick turn rate to the left and to
/// the right (a stick rarely travels evenly both ways) and the response curve, in angle mode the
/// full-stick tilt and the follow time.
pub struct GyroTuning {
    pub angle_mode: bool,
    pub rate_left_dps: f32,
    pub rate_right_dps: f32,
    pub curve: f32,
    pub tilt_deg: f32,
    pub follow_s: f32,
}

pub fn tune_gyro(t: GyroTuning) {
    let (angle_mode, full_tilt_deg, follow_s) = (t.angle_mode, t.tilt_deg, t.follow_s);
    ANGLE_MODE.store(angle_mode, Ordering::Relaxed);
    FULL_RATE.store(
        t.rate_left_dps.clamp(1.0, 2000.0).to_bits(),
        Ordering::Relaxed,
    );
    FULL_RATE_RIGHT.store(
        t.rate_right_dps.clamp(1.0, 2000.0).to_bits(),
        Ordering::Relaxed,
    );
    CURVE.store(t.curve.clamp(1.0, 4.0).to_bits(), Ordering::Relaxed);
    FULL_TILT.store(full_tilt_deg.clamp(1.0, 90.0).to_bits(), Ordering::Relaxed);
    FOLLOW.store(follow_s.clamp(0.01, 2.0).to_bits(), Ordering::Relaxed);
}

fn stick_fraction(stick: i32) -> f32 {
    let travel = stick.abs().min(STICK_MAX);
    if travel <= GYRO_DEADZONE {
        return 0.0;
    }
    let past = (travel - GYRO_DEADZONE) as f32 / (STICK_MAX - GYRO_DEADZONE) as f32;
    past * stick.signum() as f32
}

/// The turn rate, in radians per second, for this stick deflection in rate mode. The curve keeps
/// the first of the stick's travel fine: at 2 half the travel gives a quarter of the rate.
pub fn stick_rate(stick: i32) -> f32 {
    let f = stick_fraction(stick);
    let full = match f > 0.0 {
        true => tuned(&FULL_RATE_RIGHT, tuned(&FULL_RATE, FULL_RATE_DPS)),
        false => tuned(&FULL_RATE, FULL_RATE_DPS),
    };
    f.signum() * f.abs().powf(tuned(&CURVE, 1.0)) * full.to_radians()
}

static STICK: AtomicI32 = AtomicI32::new(0);
static FITTED: AtomicBool = AtomicBool::new(false);

/// Reports a gyroscope from now on and turns it by this stick deflection (+-32767).
pub fn steer_gyro(stick: i32) {
    FITTED.store(true, Ordering::Relaxed);
    STICK.store(stick, Ordering::Relaxed);
}

/// The angle, in radians, the console is held at for this stick deflection.
pub fn stick_tilt(stick: i32) -> f32 {
    stick_fraction(stick) * tuned(&FULL_TILT, FULL_TILT_DEG).to_radians()
}

struct Held {
    angle: f32,
    at: Option<Instant>,
}

static HELD: Mutex<Held> = Mutex::new(Held {
    angle: 0.0,
    at: None,
});

/// The turn rate, in radians per second, that carries the console towards the stick's angle.
pub fn turn_towards(angle: &mut f32, target: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    let rate = (target - *angle) / tuned(&FOLLOW, FOLLOW_S).max(dt);
    *angle += rate * dt;
    rate
}

fn gyro_rate() -> f32 {
    let stick = STICK.load(Ordering::Relaxed);
    if !ANGLE_MODE.load(Ordering::Relaxed) {
        return stick_rate(stick);
    }
    let target = stick_tilt(stick);
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    let dt = held.at.map_or(0.0, |t| (now - t).as_secs_f32().min(0.1));
    held.at = Some(now);
    let mut angle = held.angle;
    let rate = turn_towards(&mut angle, target, dt);
    held.angle = angle;
    rate
}

pub(crate) unsafe extern "C" fn set_sensor_state(
    port: c_uint,
    action: c_uint,
    _rate: c_uint,
) -> bool {
    port == 0
        && FITTED.load(Ordering::Relaxed)
        && matches!(action, SENSOR_GYROSCOPE_ENABLE | SENSOR_GYROSCOPE_DISABLE)
}

pub(crate) unsafe extern "C" fn get_sensor_input(port: c_uint, id: c_uint) -> f32 {
    match (port, id) {
        (0, SENSOR_GYROSCOPE_Z) => gyro_rate(),
        _ => 0.0,
    }
}
