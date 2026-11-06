use slot_retro::ButtonMask;

fn pressed(mask: u16, frames: std::ops::Range<u32>) -> Vec<u16> {
    frames.map(|f| ButtonMask(mask).turbo(f).0).collect()
}

#[test]
fn turbo_a_pulses_a_three_frames_on_three_off() {
    let a = ButtonMask::A;
    assert_eq!(
        pressed(ButtonMask::TURBO_A, 0..12),
        vec![a, a, a, 0, 0, 0, a, a, a, 0, 0, 0]
    );
}

#[test]
fn turbo_b_pulses_b() {
    let b = ButtonMask::B;
    assert_eq!(pressed(ButtonMask::TURBO_B, 0..6), vec![b, b, b, 0, 0, 0]);
}

#[test]
fn held_a_stays_down_under_turbo() {
    let both = ButtonMask::A | ButtonMask::TURBO_A;
    assert_eq!(pressed(both, 0..6), vec![ButtonMask::A; 6]);
}

#[test]
fn other_buttons_pass_through_and_turbo_bits_never_reach_the_core() {
    let held = ButtonMask::TURBO_A | ButtonMask::TURBO_B | ButtonMask::RIGHT;
    for f in 0..6 {
        let m = ButtonMask(held).turbo(f).0;
        assert_ne!(m & ButtonMask::RIGHT, 0);
        assert_eq!(m & (ButtonMask::TURBO_A | ButtonMask::TURBO_B), 0);
    }
}

#[test]
fn x_and_y_reach_the_core_with_turbo_on_or_off() {
    let held = ButtonMask::X | ButtonMask::Y;
    assert_eq!(ButtonMask(held).without_turbo().0, held);
    for f in 0..6 {
        assert_eq!(ButtonMask(held).turbo(f).0, held);
    }
}
