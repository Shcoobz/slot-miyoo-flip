#[cfg(unix)]
use std::ffi::{c_int, c_ulong, c_void};
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::{Battery, Charge, CpuCap, LedState, Mixer, Platform, RepairResult};

const TOP_STEP: u32 = 9;

/// The backlight's lowest lit step and the bend of the steps above it, as a share of its range.
#[derive(Copy, Clone)]
struct BacklightCurve {
    floor: f64,
    power: f64,
}

const BACKLIGHT_CURVE: BacklightCurve = BacklightCurve {
    floor: 8.0 / 255.0,
    power: 1.75,
};

/// The Miyoo Flip's kernel maps brightness through its own table: 1 already drives the panel at
/// 20/255, and 255 at 255 on a straight line between. That floor is far from dark, so the Flip's
/// steps are light levels instead, even to the eye from `FLIP_DARKEST` of full to full; the ones
/// under the floor keep the backlight at 1 and darken the picture the rest of the way.
const FLIP_KERNEL_FLOOR: f64 = 20.0 / 255.0;
const FLIP_DARKEST: f64 = 0.005;

/// How much light a Flip brightness step gives, as a share of full: 0 off, then 0.5 %, 1 %, 1.9 %,
/// 3.6 %, 7.1 %, 13.7 %, 26.6 %, 51.6 % and 100 %.
fn flip_light(step: u8) -> f64 {
    let step = u32::from(step).min(TOP_STEP);
    if step == 0 {
        return 0.0;
    }
    let t = f64::from(step - 1) / f64::from(TOP_STEP - 1);
    FLIP_DARKEST * (1.0 / FLIP_DARKEST).powf(t)
}

/// The Flip's backlight value for a step: 1 for every step at or under the kernel's floor.
fn flip_backlight_value(step: u8, max: u32) -> u32 {
    let light = flip_light(step);
    if light <= 0.0 {
        return 0;
    }
    let over = ((light - FLIP_KERNEL_FLOOR) / (1.0 - FLIP_KERNEL_FLOOR)).max(0.0);
    (1.0 + over * f64::from(max.max(1) - 1)).round() as u32
}

/// What the picture is multiplied by for a Flip step the backlight cannot reach.
pub fn flip_picture_dim(step: u8) -> f32 {
    let light = flip_light(step);
    match light > 0.0 && light < FLIP_KERNEL_FLOOR {
        true => (light / FLIP_KERNEL_FLOOR) as f32,
        false => 1.0,
    }
}

const DEV_INPUT: &str = "/dev/input";

const RUN_DIR: &str = "/run";

const CARD_LOG: &str = "/data/slot-card.log";

const REPAIR_SCRIPT: &str = "repair.sh";

const FSCK_EXFAT: &str = "fsck.exfat";

const REPAIR_STAGE: &str = "/tmp/slot-repair";

const REPAIR_RESULT: &str = "/data/slot-repair.result";

const WIFI_MODULE: &str = "8821cs";

const WIFI_UNLOAD_WAIT: Duration = Duration::from_secs(5);

const KERNEL_LINES: usize = 40;

pub fn uptime_seconds(s: &str) -> Option<String> {
    let first = s.split_whitespace().next()?;
    if !first.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    if !first.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(first.to_string())
}

pub fn record_first_frame(run: &Path, uptime: &str) {
    let marker = run.join("boot-first-frame");
    let again = marker.exists();
    if !again {
        let _ = fs::write(&marker, format!("{uptime}\n"));
    }
    let label = if again {
        "first-frame-again"
    } else {
        "first-frame"
    };
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(run.join("boot-trace"))
    {
        let _ = writeln!(f, "{uptime} {label}");
    }
}

pub fn trace_first_frame() {
    let Ok(raw) = fs::read_to_string("/proc/uptime") else {
        return;
    };
    let Some(up) = uptime_seconds(&raw) else {
        return;
    };
    record_first_frame(Path::new(RUN_DIR), &up);
}

const EV_FF: u16 = 0x15;
const FF_RUMBLE: u16 = 0x50;
#[cfg(unix)]
const EVIOCSFF: c_ulong = 0x4030_4580;

const DISPDBG_MAX: u32 = 255;

enum Backlight {
    Class(PathBuf),
    Dispdbg(PathBuf),
}

enum Led {
    Multi(PathBuf),
    Mono { dir: PathBuf, max: u32 },
}

pub struct DevicePlatform {
    root: PathBuf,
    sysfs: PathBuf,
    backlight: Option<Backlight>,
    max_brightness: u32,
    battery: Option<PathBuf>,
    rtc: Option<PathBuf>,
    motor: Option<Motor>,
    jack: Option<PathBuf>,
    curve: BacklightCurve,
    /// A Miyoo Flip: its brightness steps are light levels (`flip_light`), not a curve.
    flip: bool,
    led: Option<Led>,
    /// The Flip's codec volume and playback path, set off the main thread.
    mixer: Option<Mixer>,
    /// The Flip's top CPU clock, capped for the light cores.
    cpu: Option<CpuCap>,
}

impl DevicePlatform {
    pub fn new(root: PathBuf) -> Self {
        DevicePlatform::probe(Path::new("/sys"), root)
    }

    pub fn probe(sysfs: &Path, root: PathBuf) -> Self {
        let class = first_dir(&sysfs.join("class/backlight"), |d| {
            d.join("brightness").is_file()
        });
        let max_brightness = class
            .as_ref()
            .and_then(|d| read_number(&d.join("max_brightness")))
            .unwrap_or(DISPDBG_MAX);
        let backlight = match class {
            Some(dir) => Some(Backlight::Class(dir)),
            None => {
                let dbg = sysfs.join("kernel/debug/dispdbg");
                dbg.join("param")
                    .is_file()
                    .then_some(Backlight::Dispdbg(dbg))
            }
        };
        let battery = first_dir(&sysfs.join("class/power_supply"), is_battery);
        let rtc = first_dir(&sysfs.join("class/rtc"), |_| true);
        let motor = Motor::open(sysfs);
        let jack = flip_jack(sysfs);
        let flip = baseos_target().as_deref() == Some("my355");
        let curve = BACKLIGHT_CURVE;
        if flip {
            wifi_off(sysfs);
        }
        let mixer = jack.is_some().then(Mixer::spawn);
        let cpu = flip
            .then(|| CpuCap::at(&sysfs.join("devices/system/cpu/cpufreq/policy0")))
            .flatten();
        let led = status_led(&sysfs.join("class/leds")).map(|dir| {
            if dir.join("multi_intensity").is_file() {
                Led::Multi(dir)
            } else {
                let max = read_number(&dir.join("max_brightness")).unwrap_or(255);
                Led::Mono { dir, max }
            }
        });
        DevicePlatform {
            root,
            sysfs: sysfs.to_path_buf(),
            backlight,
            max_brightness,
            battery,
            rtc,
            motor,
            jack,
            curve,
            flip,
            led,
            mixer,
            cpu,
        }
    }

    fn breadcrumb(&self, line: &str) {
        use std::io::Write;
        let dir = self.root.join("Logs");
        let _ = fs::create_dir_all(&dir);
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("ags-shutdown-trace.log"))
        {
            let _ = writeln!(f, "{} {line}", self.now());
            let _ = f.sync_all();
        }
    }

    pub fn trace_boot(&self) {
        self.breadcrumb("boot: platform up, trace working");
    }

    pub fn report(&self) -> String {
        let leaf = |p: &Path| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string())
        };
        let name = |p: &Option<PathBuf>, absent: &str| match p {
            Some(p) => leaf(p),
            None => absent.to_string(),
        };
        let backlight = match &self.backlight {
            Some(Backlight::Class(dir)) => leaf(dir),
            Some(Backlight::Dispdbg(_)) => "dispdbg".to_string(),
            None => "no backlight".to_string(),
        };
        let motor = match &self.motor {
            Some(m) => format!("motor {}", m.name),
            None => "no motor".to_string(),
        };
        format!(
            "backlight {} (0 to {}), battery {}, {}, {motor}",
            backlight,
            self.max_brightness,
            name(&self.battery, "no battery"),
            name(&self.rtc, "no rtc"),
        )
    }
}

fn ran(what: &str, result: std::io::Result<std::process::ExitStatus>) {
    match result {
        Ok(status) if status.success() => eprintln!("slot: {what} ok"),
        Ok(status) => eprintln!("slot: {what} {status}"),
        Err(e) => eprintln!("slot: {what}: {e}"),
    }
}

pub fn has_bit(mask: &str, bit: u16) -> bool {
    let words: Vec<&str> = mask.split_whitespace().collect();
    let from_end = usize::from(bit) / 64;
    let Some(word) = words.len().checked_sub(from_end + 1).map(|i| words[i]) else {
        return false;
    };
    u64::from_str_radix(word, 16).is_ok_and(|w| w >> (bit % 64) & 1 == 1)
}

/// The LED that shows slot's state: one named `work` (the Miyoo Flip's power LED) if there is
/// one, else the first that is neither the charge LED the kernel drives nor an SD activity LED.
pub fn status_led(leds: &Path) -> Option<PathBuf> {
    let usable = |d: &Path| d.join("multi_intensity").is_file() || d.join("brightness").is_file();
    let work = leds.join("work");
    if usable(&work) {
        return Some(work);
    }
    first_dir(leds, |d| {
        let name = d
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        usable(d) && !name.starts_with("charger") && !name.starts_with("mmc")
    })
}

pub fn rumble_node(sysfs: &Path) -> Option<String> {
    let dir = first_dir(&sysfs.join("class/input"), |node| {
        fs::read_to_string(node.join("device/capabilities/ff"))
            .is_ok_and(|ff| has_bit(&ff, FF_RUMBLE))
    })?;
    Some(dir.file_name()?.to_string_lossy().into_owned())
}

pub fn motor_change(strength: u16, running: bool) -> Option<bool> {
    match (strength > 0, running) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

struct Motor {
    drive: Drive,
    name: String,
    running: bool,
}

enum Drive {
    /// A force-feedback input node, as on the Anbernic handhelds.
    Ff { node: fs::File, id: i16 },
    /// A motor switched by a GPIO's value file.
    Gpio(PathBuf),
}

/// The Miyoo Flip's headphone jack: GPIO 150 reads 0 with a plug in. The codec does not
/// follow it, so slot switches its playback path itself.
const FLIP_JACK_GPIO: u32 = 150;

/// The RTC alarm that ends a deep sleep after the lid's power-off timeout.
const WAKE_ALARM: &str = "/sys/class/rtc/rtc0/wakealarm";

fn baseos_target() -> Option<String> {
    let release = fs::read_to_string(BASEOS_RELEASE).ok()?;
    let target = release
        .lines()
        .find_map(|l| l.strip_prefix("BASEOS_TARGET="))?;
    Some(target.trim_matches('"').to_string())
}

fn flip_jack(sysfs: &Path) -> Option<PathBuf> {
    if baseos_target()? != "my355" {
        return None;
    }
    let gpio = sysfs.join("class/gpio");
    let dir = gpio.join(format!("gpio{FLIP_JACK_GPIO}"));
    if !dir.is_dir() {
        let _ = fs::write(gpio.join("export"), FLIP_JACK_GPIO.to_string());
    }
    let _ = fs::write(dir.join("direction"), "in");
    let value = dir.join("value");
    value.is_file().then_some(value)
}

/// BaseOS targets whose motor hangs off a plain GPIO rather than an input node.
const GPIO_MOTORS: [(&str, u32); 1] = [("my355", 20)];

const BASEOS_RELEASE: &str = "/etc/baseos-release";

#[cfg(unix)]
#[repr(C)]
#[derive(Default)]
struct FfTrigger {
    button: u16,
    interval: u16,
}

#[cfg(unix)]
#[repr(C)]
#[derive(Default)]
struct FfReplay {
    length: u16,
    delay: u16,
}

#[cfg(unix)]
#[repr(C)]
#[derive(Default)]
struct FfRumble {
    strong: u16,
    weak: u16,
}

#[cfg(unix)]
#[repr(C)]
struct FfEffect {
    kind: u16,
    id: i16,
    direction: u16,
    trigger: FfTrigger,
    replay: FfReplay,
    _align: u16,
    rumble: FfRumble,
    _tail: [u8; 28],
}

#[repr(C)]
struct FfEvent {
    sec: i64,
    usec: i64,
    kind: u16,
    code: u16,
    value: i32,
}

#[cfg(unix)]
extern "C" {
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
    fn dup2(old: c_int, new: c_int) -> c_int;
}

const F_GETFD: c_int = 1;
const F_SETFD: c_int = 2;
const FD_CLOEXEC: c_int = 1;

fn unload_wifi() {
    let Ok(mut rmmod) = Command::new("rmmod").arg(WIFI_MODULE).spawn() else {
        return;
    };
    let until = Instant::now() + WIFI_UNLOAD_WAIT;
    while Instant::now() < until {
        if !matches!(rmmod.try_wait(), Ok(None)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn release_logs() {
    #[cfg(unix)]
    if let Ok(null) = fs::OpenOptions::new().write(true).open("/dev/null") {
        unsafe {
            dup2(null.as_raw_fd(), 1);
            dup2(null.as_raw_fd(), 2);
        }
    }
}

fn close_on_exec() {
    let Ok(fds) = fs::read_dir("/proc/self/fd") else {
        return;
    };
    let fds: Vec<c_int> = fds
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
        .filter(|fd| *fd > 2)
        .collect();
    for fd in fds {
        unsafe {
            let flags = fcntl(fd, F_GETFD);
            if flags >= 0 {
                fcntl(fd, F_SETFD, flags | FD_CLOEXEC);
            }
        }
    }
}

fn upload(node: &fs::File, id: i16) -> Option<i16> {
    #[cfg(not(unix))]
    {
        let _ = (node, id);
        None
    }
    #[cfg(unix)]
    {
        let mut effect = FfEffect {
            kind: FF_RUMBLE,
            id,
            direction: 0,
            trigger: FfTrigger::default(),
            replay: FfReplay::default(),
            _align: 0,
            rumble: FfRumble {
                strong: u16::MAX,
                weak: u16::MAX,
            },
            _tail: [0; 28],
        };
        let rc = unsafe {
            ioctl(
                node.as_raw_fd(),
                EVIOCSFF,
                &mut effect as *mut FfEffect as *mut c_void,
            )
        };
        (rc >= 0).then_some(effect.id)
    }
}

impl Motor {
    fn open(sysfs: &Path) -> Option<Motor> {
        Motor::open_ff(sysfs).or_else(|| Motor::open_gpio(sysfs))
    }

    fn open_gpio(sysfs: &Path) -> Option<Motor> {
        let release = fs::read_to_string(BASEOS_RELEASE).ok()?;
        let target = release
            .lines()
            .find_map(|l| l.strip_prefix("BASEOS_TARGET="))?
            .trim_matches('"');
        let (_, pin) = GPIO_MOTORS.iter().find(|(t, _)| *t == target)?;
        let gpio = sysfs.join("class/gpio");
        let dir = gpio.join(format!("gpio{pin}"));
        if !dir.is_dir() {
            let _ = fs::write(gpio.join("export"), pin.to_string());
        }
        let name = format!("gpio{pin}");
        fs::write(dir.join("direction"), "out")
            .map_err(|e| eprintln!("slot: rumble {name}: {e}"))
            .ok()?;
        Some(Motor {
            drive: Drive::Gpio(dir.join("value")),
            name,
            running: false,
        })
    }

    fn open_ff(sysfs: &Path) -> Option<Motor> {
        let name = rumble_node(sysfs)?;
        let node = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(Path::new(DEV_INPUT).join(&name))
            .map_err(|e| eprintln!("slot: rumble {name}: {e}"))
            .ok()?;
        let Some(id) = upload(&node, -1) else {
            eprintln!("slot: rumble {name}: {}", std::io::Error::last_os_error());
            return None;
        };
        Some(Motor {
            drive: Drive::Ff { node, id },
            name,
            running: false,
        })
    }

    fn play(&mut self, on: bool) {
        let played = match &mut self.drive {
            Drive::Gpio(value) => fs::write(value, if on { "1" } else { "0" }),
            Drive::Ff { node, id } => {
                if on {
                    match upload(node, *id).or_else(|| upload(node, -1)) {
                        Some(new) => *id = new,
                        None => eprintln!(
                            "slot: rumble {}: {}",
                            self.name,
                            std::io::Error::last_os_error()
                        ),
                    }
                }
                let ev = FfEvent {
                    sec: 0,
                    usec: 0,
                    kind: EV_FF,
                    code: *id as u16,
                    value: i32::from(on),
                };
                let bytes = unsafe {
                    std::slice::from_raw_parts(
                        &ev as *const FfEvent as *const u8,
                        size_of::<FfEvent>(),
                    )
                };
                (&*node).write_all(bytes)
            }
        };
        match played {
            Ok(()) => self.running = on,
            Err(e) => eprintln!("slot: rumble: {e}"),
        }
    }
}

impl Drop for Motor {
    fn drop(&mut self) {
        if self.running {
            self.play(false);
        }
    }
}

fn first_dir(parent: &Path, keep: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let mut names: Vec<PathBuf> = fs::read_dir(parent)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    names.sort();
    names.into_iter().find(|p| keep(p))
}

fn is_battery(dir: &Path) -> bool {
    fs::read_to_string(dir.join("type")).is_ok_and(|t| t.trim() == "Battery")
        && dir.join("capacity").is_file()
}

fn read_number(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn charge_at(dir: &Path) -> Charge {
    let Ok(text) = fs::read_to_string(dir.join("status")) else {
        return Charge::Unknown;
    };
    match text.trim() {
        "Charging" => Charge::Charging,
        "Discharging" => Charge::Discharging,
        "Full" => Charge::Full,
        _ => Charge::Unknown,
    }
}

fn headphones_in(gpio: &str) -> bool {
    gpio.lines()
        .find(|l| l.contains("|Headphone detection"))
        .and_then(|l| l.rsplit_once(')'))
        .is_some_and(|(_, state)| state.split_whitespace().nth(1) == Some("hi"))
}

/// Powers the Flip's Wi-Fi and Bluetooth module down: slot uses neither, and awake on its USB bus
/// it draws about 100 mA (measured on the Flip, 2026-10-24).
fn wifi_off(sysfs: &Path) {
    let power = sysfs.join("class/rkwifi/wifi_power");
    if !power.is_file() {
        return;
    }
    match fs::write(&power, "0") {
        Ok(()) => eprintln!("slot: wifi: module powered off"),
        Err(e) => eprintln!("slot: wifi: {}: {e}", power.display()),
    }
}

fn backlight_value(step: u8, max: u32, curve: BacklightCurve) -> u32 {
    let step = u32::from(step).min(TOP_STEP);
    if step == 0 {
        return 0;
    }
    let t = f64::from(step - 1) / f64::from(TOP_STEP - 1);
    let floor = (f64::from(max) * curve.floor).round().max(1.0);
    let span = f64::from(max) - floor;
    (floor + span * t.powf(curve.power)).round() as u32
}

impl Platform for DevicePlatform {
    fn on_a_computer(&self) -> bool {
        let Ok(dir) = fs::read_dir(self.sysfs.join("class/udc")) else {
            return false;
        };
        dir.flatten().any(|e| {
            fs::read_to_string(e.path().join("state")).is_ok_and(|s| s.trim() == "configured")
        })
    }

    fn picture_dim(&self, step: u8) -> f32 {
        match self.flip {
            true => flip_picture_dim(step),
            false => 1.0,
        }
    }

    fn headphones(&self) -> bool {
        if let Some(jack) = &self.jack {
            return fs::read_to_string(jack).is_ok_and(|v| v.trim() == "0");
        }
        fs::read_to_string(self.sysfs.join("kernel/debug/gpio")).is_ok_and(|g| headphones_in(&g))
    }

    fn route_audio(&mut self, headphones: bool) {
        if let Some(mixer) = &mut self.mixer {
            mixer.route(headphones);
        }
    }

    fn set_volume(&mut self, volume: u8, headphones: bool) {
        if let Some(mixer) = &mut self.mixer {
            mixer.set_volume(volume, headphones);
        }
    }

    fn hardware_volume(&self) -> bool {
        self.mixer.as_ref().is_some_and(Mixer::active)
    }

    fn cap_cpu(&mut self, khz: u32) {
        if let Some(cpu) = &mut self.cpu {
            cpu.cap(khz);
        }
    }

    /// The Flip's deep sleep, as spruce and NextUI use it on this kernel: an RTC alarm for the
    /// power-off timeout, `deep` as the mem sleep, then `mem` into `/sys/power/state`, which
    /// returns once the lid or the power key woke the unit.
    fn suspend(&mut self, wake_after: Option<Duration>) -> bool {
        if baseos_target().as_deref() != Some("my355") {
            return false;
        }
        let power = Path::new("/sys/power");
        let alarm = Path::new(WAKE_ALARM);
        let _ = fs::write(alarm, "0");
        if let Some(after) = wake_after {
            if let Err(e) = fs::write(alarm, format!("+{}", after.as_secs().max(1))) {
                eprintln!("slot: suspend: wake alarm: {e}");
            }
        }
        if let Err(e) = fs::write(power.join("mem_sleep"), "deep") {
            eprintln!("slot: suspend: mem_sleep: {e}");
        }
        eprintln!("slot: suspend: going to sleep");
        let slept = fs::write(power.join("state"), "mem");
        let _ = fs::write(alarm, "0");
        match slept {
            Ok(()) => {
                eprintln!("slot: suspend: awake");
                true
            }
            Err(e) => {
                eprintln!("slot: suspend: {e}");
                false
            }
        }
    }

    fn card_read_only(&self) -> bool {
        read_only(&self.root)
    }

    fn card_log(&self) -> Option<PathBuf> {
        Some(PathBuf::from(CARD_LOG))
    }

    fn card_kernel_lines(&self) -> Vec<String> {
        Command::new("dmesg")
            .output()
            .map(|o| card_kernel_lines(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default()
    }

    fn set_backlight(&mut self, step: u8) {
        let Some(backlight) = &self.backlight else {
            return;
        };
        let value = match self.flip {
            true => flip_backlight_value(step, self.max_brightness),
            false => backlight_value(step, self.max_brightness, self.curve),
        };
        let write = |file: PathBuf, body: String| {
            if let Err(e) = fs::write(&file, body) {
                eprintln!("slot: {}: {e}", file.display());
            }
        };
        match backlight {
            Backlight::Class(dir) => write(dir.join("brightness"), value.to_string()),
            Backlight::Dispdbg(dir) => {
                write(dir.join("name"), "lcd0".to_string());
                write(dir.join("command"), "setbl".to_string());
                write(dir.join("param"), value.to_string());
                write(dir.join("start"), "1".to_string());
            }
        }
    }

    fn battery(&self) -> Option<Battery> {
        let dir = self.battery.as_ref()?;
        let percent = read_number(&dir.join("capacity"))?;
        Some(Battery {
            percent: percent.min(100) as u8,
            charge: charge_at(dir),
        })
    }

    fn charge(&self) -> Charge {
        self.battery
            .as_ref()
            .map_or(Charge::Unknown, |dir| charge_at(dir))
    }

    fn set_led(&mut self, state: LedState) {
        let Some(led) = &self.led else {
            return;
        };
        match led {
            Led::Multi(dir) => {
                let (r, g, b) = match state {
                    LedState::Off => (0, 0, 0),
                    LedState::Running | LedState::Charged => (0, 255, 0),
                    LedState::Low => (255, 0, 0),
                    LedState::Charging => (255, 140, 0),
                };
                let _ = fs::write(dir.join("multi_intensity"), format!("{r} {g} {b}\n"));
                let _ = fs::write(dir.join("brightness"), "255\n");
            }
            Led::Mono { dir, max } => {
                // A lone LED can still say "low" by blinking where the kernel offers a timer.
                let triggers = fs::read_to_string(dir.join("trigger")).unwrap_or_default();
                let offers = |t: &str| {
                    triggers
                        .split_whitespace()
                        .any(|w| w.trim_matches(['[', ']']) == t)
                };
                if state == LedState::Low && offers("timer") {
                    let _ = fs::write(dir.join("trigger"), "timer\n");
                    let _ = fs::write(dir.join("delay_on"), "250\n");
                    let _ = fs::write(dir.join("delay_off"), "1750\n");
                    return;
                }
                if offers("none") {
                    let _ = fs::write(dir.join("trigger"), "none\n");
                }
                let on = !matches!(state, LedState::Off | LedState::Low);
                let value = if on { *max } else { 0 };
                let _ = fs::write(dir.join("brightness"), format!("{value}\n"));
            }
        }
    }

    fn restart(&mut self) -> ! {
        self.breadcrumb("restart: reached slot, syncing before the WiFi driver unloads");
        let _ = Command::new("sync").status();
        unload_wifi();
        self.breadcrumb("restart: WiFi driver unloaded or given up on, about to sync");
        let _ = Command::new("sync").status();
        self.breadcrumb("restart: sync returned, about to signal init");
        release_logs();
        let _ = Command::new("reboot").status();
        std::thread::sleep(Duration::from_secs(10));
        std::process::exit(0)
    }

    fn repair_card(&mut self) -> ! {
        let mount = fs::read_to_string("/proc/mounts")
            .ok()
            .and_then(|m| card_mount_in(&m, &self.root));
        match (mount, stage_repair(&self.root, Path::new(REPAIR_STAGE))) {
            (Some(m), Ok(script)) => {
                close_on_exec();
                #[cfg(unix)]
                {
                    use std::os::unix::process::CommandExt;
                    let e = Command::new("/bin/sh")
                        .arg(&script)
                        .arg(&m.device)
                        .arg(&m.point)
                        .arg(&m.fs)
                        .exec();
                    eprintln!("slot: repair: {e}");
                }
            }
            (None, _) => eprintln!(
                "slot: repair: no block device under {}",
                self.root.display()
            ),
            (_, Err(e)) => eprintln!("slot: repair: {e}"),
        }
        self.restart()
    }

    fn take_repair_result(&mut self) -> Option<RepairResult> {
        take_repair_result(Path::new(REPAIR_RESULT))
    }

    fn poweroff(&mut self) -> ! {
        self.breadcrumb("poweroff: reached slot, syncing before the WiFi driver unloads");
        let _ = Command::new("sync").status();
        unload_wifi();
        self.breadcrumb("poweroff: WiFi driver unloaded or given up on, about to sync");
        let _ = Command::new("sync").status();
        self.breadcrumb("poweroff: sync returned, about to signal init");
        release_logs();
        let _ = Command::new("poweroff").status();
        std::thread::sleep(Duration::from_secs(10));
        std::process::exit(0)
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn now(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn set_clock(&mut self, secs: i64) {
        ran(
            "date",
            Command::new("date").arg(format!("-s@{secs}")).status(),
        );
        ran(
            "hwclock",
            Command::new("hwclock").args(["-w", "-u"]).status(),
        );
    }

    fn relink_adb(&mut self) -> bool {
        let gadget = self.sysfs.join("kernel/config/usb_gadget/g1/UDC");
        if !gadget.is_file() {
            return false;
        }
        let Some(udc) = first_dir(&self.sysfs.join("class/udc"), |_| true) else {
            return false;
        };
        let Some(name) = udc.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            return false;
        };
        let _ = fs::write(&gadget, "\n");
        match fs::write(&gadget, &name) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("slot: adb {}: {e}", gadget.display());
                false
            }
        }
    }

    fn set_rumble(&mut self, strength: u16) {
        let Some(motor) = &mut self.motor else {
            return;
        };
        if let Some(on) = motor_change(strength, motor.running) {
            motor.play(on);
        }
    }
}

pub fn read_only(path: &Path) -> bool {
    fs::read_to_string("/proc/mounts").is_ok_and(|m| read_only_in(&m, path))
}

pub fn card_kernel_lines(dmesg: &str) -> Vec<String> {
    let lines: Vec<String> = dmesg
        .lines()
        .filter(|l| {
            let l = l.to_ascii_lowercase();
            l.contains("fat-fs")
                || l.contains("exfat")
                || l.contains("remount")
                || l.contains("i/o error")
                || (l.contains("mmcblk") && (l.contains("error") || l.contains("fail")))
        })
        .map(str::to_owned)
        .collect();
    let skip = lines.len().saturating_sub(KERNEL_LINES);
    lines.into_iter().skip(skip).collect()
}

struct MountLine<'a> {
    device: &'a str,
    point: &'a Path,
    fs: &'a str,
    options: &'a str,
}

fn closest_mount<'a>(mounts: &'a str, path: &Path) -> Option<MountLine<'a>> {
    mounts
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let device = f.next()?;
            let point = Path::new(f.next()?);
            let fs = f.next()?;
            let options = f.next()?;
            path.starts_with(point).then_some(MountLine {
                device,
                point,
                fs,
                options,
            })
        })
        .max_by_key(|m| m.point.as_os_str().len())
}

pub fn read_only_in(mounts: &str, path: &Path) -> bool {
    closest_mount(mounts, path).is_some_and(|m| m.options.split(',').any(|o| o == "ro"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardMount {
    pub device: String,
    pub point: PathBuf,
    pub fs: String,
}

pub fn card_mount_in(mounts: &str, path: &Path) -> Option<CardMount> {
    let m = closest_mount(mounts, path)?;
    m.device.starts_with("/dev/").then(|| CardMount {
        device: m.device.to_owned(),
        point: m.point.to_path_buf(),
        fs: m.fs.to_owned(),
    })
}

pub fn stage_repair(root: &Path, stage: &Path) -> std::io::Result<PathBuf> {
    let system = root.join("System");
    fs::create_dir_all(stage)?;
    let script = stage.join(REPAIR_SCRIPT);
    fs::copy(system.join(REPAIR_SCRIPT), &script)?;
    let fsck = system.join(FSCK_EXFAT);
    if fsck.exists() {
        let staged = stage.join(FSCK_EXFAT);
        fs::copy(&fsck, &staged)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(script)
}

pub fn take_repair_result(file: &Path) -> Option<RepairResult> {
    let text = fs::read_to_string(file).ok()?;
    let _ = fs::remove_file(file);
    Some(match text.trim() {
        "repaired" => RepairResult::Repaired,
        "clean" => RepairResult::Clean,
        _ => RepairResult::Failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flips_darkest_step_is_the_backlights_floor_with_the_picture_nearly_black() {
        assert_eq!(flip_backlight_value(0, 255), 0);
        assert_eq!(flip_backlight_value(1, 255), 1);
        assert!(
            flip_picture_dim(1) < 0.07,
            "step 1 at {}",
            flip_picture_dim(1)
        );
        assert_eq!(flip_backlight_value(9, 255), 255);
        assert_eq!(flip_picture_dim(9), 1.0);
    }

    #[test]
    fn every_flip_step_is_brighter_than_the_one_below() {
        let seen = |s: u8| {
            let value = flip_backlight_value(s, 255);
            let lit = 20.0 + f64::from(value.saturating_sub(1)) * 235.0 / 254.0;
            lit * f64::from(flip_picture_dim(s))
        };
        for s in 2..=TOP_STEP as u8 {
            assert!(
                seen(s) > seen(s - 1) * 1.5,
                "step {s} is not clearly brighter"
            );
        }
    }
}
