use std::path::{Path, PathBuf};

use slot_input::{Btn, RawEvent};

pub const EVENT_BYTES: usize = 24;

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_SW: u16 = 0x05;
pub const SW_LID: u16 = 0x00;

pub const EV_ABS: u16 = 0x03;
pub const ABS_HAT0X: u16 = 0x10;
pub const ABS_HAT0Y: u16 = 0x11;
pub const ABS_LSX: u16 = 0x02;
pub const ABS_LSY: u16 = 0x03;
pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const ABS_Z: u16 = 0x02;
pub const ABS_RZ: u16 = 0x05;
/// The right stick's X on the Flip's pad (its Y is 0x04).
pub const MIYOO_RIGHT_X: u16 = 0x03;

pub const TRIGGER_PULLED: i32 = 127;

pub const MIYOO_PAD: &str = "MIYOO Player1";
pub const MIYOO_LID: &str = "hall wake key";

pub const STICK_DEADZONE: i32 = 2048;
/// Half of miyoo_inputd's +-32767; the Flip's sticks rest as far out as +-10000.
pub const MIYOO_STICK_DEADZONE: i32 = 16384;

const LEFT_STICK_TARGETS: [&str; 7] = [
    "my355",
    "rg34xxsp",
    "rg35xxh",
    "rg35xxpro",
    "rg40xxh",
    "rg40xxv",
    "rgcubexx",
];

pub fn has_left_stick(target: &str) -> bool {
    LEFT_STICK_TARGETS.contains(&target)
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Layout {
    #[default]
    Anbernic,
    MiyooPad,
}

impl Layout {
    pub fn of(name: &str) -> Option<Layout> {
        match name {
            MIYOO_PAD => Some(Layout::MiyooPad),
            MIYOO_LID => None,
            _ => Some(Layout::Anbernic),
        }
    }

    fn button(self, code: u16) -> Option<Btn> {
        match self {
            Layout::Anbernic => code_to_btn(code),
            Layout::MiyooPad => miyoo_pad_btn(code),
        }
    }

    fn stick(self) -> [u16; 2] {
        match self {
            Layout::Anbernic => [ABS_LSX, ABS_LSY],
            Layout::MiyooPad => [ABS_X, ABS_Y],
        }
    }

    fn deadzone(self) -> i32 {
        match self {
            Layout::Anbernic => STICK_DEADZONE,
            Layout::MiyooPad => MIYOO_STICK_DEADZONE,
        }
    }

    /// The axis that turns the emulated gyroscope: the right stick's X on the Flip.
    pub fn gyro_axis(self) -> Option<u16> {
        match self {
            Layout::Anbernic => None,
            Layout::MiyooPad => Some(MIYOO_RIGHT_X),
        }
    }

    fn triggers(self) -> Option<[u16; 2]> {
        match self {
            Layout::Anbernic => None,
            Layout::MiyooPad => Some([ABS_Z, ABS_RZ]),
        }
    }
}

#[derive(Default)]
pub struct Hat {
    layout: Layout,
    hat: [i32; 2],
    stick: [i32; 2],
    pulled: [bool; 2],
    steers: bool,
}

impl Hat {
    pub fn for_layout(layout: Layout) -> Self {
        Hat {
            layout,
            ..Hat::default()
        }
    }

    pub fn steer_with_stick(&mut self, on: bool) {
        self.steers = on;
    }

    pub fn feed(&mut self, ev: Ev) -> Vec<RawEvent> {
        let deadzone = self.layout.deadzone();
        let tilt = |v: i32| match v {
            v if v > deadzone => 1,
            v if v < -deadzone => -1,
            _ => 0,
        };
        if self.layout.gyro_axis() == Some(ev.code) {
            slot_retro::steer_gyro(ev.value);
            return Vec::new();
        }
        if let Some(side) = self
            .layout
            .triggers()
            .and_then(|t| t.iter().position(|c| *c == ev.code))
        {
            return self.pull(side, ev.value > TRIGGER_PULLED);
        }
        let steer = |v: i32| if self.steers { tilt(v) } else { 0 };
        let [stick_x, stick_y] = self.layout.stick();
        let (axis, from_stick, value) = match ev.code {
            ABS_HAT0X => (0, false, ev.value.signum()),
            ABS_HAT0Y => (1, false, ev.value.signum()),
            c if c == stick_x => (0, true, steer(ev.value)),
            c if c == stick_y => (1, true, steer(ev.value)),
            _ => return Vec::new(),
        };
        let was = self.held(axis);
        match from_stick {
            true => self.stick[axis] = value,
            false => self.hat[axis] = value,
        }
        let now = self.held(axis);
        if now == was {
            return Vec::new();
        }
        let ends = [[Btn::Left, Btn::Right], [Btn::Up, Btn::Down]][axis];
        let btn = |v: i32| ends[usize::from(v > 0)];
        let mut out = Vec::new();
        if was != 0 {
            out.push(RawEvent::Up(btn(was)));
        }
        if now != 0 {
            out.push(RawEvent::Down(btn(now)));
        }
        out
    }

    fn pull(&mut self, side: usize, pulled: bool) -> Vec<RawEvent> {
        if self.pulled[side] == pulled {
            return Vec::new();
        }
        self.pulled[side] = pulled;
        let btn = [Btn::L2, Btn::R2][side];
        vec![if pulled {
            RawEvent::Down(btn)
        } else {
            RawEvent::Up(btn)
        }]
    }

    fn held(&self, axis: usize) -> i32 {
        match self.hat[axis] {
            0 => self.stick[axis],
            hat => hat,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Ev {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

pub fn decode(bytes: &[u8]) -> Option<Ev> {
    let packet: &[u8; EVENT_BYTES] = bytes.get(..EVENT_BYTES)?.try_into().ok()?;
    Some(Ev {
        kind: u16::from_le_bytes([packet[16], packet[17]]),
        code: u16::from_le_bytes([packet[18], packet[19]]),
        value: i32::from_le_bytes([packet[20], packet[21], packet[22], packet[23]]),
    })
}

pub fn to_raw(ev: Ev) -> Option<RawEvent> {
    to_raw_for(ev, Layout::Anbernic)
}

pub fn to_raw_for(ev: Ev, layout: Layout) -> Option<RawEvent> {
    let btn = match ev.kind {
        EV_KEY => layout.button(ev.code)?,
        EV_SW if ev.code == SW_LID => Btn::Lid,
        _ => return None,
    };
    match ev.value {
        0 => Some(RawEvent::Up(btn)),
        1 => Some(RawEvent::Down(btn)),
        _ => None,
    }
}

pub fn code_to_btn(code: u16) -> Option<Btn> {
    Some(match code {
        0x130 => Btn::A,
        0x131 => Btn::B,
        0x132 => Btn::Y,
        0x133 => Btn::X,
        0x134 => Btn::L1,
        0x135 => Btn::R1,
        0x136 => Btn::Select,
        0x137 => Btn::Start,
        0x138 => Btn::Menu,
        0x13a => Btn::L2,
        0x13b => Btn::R2,
        115 => Btn::VolUp,
        114 => Btn::VolDown,
        116 => Btn::Power,
        _ => return None,
    })
}

pub fn miyoo_pad_btn(code: u16) -> Option<Btn> {
    Some(match code {
        0x130 => Btn::B,
        0x131 => Btn::A,
        0x133 => Btn::Y,
        0x134 => Btn::X,
        0x136 => Btn::L1,
        0x137 => Btn::R1,
        0x13a => Btn::Select,
        0x13b => Btn::Start,
        0x13c => Btn::Menu,
        0x13d => Btn::L3,
        0x13e => Btn::R3,
        _ => return None,
    })
}

const WANTED: [u16; 15] = [
    0x130, 0x131, 0x132, 0x133, 0x134, 0x135, 0x136, 0x137, 0x138, 0x13a, 0x13b, 0x162, 115, 114,
    116,
];

pub use slot_power::has_bit;

pub fn pick_devices(dev: &Path, sys: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dev) else {
        return Vec::new();
    };
    let mut nodes: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("event"))
        })
        .collect();
    nodes.sort();
    nodes.retain(|node| {
        let Some(name) = node.file_name() else {
            return false;
        };
        wanted_node(&sys.join(name).join("device"))
    });
    nodes
}

fn wanted_node(device: &Path) -> bool {
    let cap = |file: &str| std::fs::read_to_string(device.join("capabilities").join(file));
    let keys = cap("key").unwrap_or_default();
    let lid = cap("sw").is_ok_and(|sw| has_bit(&sw, SW_LID));
    lid || WANTED.iter().any(|bit| has_bit(&keys, *bit))
}

pub fn device_name(sys: &Path, node: &Path) -> String {
    node.file_name()
        .and_then(|n| std::fs::read_to_string(sys.join(n).join("device/name")).ok())
        .map(|n| n.trim().to_string())
        .unwrap_or_else(|| node.display().to_string())
}
