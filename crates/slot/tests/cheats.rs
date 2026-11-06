mod common;

use common::{app_playing_in, tmp_root_with_carts};
use slot::app::App;
use slot_input::{Action, Btn};
use slot_ui::QuickRow;

const CHT: &str = "cheats = 2\n\
cheat0_desc = \"Infinite Health\"\ncheat0_code = \"82003228+0064\"\ncheat0_enable = false\n\
cheat1_desc = \"Max Money\"\ncheat1_code = \"A1B2C3D4\"\ncheat1_enable = true\n";

fn press(a: &mut App, btn: Btn) {
    a.apply(Action::GbaDown(btn));
    a.apply(Action::GbaUp(btn));
}

fn playing_with_cheats() -> (tempfile::TempDir, App) {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let dir = d.path().join("Cheats/GBA");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Emerald.cht"), CHT).unwrap();
    let mut a = app_playing_in(d.path(), "Emerald");
    assert_eq!(
        a.load_cheats(),
        vec![(1, "A1B2C3D4".to_string())],
        "the core starts with the cheats the file has on"
    );
    (d, a)
}

#[test]
fn a_game_without_a_cht_shows_no_cheats_row() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let mut a = app_playing_in(d.path(), "Emerald");
    assert!(a.load_cheats().is_empty());
    assert!(a.in_game_hidden().contains(&QuickRow::Cheats));
}

#[test]
fn the_cheats_row_opens_a_list_whose_switches_reach_the_core_and_the_card() {
    let (d, mut a) = playing_with_cheats();
    assert!(!a.in_game_hidden().contains(&QuickRow::Cheats));
    a.apply(Action::QuickMenu);
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Down);
    assert_eq!(a.in_game_menu(), Some(QuickRow::Cheats));
    press(&mut a, Btn::A);
    assert_eq!(a.in_game_menu(), Some(QuickRow::CheatList));
    assert_eq!(a.take_cheats(), None, "nothing switched yet");
    press(&mut a, Btn::Right);
    assert_eq!(
        a.take_cheats(),
        Some(vec![
            (0, "82003228+0064".to_string()),
            (1, "A1B2C3D4".to_string())
        ])
    );
    assert_eq!(a.take_cheats(), None, "told once");
    press(&mut a, Btn::Down);
    press(&mut a, Btn::A);
    assert_eq!(
        a.take_cheats(),
        Some(vec![(0, "82003228+0064".to_string())])
    );
    assert_eq!(
        std::fs::read_to_string(d.path().join("Config/cheats/GBA/Emerald.txt")).unwrap(),
        "0\n"
    );
    press(&mut a, Btn::B);
    assert_eq!(a.in_game_menu(), Some(QuickRow::Cheats));
}

#[test]
fn the_choices_come_back_when_the_game_loads_again() {
    let (_d, mut a) = playing_with_cheats();
    a.apply(Action::QuickMenu);
    press(&mut a, Btn::Down);
    press(&mut a, Btn::Down);
    press(&mut a, Btn::A);
    press(&mut a, Btn::Left);
    assert_eq!(
        a.load_cheats(),
        vec![
            (0, "82003228+0064".to_string()),
            (1, "A1B2C3D4".to_string())
        ]
    );
}
