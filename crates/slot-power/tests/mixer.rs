use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slot_power::{
    control_level, find_control, parse_cget, ControlRange, Mixer, HEADPHONE_CONTROLS,
    SPEAKER_CONTROLS,
};

const CONTROLS: &str = "numid=1,iface=MIXER,name='Playback Path'\n\
numid=2,iface=MIXER,name='Capture MIC Path'\n\
numid=3,iface=MIXER,name='SPK Volume'\n\
numid=4,iface=MIXER,name='hp volume'\n";

const SPK_CGET: &str = "numid=3,iface=MIXER,name='SPK Volume'\n  \
; type=INTEGER,access=rw---R--,values=2,min=0,max=255,step=0\n  \
: values=255,255\n  \
| dBscale-min=-95.25dB,step=0.38dB,mute=1\n";

const PLAIN_CGET: &str = "numid=4,iface=MIXER,name='HP Volume'\n  \
; type=INTEGER,access=rw------,values=1,min=0,max=100,step=0\n  \
: values=100\n";

fn plain(max: i64) -> ControlRange {
    ControlRange {
        min: 0,
        max,
        values: 1,
        step_db: None,
        min_mutes: false,
    }
}

fn scaled(max: i64, step_db: f32) -> ControlRange {
    ControlRange {
        step_db: Some(step_db),
        ..plain(max)
    }
}

#[test]
fn cget_gives_the_range_channels_and_db_scale() {
    assert_eq!(
        parse_cget(SPK_CGET),
        Some(ControlRange {
            min: 0,
            max: 255,
            values: 2,
            step_db: Some(0.38),
            min_mutes: true,
        })
    );
}

#[test]
fn cget_without_a_db_scale_still_gives_the_range() {
    assert_eq!(parse_cget(PLAIN_CGET), Some(plain(100)));
}

#[test]
fn cget_reads_a_min_max_db_range_as_steps() {
    let out = "  ; type=INTEGER,access=rw---R--,values=1,min=0,max=30,step=0\n  \
               : values=20\n  \
               | dBminmax-min=-30.00dB,max=0.00dB\n";
    let range = parse_cget(out).expect("an integer control");
    assert_eq!(range.step_db, Some(1.0));
    assert!(!range.min_mutes);
}

#[test]
fn cget_of_a_switch_or_an_empty_range_is_not_a_volume() {
    let switch = "  ; type=BOOLEAN,access=rw------,values=1\n  : values=on\n";
    assert_eq!(parse_cget(switch), None);
    let empty = "  ; type=INTEGER,access=rw------,values=1,min=3,max=3,step=0\n";
    assert_eq!(parse_cget(empty), None);
    assert_eq!(parse_cget(""), None);
}

#[test]
fn controls_are_found_by_name_whatever_their_case() {
    assert_eq!(
        find_control(CONTROLS, &SPEAKER_CONTROLS),
        Some("SPK Volume".to_string())
    );
    assert_eq!(
        find_control(CONTROLS, &HEADPHONE_CONTROLS),
        Some("hp volume".to_string())
    );
    assert_eq!(find_control(CONTROLS, &["Master Playback Volume"]), None);
}

#[test]
fn volume_zero_turns_the_amplifier_off_and_full_is_the_controls_top() {
    for range in [plain(100), scaled(255, 0.38), scaled(31, 1.5)] {
        assert_eq!(control_level(0, &range), None, "{range:?}");
        assert_eq!(control_level(100, &range), Some(range.max), "{range:?}");
        assert_eq!(control_level(200, &range), Some(range.max), "{range:?}");
    }
}

#[test]
fn every_step_of_five_is_never_quieter_than_the_one_below() {
    for range in [plain(100), plain(7), scaled(255, 0.38), scaled(31, 1.5)] {
        let mut last = control_level(5, &range).expect("a level above zero plays");
        for v in (10..=100).step_by(5) {
            let level = control_level(v, &range).expect("a level above zero plays");
            assert!(level >= last, "{range:?}: volume {v} went down");
            last = level;
        }
    }
}

#[test]
fn a_db_scaled_control_drops_the_same_db_per_step_as_the_software_gain() {
    let range = scaled(255, 0.5);
    let db = |v: u8| (control_level(v, &range).unwrap() - range.max) as f32 * 0.5;
    assert!(
        (db(50) - slot_power::volume_db(50)).abs() <= 0.5,
        "volume 50 at {} dB",
        db(50)
    );
    assert!(
        (db(5) - slot_power::volume_db(5)).abs() <= 0.5,
        "volume 5 at {} dB",
        db(5)
    );
}

#[test]
fn a_control_without_a_db_scale_takes_the_volume_itself_as_spruce_sets_it() {
    let range = plain(100);
    for v in [5, 50, 95] {
        assert_eq!(control_level(v, &range), Some(i64::from(v)), "volume {v}");
    }
}

#[test]
fn the_quietest_step_never_lands_on_a_muting_value() {
    let muting = ControlRange {
        min_mutes: true,
        ..scaled(20, 1.0)
    };
    assert_eq!(control_level(5, &muting), Some(1));
    assert!(control_level(5, &plain(100)).is_some_and(|l| l >= 1));
}

/// A stand-in amixer: answers `controls` and `cget`, takes every `cset`, records each call.
fn fake(
    calls: Arc<Mutex<Vec<String>>>,
    cset_works: bool,
) -> impl FnMut(&[&str]) -> Option<String> + Send + 'static {
    move |args: &[&str]| {
        calls.lock().unwrap().push(args.join(" "));
        match args {
            [.., "controls"] => Some(CONTROLS.to_string()),
            [.., "cget", "name=SPK Volume"] => Some(SPK_CGET.to_string()),
            [.., "cget", "name=hp volume"] => Some(PLAIN_CGET.to_string()),
            _ if args.contains(&"cset") => cset_works.then(String::new),
            _ => None,
        }
    }
}

fn wait_for(what: &str, done: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn csets(calls: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.contains("cset"))
        .cloned()
        .collect()
}

#[test]
fn the_codec_takes_the_volume_then_the_path_and_mute_turns_the_amplifier_off() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut mixer = Mixer::spawn_with(fake(calls.clone(), true));
    mixer.set_volume(100, false);
    wait_for("the codec to carry the volume", || mixer.active());
    assert_eq!(
        csets(&calls),
        [
            "-q -c 0 cset name=SPK Volume 255,255",
            "-q -c 0 cset name=Playback Path SPK"
        ]
    );
    mixer.set_volume(0, false);
    wait_for("the amplifier to go off", || {
        csets(&calls).last().map(String::as_str) == Some("-q -c 0 cset name=Playback Path OFF")
    });
    assert!(mixer.active(), "a muted codec still carries the volume");
}

#[test]
fn a_failing_codec_leaves_the_volume_to_the_samples() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut mixer = Mixer::spawn_with(fake(calls.clone(), false));
    mixer.set_volume(50, false);
    wait_for("the set to be tried", || !csets(&calls).is_empty());
    std::thread::sleep(Duration::from_millis(20));
    assert!(!mixer.active());
}

#[test]
fn no_amixer_leaves_the_volume_to_the_samples() {
    let mut mixer = Mixer::spawn_with(|_: &[&str]| None);
    mixer.set_volume(50, true);
    std::thread::sleep(Duration::from_millis(50));
    assert!(!mixer.active());
}

#[test]
fn an_output_with_no_volume_asked_yet_is_routed_but_left_to_the_samples() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut mixer = Mixer::spawn_with(fake(calls.clone(), true));
    mixer.set_volume(40, false);
    wait_for("the speaker volume", || mixer.active());
    mixer.route(true);
    wait_for("the samples to take the volume back", || !mixer.active());
    assert_eq!(
        csets(&calls).last().map(String::as_str),
        Some("-q -c 0 cset name=Playback Path HP"),
        "the headphones' control is still at its old level, so only the path is set"
    );
}

#[test]
fn the_samples_quieten_only_the_bottom_of_a_codec_with_no_db_scale() {
    assert!(
        slot_power::assist_volume(5) < 50,
        "step 1 is not taken down"
    );
    assert_eq!(slot_power::assist_volume(40), 100);
    assert_eq!(slot_power::assist_volume(100), 100);
    let mut last = 0;
    for v in (5..=40).step_by(5) {
        let s = slot_power::assist_volume(v);
        assert!(s >= last, "volume {v} quieter than the step below");
        last = s;
    }
}

#[test]
fn the_volume_curve_is_low_at_the_bottom_and_loud_in_the_upper_half() {
    assert!((slot_power::volume_db(1) - -58.8).abs() < 0.5);
    assert!((slot_power::volume_db(50) - -15.0).abs() < 0.1);
    assert!(slot_power::volume_db(80) > -3.0);
    assert_eq!(slot_power::volume_db(100), 0.0);
}
