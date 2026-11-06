mod common;

use slot::app::App;
use slot_input::{Action, Btn};
use slot_store::{read_slot_state, write_slot_state, Platform, Recent, SlotState};

fn emerald() -> Recent {
    Recent {
        platform: Platform::Gba,
        stem: "Emerald".into(),
    }
}

fn press(app: &mut App, b: Btn) {
    app.apply(Action::GbaDown(b));
    app.apply(Action::GbaUp(b));
    app.update(0.5);
}

/// A card with Emerald (labelled, saved, a favourite and a recent) and Zelda.
fn card() -> (tempfile::TempDir, App) {
    let d = common::tmp_root_with_carts(&["Emerald", "Zelda"]);
    let root = d.path();
    std::fs::create_dir_all(root.join("Labels/GBA")).unwrap();
    std::fs::write(root.join("Labels/GBA/Emerald.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    std::fs::create_dir_all(root.join("Saves/GBA")).unwrap();
    std::fs::write(root.join("Saves/GBA/Emerald.srm"), [1u8; 64]).unwrap();
    std::fs::create_dir_all(root.join("Cheats/GBA")).unwrap();
    std::fs::write(root.join("Cheats/GBA/Emerald.cht"), "cheats = 0\n").unwrap();
    slot_store::write_selected_core(root, "Emerald", slot_store::Core::Mgba).unwrap();
    write_slot_state(
        root,
        &SlotState {
            clock_set: true,
            recents: vec![emerald()],
            favorites: vec![emerald()],
            ..Default::default()
        },
    )
    .unwrap();
    let mut app = App::boot(root);
    while app.selected_stem() != Some("Emerald") {
        app.apply(Action::NextShelf);
        app.update(0.5);
    }
    (d, app)
}

#[test]
fn y_on_the_open_cartridge_asks_and_x_takes_the_game_off_the_card_but_keeps_its_saves() {
    let (d, mut app) = card();
    let root = d.path();
    press(&mut app, Btn::Start);
    press(&mut app, Btn::Y);
    assert!(
        root.join("Games/GBA/Emerald.gba").is_file(),
        "Y alone deleted the game without asking"
    );
    press(&mut app, Btn::X);
    assert!(
        !root.join("Games/GBA/Emerald.gba").exists(),
        "the rom is still there"
    );
    assert!(
        !root.join("Labels/GBA/Emerald.png").exists(),
        "its label stayed"
    );
    assert!(
        !root.join("Cheats/GBA/Emerald.cht").exists(),
        "its cheats stayed"
    );
    assert!(
        root.join("Saves/GBA/Emerald.srm").is_file(),
        "its save was deleted too"
    );
    assert_eq!(
        slot_store::ini::value(root, slot_store::SELECTED_CORE_FILE, "Emerald"),
        None,
        "its core choice stayed behind"
    );
    let state = read_slot_state(root);
    assert!(!state.recents.contains(&emerald()) && !state.favorites.contains(&emerald()));
    assert!(
        app.carts().all(|c| c.stem != "Emerald"),
        "it still stands on a shelf"
    );
    assert!(
        app.core_picker().is_none(),
        "the cartridge stayed open on a game that is gone"
    );
}

#[test]
fn b_keeps_the_game() {
    let (d, mut app) = card();
    press(&mut app, Btn::Start);
    press(&mut app, Btn::Y);
    press(&mut app, Btn::B);
    press(&mut app, Btn::X);
    assert!(d.path().join("Games/GBA/Emerald.gba").is_file());
    assert!(app.carts().any(|c| c.stem == "Emerald"));
}

#[test]
fn the_last_game_of_a_folder_takes_the_empty_folder_with_it() {
    let d = common::tmp_root_with_carts(&["Zelda"]);
    let folder = d.path().join("Games/GBA/Hacks");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("Lazarus.gba"), vec![0u8; 0x100]).unwrap();
    write_slot_state(
        d.path(),
        &SlotState {
            clock_set: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut app = App::boot(d.path());
    while app.selected_stem() != Some("Hacks") {
        app.apply(Action::NextShelf);
        app.update(0.5);
    }
    press(&mut app, Btn::A);
    app.update(1.2);
    assert_eq!(app.selected_stem(), Some("Lazarus"));
    press(&mut app, Btn::Start);
    press(&mut app, Btn::Y);
    press(&mut app, Btn::X);
    assert!(!folder.exists(), "the empty folder stayed on the card");
    assert!(app.carts().all(|c| c.stem != "Lazarus"));
    assert_eq!(
        app.selected_stem(),
        Some("Zelda"),
        "the shelf kept the folder that is gone"
    );
}
