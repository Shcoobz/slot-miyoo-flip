mod common;

use std::path::Path;

use common::{app_playing_in, boot, tmp_root_with_carts};
use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_store::{read_slot_state, write_slot_state, Core, Platform, StateRing};
use tempfile::TempDir;

fn card() -> TempDir {
    tmp_root_with_carts(&["Emerald", "Fusion", "Ruby"])
}

fn played(root: &Path, stems: &[&str]) {
    let mut s = read_slot_state(root);
    for stem in stems {
        s.played(Platform::Gba, stem);
    }
    write_slot_state(root, &s).expect("write slot.state");
}

fn settle(a: &mut App) {
    for _ in 0..120 {
        a.update(1.0 / 60.0);
    }
}

fn title(a: &App) -> String {
    a.polaroid_title("2026-08-09_12-00-00")
}

fn title_of(a: &App, stem: &str) -> String {
    let cart = a.carts().find(|c| c.stem == stem).expect("cart");
    slot_ui::label_text(cart)
}

fn inserting(a: &App) -> Option<&str> {
    match a.phase() {
        Phase::Inserting { cart, .. } => Some(cart),
        _ => None,
    }
}

#[test]
fn the_switcher_lists_the_recent_games_newest_first() {
    let d = card();
    played(d.path(), &["Ruby", "Emerald", "Fusion"]);
    let mut a = boot(d.path());
    a.apply(Action::GameSwitcher);
    assert!(a.games_open());
    assert!(matches!(a.phase(), Phase::Shelf));
    assert_eq!(a.polaroid_entries().len(), 3);
    assert_eq!(title(&a), title_of(&a, "Fusion"));
    a.apply(Action::GbaDown(Btn::Right));
    assert_eq!(title(&a), title_of(&a, "Emerald"));
}

#[test]
fn a_game_whose_cart_is_gone_is_left_out() {
    let d = card();
    played(d.path(), &["Emerald", "Missing"]);
    let mut a = boot(d.path());
    a.apply(Action::GameSwitcher);
    assert_eq!(a.polaroid_entries().len(), 1);
    assert_eq!(title(&a), title_of(&a, "Emerald"));
}

#[test]
fn a_puts_the_selected_game_in() {
    let d = card();
    played(d.path(), &["Emerald", "Fusion"]);
    let mut a = boot(d.path());
    a.apply(Action::GameSwitcher);
    a.apply(Action::GbaDown(Btn::Right));
    a.apply(Action::GbaDown(Btn::A));
    assert!(!a.games_open());
    assert_eq!(inserting(&a), Some("Emerald"));
    a.on_core_ready();
    settle(&mut a);
    assert!(matches!(a.phase(), Phase::Playing { cart } if cart == "Emerald"));
}

#[test]
fn b_and_a_menu_tap_close_the_switcher() {
    let d = card();
    played(d.path(), &["Emerald"]);
    let mut a = boot(d.path());
    for close in [
        Action::GbaDown(Btn::B),
        Action::QuickMenu,
        Action::GameSwitcher,
    ] {
        a.apply(Action::GameSwitcher);
        assert!(a.games_open());
        a.apply(close);
        assert!(!a.games_open(), "{close:?}");
        assert!(matches!(a.phase(), Phase::Shelf), "{close:?}");
    }
}

#[test]
fn the_shelf_takes_no_input_while_the_switcher_is_open() {
    let d = card();
    played(d.path(), &["Emerald"]);
    let mut a = boot(d.path());
    a.apply(Action::GameSwitcher);
    assert_eq!(a.input_context(), slot_input::Context::Menu);
    a.apply(Action::GbaDown(Btn::Start));
    assert!(a.core_picker().is_none());
}

#[test]
fn with_no_recent_games_the_switcher_refuses() {
    let d = card();
    let mut a = boot(d.path());
    a.apply(Action::GameSwitcher);
    assert!(!a.games_open());
    assert!(a.refusal_active(a.now()));
}

#[test]
fn r3_in_game_ejects_then_opens_the_switcher_with_the_resume_picture() {
    let d = card();
    played(d.path(), &["Fusion"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.apply(Action::GameSwitcher);
    assert!(matches!(a.phase(), Phase::Ejecting { .. }));
    assert!(!a.games_open(), "opened before the cart was out");
    settle(&mut a);
    assert!(matches!(a.phase(), Phase::Shelf));
    assert!(a.games_open());
    assert_eq!(title(&a), title_of(&a, "Emerald"));
    let ring = StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald");
    let thumb = ring.resume_thumb().expect("no resume thumb");
    assert_eq!(a.polaroid_entries()[0].thumb, thumb);
}

#[test]
fn x_opens_the_games_save_states_once_it_is_playing() {
    let d = card();
    played(d.path(), &["Emerald"]);
    StateRing::new(d.path(), Platform::Gba, Core::Mgba, "Emerald")
        .push(&[1u8; 16], b"png", "2026-08-09_00-00-01")
        .expect("push");
    let mut a = boot(d.path());
    a.apply(Action::GameSwitcher);
    a.apply(Action::GbaDown(Btn::X));
    assert_eq!(inserting(&a), Some("Emerald"));
    a.on_core_ready();
    settle(&mut a);
    assert!(
        matches!(a.phase(), Phase::Polaroids { .. }),
        "{:?}",
        a.phase()
    );
    assert!(!a.games_open());
}

#[test]
fn x_on_a_game_without_states_refuses_and_stays() {
    let d = card();
    played(d.path(), &["Emerald"]);
    let mut a = boot(d.path());
    a.apply(Action::GameSwitcher);
    a.apply(Action::GbaDown(Btn::X));
    assert!(a.games_open());
    assert!(matches!(a.phase(), Phase::Shelf));
    assert!(a.refusal_active(a.now()));
}

#[test]
fn closing_the_lid_closes_the_switcher() {
    let d = card();
    played(d.path(), &["Emerald"]);
    let mut a = boot(d.path());
    a.apply(Action::GameSwitcher);
    a.apply(Action::LidClose);
    a.apply(Action::LidOpen);
    assert!(matches!(a.phase(), Phase::Shelf));
    assert!(!a.games_open());
    assert!(!a.polaroids_open());
}

#[test]
fn a_linked_game_refuses_the_switcher() {
    let d = card();
    played(d.path(), &["Fusion"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    a.begin_link(0);
    a.apply(Action::GameSwitcher);
    assert!(matches!(a.phase(), Phase::Playing { .. }));
    assert!(a.refusal_active(a.now()));
}

#[test]
fn y_takes_a_game_off_the_switcher_but_leaves_it_on_the_card() {
    let d = card();
    played(d.path(), &["Ruby", "Emerald", "Fusion"]);
    let mut a = boot(d.path());
    a.apply(Action::GameSwitcher);
    a.apply(Action::GbaDown(Btn::Y));
    assert!(a.games_open());
    assert_eq!(a.polaroid_entries().len(), 2);
    assert_eq!(title(&a), title_of(&a, "Emerald"));
    let state = read_slot_state(d.path());
    assert!(
        state.recents.iter().all(|r| r.stem != "Fusion"),
        "Fusion is still a recent game"
    );
    assert!(d.path().join("Games/GBA/Fusion.gba").is_file());
    assert!(
        a.carts().any(|c| c.stem == "Fusion"),
        "Fusion left its shelf"
    );
}

#[test]
fn removing_the_last_game_closes_the_switcher() {
    let d = card();
    played(d.path(), &["Emerald"]);
    let mut a = boot(d.path());
    a.apply(Action::GameSwitcher);
    a.apply(Action::GbaDown(Btn::Y));
    assert!(!a.games_open());
    assert!(!a.polaroids_open());
    assert!(matches!(a.phase(), Phase::Shelf));
}
