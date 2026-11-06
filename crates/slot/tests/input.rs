use slot::input::HostInput;
use slot_input::{Btn, InputSource, RawEvent};
use winit::event::WindowEvent;
use winit::keyboard::KeyCode;

fn edge(h: &mut HostInput, code: KeyCode, pressed: bool) -> Vec<RawEvent> {
    h.key(code, pressed, false);
    h.poll(0)
}

#[test]
fn the_host_keymap_matches_the_documented_layout() {
    let map = [
        (KeyCode::ArrowUp, Btn::Up),
        (KeyCode::ArrowDown, Btn::Down),
        (KeyCode::ArrowLeft, Btn::Left),
        (KeyCode::ArrowRight, Btn::Right),
        (KeyCode::KeyZ, Btn::A),
        (KeyCode::KeyX, Btn::B),
        (KeyCode::KeyC, Btn::X),
        (KeyCode::KeyA, Btn::L1),
        (KeyCode::KeyS, Btn::R1),
        (KeyCode::KeyQ, Btn::L2),
        (KeyCode::KeyW, Btn::R2),
        (KeyCode::Enter, Btn::Start),
        (KeyCode::ShiftRight, Btn::Select),
        (KeyCode::Tab, Btn::Menu),
        (KeyCode::Equal, Btn::VolUp),
        (KeyCode::Minus, Btn::VolDown),
        (KeyCode::Backslash, Btn::Power),
    ];
    let mut h = HostInput::new();
    for (code, btn) in map {
        assert_eq!(edge(&mut h, code, true), vec![RawEvent::Down(btn)]);
        assert_eq!(edge(&mut h, code, false), vec![RawEvent::Up(btn)]);
    }
    assert!(edge(&mut h, KeyCode::KeyP, true).is_empty());
}

#[test]
fn key_repeat_does_not_re_press_the_button() {
    let mut h = HostInput::new();
    assert_eq!(
        edge(&mut h, KeyCode::Tab, true),
        vec![RawEvent::Down(Btn::Menu)]
    );
    h.key(KeyCode::Tab, true, true);
    h.key(KeyCode::Tab, true, true);
    assert!(
        h.poll(0).is_empty(),
        "autorepeat would re-arm the menu hold and double tap windows"
    );
}

#[test]
fn the_lid_key_toggles_because_the_host_has_no_hinge() {
    let mut h = HostInput::new();
    assert_eq!(
        edge(&mut h, KeyCode::BracketRight, true),
        vec![RawEvent::Down(Btn::Lid)]
    );
    assert!(edge(&mut h, KeyCode::BracketRight, false).is_empty());
    assert_eq!(
        edge(&mut h, KeyCode::BracketRight, true),
        vec![RawEvent::Up(Btn::Lid)]
    );
    assert_eq!(
        edge(&mut h, KeyCode::BracketRight, true),
        vec![RawEvent::Down(Btn::Lid)]
    );
}

#[test]
fn the_two_reflex_keys_no_longer_end_a_session() {
    let mut h = HostInput::new();
    for reflex in [KeyCode::Escape, KeyCode::KeyL] {
        assert!(
            edge(&mut h, reflex, true).is_empty(),
            "{reflex:?} is bound again, and it can end a live link"
        );
    }
}

#[test]
fn a_window_that_loses_focus_lets_go_of_the_keys_held_in_it() {
    let mut h = HostInput::new();
    assert_eq!(
        edge(&mut h, KeyCode::KeyZ, true),
        vec![RawEvent::Down(Btn::A)]
    );
    assert_eq!(
        edge(&mut h, KeyCode::ArrowRight, true),
        vec![RawEvent::Down(Btn::Right)]
    );
    assert_eq!(
        edge(&mut h, KeyCode::KeyZ, false),
        vec![RawEvent::Up(Btn::A)]
    );
    h.on_window_event(&WindowEvent::Focused(false));
    assert_eq!(
        h.poll(0),
        vec![RawEvent::Up(Btn::Right)],
        "the key still down when the window went away was never released"
    );
    h.on_window_event(&WindowEvent::Focused(false));
    assert!(h.poll(0).is_empty());
    h.on_window_event(&WindowEvent::Focused(true));
    assert!(h.poll(0).is_empty());
}

#[test]
fn the_pad_presses_the_console_buttons_bound_to_what_is_held() {
    use slot::input::Pad;
    use slot_input::Action;
    use slot_retro::ButtonMask;
    use slot_store::{Binding, Controls, Device, GameButton, Modifier, Platform};
    let mut c = Controls::default();
    c.set_game(GameButton::TurboA, None);
    c.set_game(GameButton::TurboB, None);
    let mut pad = Pad::default();
    pad.apply(Action::GbaDown(Btn::X));
    assert_eq!(
        pad.mask(&c, Platform::Gba).0,
        0,
        "X reached the game with nothing bound to it"
    );
    c.set_game(GameButton::A, Binding::plain(Device::X));
    assert_eq!(pad.mask(&c, Platform::Gba).0, ButtonMask::A);
    pad.apply(Action::GbaDown(Btn::A));
    assert_eq!(
        pad.mask(&c, Platform::Gba).0,
        ButtonMask::A,
        "A still pressed A after A moved to X"
    );
    pad.apply(Action::GbaUp(Btn::X));
    assert_eq!(pad.mask(&c, Platform::Gba).0, 0);
    c.set_game(GameButton::B, Binding::with(Modifier::Menu, Device::Y));
    pad.apply(Action::GbaDown(Btn::Y));
    assert_eq!(
        pad.mask(&c, Platform::Gba).0,
        0,
        "plain Y pressed a MENU+Y binding"
    );
    pad.apply(Action::MenuChordDown(Btn::Y));
    assert_eq!(pad.mask(&c, Platform::Gba).0, ButtonMask::B);
}

#[test]
fn turbo_a_and_b_fire_from_whatever_they_are_bound_to() {
    use slot::input::Pad;
    use slot_input::Action;
    use slot_retro::ButtonMask;
    use slot_store::{Binding, Controls, Device, GameButton, Platform};
    let mut c = Controls::default();
    let mut pad = Pad::default();
    pad.apply(Action::GbaDown(Btn::X));
    assert_eq!(pad.mask(&c, Platform::Gba).turbo(0).0, ButtonMask::A);
    assert_eq!(pad.mask(&c, Platform::Gba).turbo(3).0, 0);
    assert_eq!(pad.mask(&c, Platform::Gba).without_turbo().0, 0);
    pad.apply(Action::GbaUp(Btn::X));
    pad.apply(Action::GbaDown(Btn::Y));
    assert_eq!(pad.mask(&c, Platform::Gba).turbo(0).0, ButtonMask::B);
    pad.apply(Action::GbaUp(Btn::Y));
    c.set_game(GameButton::TurboA, Binding::plain(Device::L2));
    pad.apply(Action::GbaDown(Btn::X));
    assert_eq!(
        pad.mask(&c, Platform::Gba).turbo(0).0,
        0,
        "X kept turbo A after it moved"
    );
    pad.apply(Action::GbaDown(Btn::L2));
    assert_eq!(pad.mask(&c, Platform::Gba).turbo(0).0, ButtonMask::A);
}

#[test]
fn the_snes_gets_x_y_l_and_r_where_its_own_pad_has_them() {
    use slot::input::Pad;
    use slot_input::Action;
    use slot_retro::ButtonMask;
    use slot_store::{Controls, Platform};
    let c = Controls::default();
    let mut pad = Pad::default();
    for b in [Btn::A, Btn::B, Btn::X, Btn::Y, Btn::L1, Btn::R1] {
        pad.apply(Action::GbaDown(b));
    }
    let all = ButtonMask::A
        | ButtonMask::B
        | ButtonMask::X
        | ButtonMask::Y
        | ButtonMask::L
        | ButtonMask::R;
    let snes = pad.mask(&c, Platform::Snes);
    assert_eq!(snes.0, all);
    assert_eq!(snes.turbo(3).0, all, "turbo took the SNES X and Y");
    assert_eq!(snes.without_turbo().0, all);
}

#[test]
fn the_nes_and_game_boy_get_turbo_on_x_and_y_and_no_shoulders() {
    use slot::input::Pad;
    use slot_input::Action;
    use slot_retro::ButtonMask;
    use slot_store::{Controls, Platform};
    let c = Controls::default();
    let mut pad = Pad::default();
    for b in [Btn::X, Btn::Y, Btn::L1, Btn::R1] {
        pad.apply(Action::GbaDown(b));
    }
    for platform in [Platform::Nes, Platform::Gb] {
        let m = pad.mask(&c, platform);
        assert_eq!(
            m.0,
            ButtonMask::TURBO_A | ButtonMask::TURBO_B,
            "{platform:?}"
        );
        assert_eq!(m.turbo(0).0, ButtonMask::A | ButtonMask::B);
    }
}
