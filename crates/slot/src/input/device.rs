use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use slot_input::{Btn, InputSource, Millis, RawEvent};

use super::evdev::{
    decode, device_name, pick_devices, to_raw_for, Ev, Hat, Layout, EVENT_BYTES, EV_ABS, EV_KEY,
    EV_SYN,
};
use super::trace;

const DEV: &str = "/dev/input";
const SYS: &str = "/sys/class/input";
const PSY: &str = "/sys/class/power_supply";
const MIYOO_HALL: &str = "/sys/devices/platform/hall-mh248/hallvalue";
const GYRO_FILE: &str = "Config/gyro.txt";

/// Config/gyro.txt tunes the stick-driven gyroscope: `mode=rate` (default) with `rate_dps=` (or
/// `rate_left_dps=` and `rate_right_dps=`) and `curve=`, or `mode=angle` with `tilt_deg=` and
/// `follow_ms=`.
fn tune_gyro(root: &Path) {
    let text = std::fs::read_to_string(root.join(GYRO_FILE)).unwrap_or_default();
    let value = |key: &str| {
        text.lines().find_map(|l| {
            l.trim()
                .strip_prefix(key)?
                .strip_prefix('=')?
                .trim()
                .parse::<f32>()
                .ok()
        })
    };
    let angle = text.lines().any(|l| l.trim() == "mode=angle");
    let rate = value("rate_dps").unwrap_or(slot_retro::FULL_RATE_DPS);
    let left = value("rate_left_dps").unwrap_or(rate);
    let right = value("rate_right_dps").unwrap_or(rate);
    let curve = value("curve").unwrap_or(1.0);
    let tilt = value("tilt_deg").unwrap_or(slot_retro::FULL_TILT_DEG);
    let follow = value("follow_ms").map_or(slot_retro::FOLLOW_S, |ms| ms / 1000.0);
    slot_retro::tune_gyro(slot_retro::GyroTuning {
        angle_mode: angle,
        rate_left_dps: left,
        rate_right_dps: right,
        curve,
        tilt_deg: tilt,
        follow_s: follow,
    });
    match angle {
        true => eprintln!(
            "slot: gyro: angle, full tilt {tilt} deg, follow {:.0} ms",
            follow * 1000.0
        ),
        false => eprintln!("slot: gyro: rate, left {left} right {right} deg/s, curve {curve}"),
    }
}

const LID_POLL_MS: Millis = 150;

pub struct DeviceInput {
    pending: Arc<Mutex<Vec<RawEvent>>>,
    stick: Arc<AtomicBool>,
    hall: Option<PathBuf>,
    lid_shut: Option<bool>,
    next_lid_poll: Millis,
}

impl DeviceInput {
    pub fn open(root: &Path) -> Self {
        DeviceInput::open_in(Path::new(DEV), Path::new(SYS), root, trace::enabled())
    }

    pub fn open_in(dev: &Path, sys: &Path, root: &Path, trace: bool) -> Self {
        let trace = trace.then(|| Trace::start(root, dev, sys)).flatten();
        DeviceInput::open_traced(dev, sys, root, trace)
    }

    fn open_traced(dev: &Path, sys: &Path, root: &Path, trace: Option<Arc<Trace>>) -> Self {
        let pending: Arc<Mutex<Vec<RawEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let stick = Arc::new(AtomicBool::new(false));
        for node in pick_devices(dev, sys) {
            let device = device_name(sys, &node);
            let Some(layout) = Layout::of(&device) else {
                eprintln!("slot: input {} ({device}) skipped", node.display());
                continue;
            };
            eprintln!("slot: input {} ({device}, {layout:?})", node.display());
            if layout.gyro_axis().is_some() {
                slot_retro::steer_gyro(0);
                tune_gyro(root);
            }
            quicken_poll(sys, &node);
            let queue = pending.clone();
            let steers = stick.clone();
            let name = node.display().to_string();
            let trace = trace.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("slot-input-{name}"))
                .spawn(move || read_node(&node, layout, &queue, &steers, trace.as_deref()));
            if let Err(e) = spawned {
                eprintln!("slot: input {name}: {e}");
            }
        }
        DeviceInput {
            pending,
            stick,
            hall: find_hall(Path::new(PSY))
                .or_else(|| Some(PathBuf::from(MIYOO_HALL)).filter(|p| p.is_file())),
            lid_shut: None,
            next_lid_poll: 0,
        }
    }
}

const POLL_MS: u32 = 10;

fn quicken_poll(sys: &Path, node: &Path) {
    let Some(name) = node.file_name() else {
        return;
    };
    let attr = sys.join(name).join("device/poll");
    let Some(now) = std::fs::read_to_string(&attr)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
    else {
        return;
    };
    if now > POLL_MS {
        match std::fs::write(&attr, POLL_MS.to_string()) {
            Ok(()) => eprintln!(
                "slot: input {}: poll {now} ms -> {POLL_MS} ms",
                node.display()
            ),
            Err(e) => eprintln!("slot: input {}: poll: {e}", node.display()),
        }
    }
}

fn read_node(
    node: &Path,
    layout: Layout,
    queue: &Mutex<Vec<RawEvent>>,
    steers: &AtomicBool,
    trace: Option<&Trace>,
) {
    let label = node
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| node.display().to_string());
    let mut hat = Hat::for_layout(layout);
    let mut file = match File::open(node) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("slot: input {}: {e}", node.display());
            return;
        }
    };
    let mut buf = [0u8; EVENT_BYTES * 16];
    loop {
        let read = match file.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("slot: input {}: {e}", node.display());
                return;
            }
        };
        let mut events = Vec::new();
        for ev in buf[..read].chunks_exact(EVENT_BYTES).filter_map(decode) {
            if let Some(trace) = trace {
                trace.event(&label, ev);
            }
            if (ev.kind == EV_KEY && ev.value == 1) || (ev.kind == EV_ABS && ev.value != 0) {
                crate::latency::read();
            }
            match ev.kind {
                EV_ABS => {
                    hat.steer_with_stick(steers.load(Ordering::Relaxed));
                    events.extend(hat.feed(ev));
                }
                _ => events.extend(to_raw_for(ev, layout)),
            }
        }
        if events.is_empty() {
            continue;
        }
        queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend(events);
    }
}

struct Trace {
    out: Mutex<File>,
    began: Instant,
}

impl Trace {
    fn start(root: &Path, dev: &Path, sys: &Path) -> Option<Arc<Trace>> {
        let dir = crate::root::logs_dir(root);
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(trace::TRACE_FILE);
        let mut file = match File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("slot: {}: {e}", path.display());
                return None;
            }
        };
        for line in trace::survey(dev, sys) {
            let _ = writeln!(file, "{line}");
        }
        let _ = writeln!(file, "--");
        let _ = file.flush();
        Some(Arc::new(Trace {
            out: Mutex::new(file),
            began: Instant::now(),
        }))
    }

    fn event(&self, node: &str, ev: Ev) {
        if ev.kind == EV_SYN {
            return;
        }
        let line = trace::event_line(node, self.began.elapsed().as_millis(), ev);
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
}

impl DeviceInput {
    pub fn steer_with_stick(&self, on: bool) {
        self.stick.store(on, Ordering::Relaxed);
    }

    fn read_lid(&self) -> Option<bool> {
        let raw = std::fs::read_to_string(self.hall.as_ref()?).ok()?;
        match raw.trim() {
            "0" => Some(true),
            "1" => Some(false),
            _ => None,
        }
    }
}

impl InputSource for DeviceInput {
    fn poll(&mut self, now: Millis) -> Vec<RawEvent> {
        let mut out = std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()));
        if self.hall.is_some() && now >= self.next_lid_poll {
            self.next_lid_poll = now + LID_POLL_MS;
            if let Some(shut) = self.read_lid() {
                if self.lid_shut != Some(shut) {
                    self.lid_shut = Some(shut);
                    out.push(if shut {
                        RawEvent::Down(Btn::Lid)
                    } else {
                        RawEvent::Up(Btn::Lid)
                    });
                }
            }
        }
        out
    }
}

fn find_hall(psy: &Path) -> Option<PathBuf> {
    let mut supplies: Vec<PathBuf> = std::fs::read_dir(psy)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    supplies.sort();
    supplies
        .into_iter()
        .map(|d| d.join("hallkey"))
        .find(|p| p.is_file())
}
