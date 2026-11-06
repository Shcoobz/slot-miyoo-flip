//! The codec's own volume on the Miyoo Flip. Scaling the samples throws away resolution at
//! every step below full; setting the codec's control lets the whole 16 bits reach it, and at
//! volume 0 or mute the amplifier is switched off instead of playing silence.
//!
//! `amixer` takes a few milliseconds a run, so a thread of its own runs it: a volume press never
//! stalls a frame, and when presses come faster than it runs only the last is set.

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// The quietest volume step's level in dB below full: barely there.
pub const VOLUME_FLOOR_DB: f32 = -60.0;

/// The level in dB for Slot's volume `1..=100`: the floor at the bottom, bending up so the low
/// steps stay low and the upper half is loud (-15 dB at 50, -2.4 dB at 80). The software gain and
/// a dB-scaled codec share it.
pub fn volume_db(volume: u8) -> f32 {
    let t = f32::from(volume.clamp(1, 100)) / 100.0;
    VOLUME_FLOOR_DB * (1.0 - t) * (1.0 - t)
}

/// Where the samples take over from a codec with no dB scale: under this volume the codec alone
/// is still too loud (the Flip's `SPK Volume` is plainly audible at 5 of 100).
const ASSIST_UNDER: f32 = 40.0;
/// How much the samples take off at the quietest step on such a codec.
const ASSIST_FLOOR_DB: f32 = -30.0;

/// The level in dB the samples add for Slot's volume on a codec with no dB scale: 0 from
/// `ASSIST_UNDER` up, down to `ASSIST_FLOOR_DB` at the bottom.
pub fn assist_db(volume: u8) -> f32 {
    let short = (1.0 - f32::from(volume.min(100)) / ASSIST_UNDER).max(0.0);
    ASSIST_FLOOR_DB * short * short
}

/// The sample volume whose `volume_db` is `assist_db(volume)`: what the samples are scaled to
/// while a codec with no dB scale carries the rest.
pub fn assist_volume(volume: u8) -> u8 {
    let db = assist_db(volume);
    if db >= 0.0 {
        return 100;
    }
    (100.0 * (1.0 - (db / VOLUME_FLOOR_DB).sqrt()))
        .round()
        .clamp(1.0, 100.0) as u8
}

/// The speaker's and the headphones' volume controls, by the names codecs give them, the
/// Flip's first.
pub const SPEAKER_CONTROLS: [&str; 3] = ["SPK Volume", "Speaker Volume", "Speaker Playback Volume"];
pub const HEADPHONE_CONTROLS: [&str; 3] =
    ["HP Volume", "Headphone Volume", "Headphone Playback Volume"];

const PATH_CONTROL: &str = "Playback Path";

/// An integer control's range, as `amixer cget` prints it.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlRange {
    pub min: i64,
    pub max: i64,
    /// How many channels the control holds; a set writes the level to each.
    pub values: usize,
    /// dB per step, where the control publishes its dB scale.
    pub step_db: Option<f32>,
    /// The lowest value mutes rather than plays quietly.
    pub min_mutes: bool,
}

/// Reads an integer control's range from `amixer cget` output:
///
/// ```text
///   ; type=INTEGER,access=rw---R--,values=1,min=0,max=100,step=0
///   : values=100
///   | dBscale-min=-95.25dB,step=0.75dB,mute=0
/// ```
pub fn parse_cget(out: &str) -> Option<ControlRange> {
    fn fields(line: &str) -> Vec<(String, String)> {
        line.trim_start_matches([' ', ';', '|', ':'])
            .split(',')
            .filter_map(|kv| kv.trim().split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    fn field(fields: &[(String, String)], key: &str) -> Option<String> {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }
    let kind = out
        .lines()
        .map(fields)
        .find(|f| field(f, "type").as_deref() == Some("INTEGER"))?;
    let min: i64 = field(&kind, "min")?.parse().ok()?;
    let max: i64 = field(&kind, "max")?.parse().ok()?;
    if max <= min {
        return None;
    }
    let values = field(&kind, "values")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1usize)
        .max(1);
    let db = |v: Option<String>| v.and_then(|v| v.strip_suffix("dB")?.parse::<f32>().ok());
    let mut step_db = None;
    let mut min_mutes = false;
    for line in out.lines().map(fields) {
        if field(&line, "dBscale-min").is_some() {
            step_db = db(field(&line, "step"));
            min_mutes = field(&line, "mute").as_deref() == Some("1");
        } else if let Some(lo) = db(field(&line, "dBminmax-min")) {
            step_db = db(field(&line, "max")).map(|hi| (hi - lo) / (max - min) as f32);
        } else if let Some(lo) = db(field(&line, "dBminmaxmute-min")) {
            step_db = db(field(&line, "max")).map(|hi| (hi - lo) / (max - min) as f32);
            min_mutes = true;
        }
    }
    Some(ControlRange {
        min,
        max,
        values,
        step_db: step_db.filter(|s| *s > 0.0),
        min_mutes,
    })
}

/// The first of `names` that `amixer controls` lists, as it spells it there; case is not
/// trusted to match.
pub fn find_control(controls: &str, names: &[&str]) -> Option<String> {
    let listed: Vec<&str> = controls
        .lines()
        .filter_map(|l| {
            let rest = l.split_once("name='")?.1;
            Some(&rest[..rest.rfind('\'')?])
        })
        .collect();
    names.iter().find_map(|want| {
        listed
            .iter()
            .find(|name| name.eq_ignore_ascii_case(want))
            .map(|name| name.to_string())
    })
}

/// The control value for Slot's volume `0..=100`, or None at 0, where the amplifier goes off.
/// 100 is the control's top; a dB-scaled control follows `volume_db`. A control without a dB
/// scale takes the volume itself, as spruce sets the Flip's `SPK Volume` (5 to 100): it is loud
/// enough at the top that way, and `assist_db` quietens the bottom in the samples. A volume above
/// 0 never lands on a value that could mute.
pub fn control_level(volume: u8, range: &ControlRange) -> Option<i64> {
    let v = volume.min(100);
    if v == 0 {
        return None;
    }
    let lowest = match range.min_mutes || range.step_db.is_none() {
        true => range.min + 1,
        false => range.min,
    }
    .min(range.max);
    let level = match range.step_db {
        Some(step) => range.max - (-volume_db(v) / step).round() as i64,
        None => range.min + ((range.max - range.min) as f32 * f32::from(v) / 100.0).round() as i64,
    };
    Some(level.clamp(lowest, range.max))
}

/// What the codec should play: the volume for the output, None where it is not known yet, so
/// only the path is set and the samples keep the volume.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
struct Output {
    volume: Option<u8>,
    headphones: bool,
}

struct Control {
    name: String,
    range: ControlRange,
}

struct Codec {
    speaker: Option<Control>,
    headphones: Option<Control>,
}

/// Runs amixer with `args`: its output, or None when it did not run or failed.
type Run = dyn FnMut(&[&str]) -> Option<String> + Send;

impl Codec {
    fn probe(run: &mut Run) -> Codec {
        let Some(controls) = run(&["-c", "0", "controls"]) else {
            return Codec {
                speaker: None,
                headphones: None,
            };
        };
        let mut control = |names: &[&str]| {
            let name = find_control(&controls, names)?;
            let id = format!("name={name}");
            let range = parse_cget(&run(&["-c", "0", "cget", id.as_str()])?)?;
            Some(Control { name, range })
        };
        let speaker = control(&SPEAKER_CONTROLS[..]);
        let headphones = control(&HEADPHONE_CONTROLS[..]);
        for (what, c) in [("speaker", &speaker), ("headphones", &headphones)] {
            match c {
                Some(c) => eprintln!(
                    "slot: mixer: {what} '{}' {}..{}, {:?} dB a step",
                    c.name, c.range.min, c.range.max, c.range.step_db
                ),
                None => eprintln!("slot: mixer: no {what} volume, the samples carry it"),
            }
        }
        Codec {
            speaker,
            headphones,
        }
    }

    /// Puts `want` on the codec. A path change goes the way spruce found the Flip's driver needs:
    /// the driver lands a path write on a raw stored level, so the level is parked high (quiet
    /// there) while the path is off, the path set, and only then the level. True when the codec
    /// now carries the volume, false when the samples must; `path` is the path last set,
    /// `reroute` sets it again regardless.
    fn apply(
        &self,
        want: Output,
        reroute: bool,
        path: &mut Option<&'static str>,
        run: &mut Run,
    ) -> bool {
        let routed = if want.headphones { "HP" } else { "SPK" };
        let control = match want.headphones {
            true => &self.headphones,
            false => &self.speaker,
        };
        let (Some(control), Some(volume)) = (control, want.volume) else {
            set_path(routed, reroute, path, run);
            return false;
        };
        match control_level(volume, &control.range) {
            None => set_path("OFF", reroute, path, run),
            Some(level) => {
                let id = format!("name={}", control.name);
                let set = |run: &mut Run, value: i64| {
                    let value = vec![value.to_string(); control.range.values].join(",");
                    run(&["-q", "-c", "0", "cset", id.as_str(), value.as_str()]).is_some()
                };
                let moving = reroute || *path != Some(routed);
                if moving && control.range.step_db.is_none() {
                    set_path("OFF", true, path, run);
                    let park = match level == control.range.max {
                        true => control.range.max - 5,
                        false => control.range.max,
                    };
                    set(run, park);
                }
                if !moving || control.range.step_db.is_some() {
                    if !set(run, level) {
                        eprintln!("slot: mixer: setting '{}' failed", control.name);
                        return false;
                    }
                    set_path(routed, reroute, path, run);
                    return true;
                }
                set_path(routed, true, path, run);
                if !set(run, level) {
                    eprintln!("slot: mixer: setting '{}' failed", control.name);
                    return false;
                }
                true
            }
        }
    }
}

fn set_path(
    to: &'static str,
    reroute: bool,
    path: &mut Option<&'static str>,
    run: &mut Run,
) -> bool {
    if !reroute && *path == Some(to) {
        return true;
    }
    let id = format!("name={PATH_CONTROL}");
    let set = run(&["-q", "-c", "0", "cset", id.as_str(), to]).is_some();
    *path = set.then_some(to);
    if !set {
        eprintln!("slot: mixer: playback path {to} failed");
    }
    set
}

#[derive(Default)]
struct Pending {
    want: Option<Output>,
    reroute: bool,
}

struct Shared {
    pending: Mutex<Pending>,
    wake: Condvar,
    active: AtomicBool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Pending> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The handle to the mixer thread.
pub struct Mixer {
    shared: Arc<Shared>,
    /// The last volume asked for each output, speaker then headphones.
    volumes: [Option<u8>; 2],
}

impl Mixer {
    /// The mixer on card 0, set with the system's `amixer`.
    pub fn spawn() -> Mixer {
        Mixer::spawn_with(amixer)
    }

    /// The mixer, with `run` standing in for amixer: given its arguments, its output or None.
    pub fn spawn_with(run: impl FnMut(&[&str]) -> Option<String> + Send + 'static) -> Mixer {
        let shared = Arc::new(Shared {
            pending: Mutex::new(Pending::default()),
            wake: Condvar::new(),
            active: AtomicBool::new(false),
        });
        let worker = shared.clone();
        let mut run: Box<Run> = Box::new(run);
        let spawned = std::thread::Builder::new()
            .name("slot-mixer".into())
            .spawn(move || {
                let codec = Codec::probe(&mut *run);
                let mut path = None;
                loop {
                    let (want, reroute) = {
                        let mut p = worker.lock();
                        while p.want.is_none() {
                            p = worker.wake.wait(p).unwrap_or_else(|e| e.into_inner());
                        }
                        (p.want.take(), std::mem::take(&mut p.reroute))
                    };
                    if let Some(want) = want {
                        let on = codec.apply(want, reroute, &mut path, &mut *run);
                        worker.active.store(on, Ordering::Release);
                    }
                }
            });
        if let Err(e) = spawned {
            eprintln!("slot: mixer: {e}, the samples carry the volume");
        }
        Mixer {
            shared,
            volumes: [None; 2],
        }
    }

    /// Asks for `volume` on the output; returns at once, and the last ask wins.
    pub fn set_volume(&mut self, volume: u8, headphones: bool) {
        self.volumes[usize::from(headphones)] = Some(volume);
        self.ask(
            Output {
                volume: Some(volume),
                headphones,
            },
            false,
        );
    }

    /// Sends the sound to the headphones or the speaker at that output's last volume, setting
    /// the path again even if it looks set, as after a sleep.
    pub fn route(&mut self, headphones: bool) {
        let volume = self.volumes[usize::from(headphones)];
        self.ask(Output { volume, headphones }, true);
    }

    /// True while the codec carries the volume.
    pub fn active(&self) -> bool {
        self.shared.active.load(Ordering::Acquire)
    }

    fn ask(&self, want: Output, reroute: bool) {
        let mut p = self.shared.lock();
        p.want = Some(want);
        p.reroute |= reroute;
        self.shared.wake.notify_one();
    }
}

fn amixer(args: &[&str]) -> Option<String> {
    let out = Command::new("amixer").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}
