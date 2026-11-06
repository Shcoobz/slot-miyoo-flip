mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{app_playing_in, boot, panel, tmp_root_with_carts, StubSnapshot, CLOCK_IS_SET};
use slot::app::{cpu_cap, App, Phase, CPU_FULL_KHZ, CPU_LIGHT_KHZ};
use slot::frames::LastUpload;
use slot_input::{Action, Btn};
use slot_power::{Battery, Charge, LedState, Platform, Power};
use slot_store::{write_slot_state, Core, SlotState};
use slot_ui::FfState;

#[derive(Clone, Debug, PartialEq)]
enum Call {
    Cpu(u32),
    Volume(u8, bool),
}

/// A device that records what the savings ask of it, with a codec that carries the volume
/// whenever `hardware` says so.
struct Recorder {
    root: PathBuf,
    calls: Arc<Mutex<Vec<Call>>>,
    hardware: Arc<AtomicBool>,
}

impl Platform for Recorder {
    fn set_backlight(&mut self, _step: u8) {}
    fn battery(&self) -> Option<Battery> {
        None
    }
    fn charge(&self) -> Charge {
        Charge::Unknown
    }
    fn set_led(&mut self, _state: LedState) {}
    fn poweroff(&mut self) -> ! {
        panic!("the recorder never powers off")
    }
    fn restart(&mut self) -> ! {
        panic!("the recorder never restarts")
    }
    fn root(&self) -> &Path {
        &self.root
    }
    fn now(&self) -> i64 {
        CLOCK_IS_SET
    }
    fn set_clock(&mut self, _secs: i64) {}
    fn set_rumble(&mut self, _strength: u16) {}
    fn cap_cpu(&mut self, khz: u32) {
        self.calls.lock().unwrap().push(Call::Cpu(khz));
    }
    fn set_volume(&mut self, volume: u8, headphones: bool) {
        self.calls
            .lock()
            .unwrap()
            .push(Call::Volume(volume, headphones));
    }
    fn hardware_volume(&self) -> bool {
        self.hardware.load(Ordering::Relaxed)
    }
}

struct Rig {
    calls: Arc<Mutex<Vec<Call>>>,
    hardware: Arc<AtomicBool>,
}

impl Rig {
    fn cpu(&self) -> Vec<u32> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter_map(|c| match c {
                Call::Cpu(khz) => Some(*khz),
                Call::Volume(..) => None,
            })
            .collect()
    }

    fn volumes(&self) -> Vec<(u8, bool)> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter_map(|c| match c {
                Call::Volume(v, hp) => Some((*v, *hp)),
                Call::Cpu(_) => None,
            })
            .collect()
    }
}

fn recorded(a: &mut App, root: &Path) -> Rig {
    let rig = Rig {
        calls: Arc::default(),
        hardware: Arc::default(),
    };
    a.set_power(Power::new(
        Box::new(Recorder {
            root: root.to_path_buf(),
            calls: rig.calls.clone(),
            hardware: rig.hardware.clone(),
        }),
        Duration::from_secs(300),
    ));
    rig
}

fn frame(a: &mut App) {
    let now = a.now() + 16;
    a.tick_ms(now);
}

fn playing_with(root: &Path, state: SlotState) -> App {
    write_slot_state(
        root,
        &SlotState {
            cart: Some("Emerald".to_string()),
            clock_set: true,
            utc_offset_min: 0,
            ..state
        },
    )
    .expect("write slot.state");
    let mut a = App::boot(root);
    a.set_snapshot(StubSnapshot::boxed());
    a.on_core_ready();
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "{:?}",
        a.phase()
    );
    a
}

fn dimming_after(root: &Path, secs: u16) -> (App, Arc<AtomicU8>) {
    let mut a = playing_with(
        root,
        SlotState {
            dim_screen_s: secs,
            ..Default::default()
        },
    );
    let (power, backlight) = panel(root, Duration::from_secs(300));
    a.set_power(power);
    let now = a.now();
    a.apply_at(Action::GbaUp(Btn::A), now);
    (a, backlight)
}

#[test]
fn the_screen_dims_after_the_setting_and_any_press_lights_it_again() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, backlight) = dimming_after(d.path(), 15);
    let lit = backlight.load(Ordering::Relaxed);
    assert!(lit > 1, "the brightness has to have room to dim");
    let t0 = a.now();
    a.tick_ms(t0 + 14_900);
    assert_eq!(backlight.load(Ordering::Relaxed), lit, "dimmed too soon");
    a.tick_ms(t0 + 15_000);
    assert_eq!(
        backlight.load(Ordering::Relaxed),
        1,
        "not dimmed to the lowest lit step"
    );
    assert!(a.dimmed());
    assert_eq!(
        a.brightness(),
        lit,
        "dimming changed the brightness setting"
    );
    a.apply_at(Action::GbaDown(Btn::B), t0 + 20_000);
    a.tick_ms(t0 + 20_000);
    assert_eq!(
        backlight.load(Ordering::Relaxed),
        lit,
        "a press did not light it back up"
    );
    assert!(!a.dimmed());
}

#[test]
fn dim_screen_off_never_dims() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, backlight) = dimming_after(d.path(), 0);
    let lit = backlight.load(Ordering::Relaxed);
    let t0 = a.now();
    for s in 1..=30 {
        a.tick_ms(t0 + s * 20_000);
    }
    assert_eq!(backlight.load(Ordering::Relaxed), lit);
    assert!(!a.dimmed());
}

#[test]
fn a_held_button_keeps_the_screen_lit() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let (mut a, backlight) = dimming_after(d.path(), 15);
    let lit = backlight.load(Ordering::Relaxed);
    a.set_buttons_held(true);
    let t0 = a.now();
    a.tick_ms(t0 + 60_000);
    assert_eq!(backlight.load(Ordering::Relaxed), lit);
    a.set_buttons_held(false);
    a.tick_ms(t0 + 60_016);
    assert_eq!(backlight.load(Ordering::Relaxed), 1);
}

#[test]
fn dimming_comes_before_auto_sleep_and_a_wake_lights_the_chosen_brightness() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = playing_with(
        d.path(),
        SlotState {
            dim_screen_s: 15,
            auto_sleep_min: 1,
            ..Default::default()
        },
    );
    let (power, backlight) = panel(d.path(), Duration::from_secs(300));
    a.set_power(power);
    let lit = backlight.load(Ordering::Relaxed);
    let t0 = a.now();
    a.apply_at(Action::GbaUp(Btn::A), t0);
    a.tick_ms(t0 + 30_000);
    assert_eq!(backlight.load(Ordering::Relaxed), 1);
    a.tick_ms(t0 + 60_000);
    assert!(a.dozing(), "Auto Sleep no longer follows the dim");
    assert_eq!(backlight.load(Ordering::Relaxed), 0);
    a.apply_at(Action::GbaDown(Btn::A), t0 + 61_000);
    a.tick_ms(t0 + 61_000);
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "{:?}",
        a.phase()
    );
    assert_eq!(backlight.load(Ordering::Relaxed), lit);
}

#[test]
fn the_light_cores_are_capped_and_the_heavy_ones_get_the_whole_cpu() {
    for core in Core::ALL {
        let light = matches!(
            core,
            Core::Gambatte | Core::Fceumm | Core::Nestopia | Core::Snes9x2005 | Core::Gpsp
        );
        let want = if light { CPU_LIGHT_KHZ } else { CPU_FULL_KHZ };
        assert_eq!(cpu_cap(core), want, "{core:?}");
    }
    assert_eq!(CPU_LIGHT_KHZ, 1_200_000);
    assert_eq!(CPU_FULL_KHZ, 1_800_000);
}

#[test]
fn a_light_core_is_capped_in_game_and_fast_forward_lifts_the_cap() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    let rig = recorded(&mut a, d.path());
    a.set_core(Core::Gambatte);
    frame(&mut a);
    assert_eq!(rig.cpu().last(), Some(&CPU_LIGHT_KHZ));
    a.set_ff(FfState::Held);
    frame(&mut a);
    assert_eq!(
        rig.cpu().last(),
        Some(&CPU_FULL_KHZ),
        "fast-forward kept the cap"
    );
    a.set_ff(FfState::Off);
    frame(&mut a);
    assert_eq!(
        rig.cpu().last(),
        Some(&CPU_LIGHT_KHZ),
        "the cap did not come back"
    );
    a.set_core(Core::Mgba);
    frame(&mut a);
    assert_eq!(rig.cpu().last(), Some(&CPU_FULL_KHZ));
}

#[test]
fn the_cap_is_sent_only_when_it_changes() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    let rig = recorded(&mut a, d.path());
    a.set_core(Core::Fceumm);
    for _ in 0..30 {
        frame(&mut a);
    }
    assert_eq!(rig.cpu(), [CPU_LIGHT_KHZ]);
}

#[test]
fn the_shelf_gets_the_whole_cpu() {
    // Two games, or the card's only game would go straight in.
    let d = tmp_root_with_carts(&["Emerald", "Zelda"]);
    let mut a = boot(d.path());
    assert!(matches!(a.phase(), Phase::Shelf), "{:?}", a.phase());
    let rig = recorded(&mut a, d.path());
    a.set_core(Core::Gambatte);
    frame(&mut a);
    assert_eq!(rig.cpu(), [CPU_FULL_KHZ]);
}

#[test]
fn the_codec_gets_each_new_volume_once_and_mute_as_zero() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    let rig = recorded(&mut a, d.path());
    frame(&mut a);
    let start = a.output_volume();
    assert_eq!(rig.volumes(), [(start, false)]);
    let now = a.now();
    a.apply_at(Action::VolumeDown, now);
    a.apply_at(Action::VolumeDown, now);
    a.apply_at(Action::VolumeDown, now);
    frame(&mut a);
    assert_eq!(
        rig.volumes(),
        [(start, false), (start - 15, false)],
        "quick presses are one change for the codec"
    );
    a.apply(Action::MuteToggle);
    frame(&mut a);
    assert_eq!(rig.volumes().last(), Some(&(0, false)));
}

#[test]
fn the_samples_pass_at_full_scale_only_while_the_codec_carries_the_volume() {
    let d = tmp_root_with_carts(&["Emerald"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    let rig = recorded(&mut a, d.path());
    frame(&mut a);
    let volume = a.output_volume();
    assert!(volume > 0 && volume < 100);
    assert_eq!(
        a.sample_volume(),
        volume,
        "no codec volume: the samples carry it"
    );
    rig.hardware.store(true, Ordering::Relaxed);
    assert_eq!(a.sample_volume(), 100);
    a.apply(Action::MuteToggle);
    assert_eq!(
        a.sample_volume(),
        0,
        "a mute is silent before the amplifier goes off"
    );
    a.apply(Action::MuteToggle);
    rig.hardware.store(false, Ordering::Relaxed);
    assert_eq!(
        a.sample_volume(),
        volume,
        "a failed codec hands the volume back"
    );
}

#[test]
fn an_identical_frame_is_not_uploaded_twice() {
    let mut last = LastUpload::default();
    let a = vec![7u8; 240 * 160 * 4];
    let mut b = a.clone();
    b[1000] = 8;
    assert!(last.changed(&a, (240, 160)), "the first frame is new");
    assert!(!last.changed(&a, (240, 160)), "a repeat was uploaded again");
    assert!(
        last.changed(&b, (240, 160)),
        "a changed pixel was not uploaded"
    );
    assert!(!last.changed(&b, (240, 160)));
    let nes = vec![7u8; 256 * 240 * 4];
    assert!(last.changed(&nes, (256, 240)));
    let same_bytes_other_shape = vec![7u8; 256 * 240 * 4];
    assert!(last.changed(&same_bytes_other_shape, (240, 256)));
}
