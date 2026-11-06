use slot_store::{
    core_for, core_for_platform, read_selected_cores, write_selected_core, Core, Platform,
};
use tempfile::tempdir;

fn root_with(ini: Option<&str>) -> tempfile::TempDir {
    let d = tempdir().unwrap();
    std::fs::create_dir(d.path().join("Config")).unwrap();
    if let Some(text) = ini {
        std::fs::write(d.path().join("Config/selected_core.txt"), text).unwrap();
    }
    d
}

#[test]
fn absent_file_means_everything_defaults() {
    let d = root_with(None);
    assert!(read_selected_cores(d.path()).is_empty());
    assert_eq!(core_for(d.path(), "Emerald"), Core::Mgba);
}

#[test]
fn a_listed_stem_gets_its_core() {
    let d = root_with(Some("Pokemon - Emerald Version (USA, Europe) = gpsp\n"));
    assert_eq!(
        core_for(d.path(), "Pokemon - Emerald Version (USA, Europe)"),
        Core::Gpsp
    );
}

#[test]
fn an_unlisted_stem_defaults_to_mgba() {
    let d = root_with(Some("Emerald = gpsp\n"));
    assert_eq!(core_for(d.path(), "Metroid Fusion"), Core::Mgba);
}

#[test]
fn malformed_lines_are_ignored_rather_than_fatal() {
    let d = root_with(Some(concat!(
        "\n",
        "# a comment\n",
        "; another comment\n",
        "[cores]\n",
        "no equals sign here\n",
        "Emerald = notacore\n",
        "  Spaced Out   =   gpsp  \n",
        "= gpsp\n",
        "Trailing =\n",
    )));
    let map = read_selected_cores(d.path());
    assert_eq!(
        map.get("Spaced Out"),
        Some(&Core::Gpsp),
        "whitespace not trimmed"
    );
    assert_eq!(
        map.get("Emerald"),
        None,
        "an unknown core name must not be stored"
    );
    assert_eq!(core_for(d.path(), "Emerald"), Core::Mgba);
    assert_eq!(map.len(), 1, "only the one good line should survive");
}

#[test]
fn a_later_duplicate_wins() {
    let d = root_with(Some("Emerald = mgba\nEmerald = gpsp\n"));
    assert_eq!(core_for(d.path(), "Emerald"), Core::Gpsp);
}

#[test]
fn the_last_line_for_a_cart_wins_even_when_it_is_the_typo() {
    let d = root_with(Some("Emerald = gpsp\nEmerald = notacore\n"));
    assert_eq!(
        core_for(d.path(), "Emerald"),
        Core::Mgba,
        "a line the later one superseded is still in force"
    );
    assert_eq!(
        read_selected_cores(d.path()).get("Emerald"),
        None,
        "the map and core_for read the same two lines differently"
    );

    let d = root_with(Some("Emerald = notacore\nEmerald = gpsp\n"));
    assert_eq!(core_for(d.path(), "Emerald"), Core::Gpsp);
    assert_eq!(
        read_selected_cores(d.path()).get("Emerald"),
        Some(&Core::Gpsp)
    );
}

#[test]
fn core_names_round_trip() {
    for c in Core::ALL {
        assert_eq!(Core::parse(c.as_str()), Some(c));
    }
    assert_eq!(
        Core::parse("MGBA"),
        Some(Core::Mgba),
        "case is not the user's problem"
    );
    assert_eq!(Core::parse("nonsense"), None);
    assert_eq!(Core::default(), Core::Mgba);
}

#[test]
fn writing_a_core_creates_the_file_when_absent() {
    let d = root_with(None);
    slot_store::write_selected_core(d.path(), "Emerald", Core::Gpsp).unwrap();
    assert_eq!(core_for(d.path(), "Emerald"), Core::Gpsp);
}

#[test]
fn writing_a_core_replaces_that_carts_line_and_leaves_the_rest_alone() {
    let d = root_with(Some(concat!(
        "# my notes\n",
        "\n",
        "Emerald = mgba\n",
        "Metroid Fusion = gpsp\n",
    )));
    slot_store::write_selected_core(d.path(), "Emerald", Core::Gpsp).unwrap();

    let text = std::fs::read_to_string(d.path().join("Config/selected_core.txt")).unwrap();
    assert!(
        text.contains("# my notes"),
        "a hand-written comment was destroyed"
    );
    assert!(
        text.contains("Metroid Fusion = gpsp"),
        "another cart's entry was lost"
    );
    assert_eq!(core_for(d.path(), "Emerald"), Core::Gpsp);
    assert_eq!(core_for(d.path(), "Metroid Fusion"), Core::Gpsp);
    assert_eq!(
        text.matches("Emerald").count(),
        1,
        "the old line was left behind"
    );
}

#[test]
fn writing_a_core_appends_a_cart_the_file_has_never_seen() {
    let d = root_with(Some("Emerald = gpsp\n"));
    slot_store::write_selected_core(d.path(), "Drill Dozer", Core::Gpsp).unwrap();
    assert_eq!(core_for(d.path(), "Emerald"), Core::Gpsp);
    assert_eq!(core_for(d.path(), "Drill Dozer"), Core::Gpsp);
}

#[test]
fn writing_the_default_still_records_it() {
    let d = root_with(Some("Emerald = gpsp\n"));
    slot_store::write_selected_core(d.path(), "Emerald", Core::Mgba).unwrap();
    assert_eq!(core_for(d.path(), "Emerald"), Core::Mgba);
    let text = std::fs::read_to_string(d.path().join("Config/selected_core.txt")).unwrap();
    assert!(text.contains("Emerald = mgba"));
}

#[test]
fn every_platform_defaults_to_mgba() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(
        core_for_platform(d.path(), "Tetris", Platform::Gb),
        Core::Mgba
    );
    assert_eq!(
        core_for_platform(d.path(), "Tetris Chromatic", Platform::Gbc),
        Core::Mgba
    );
    assert_eq!(
        core_for_platform(d.path(), "Emerald", Platform::Gba),
        Core::Mgba
    );
}

#[test]
fn a_cart_can_ask_for_a_non_default_core_by_hand() {
    let d = tempfile::tempdir().unwrap();
    write_selected_core(d.path(), "Emerald", Core::Gpsp).unwrap();
    assert_ne!(Core::default_for(Platform::Gba), Core::Gpsp);
    assert_eq!(
        core_for_platform(d.path(), "Emerald", Platform::Gba),
        Core::Gpsp
    );
}

#[test]
fn a_line_naming_a_core_the_platform_cannot_run_is_dropped() {
    let d = tempfile::tempdir().unwrap();
    write_selected_core(d.path(), "Tetris", Core::Gpsp).unwrap();
    assert_eq!(
        core_for_platform(d.path(), "Tetris", Platform::Gb),
        Core::Mgba
    );
    assert_eq!(
        core_for_platform(d.path(), "Tetris", Platform::Gbc),
        Core::Mgba
    );
}

#[test]
fn the_picker_board_has_exactly_two_sockets() {
    for platform in [Platform::Gba, Platform::Gb, Platform::Gbc] {
        let pair = Core::pair(platform);
        assert!(pair.iter().all(|c| c.runs(platform)), "{platform:?}");
        assert_ne!(pair[0], pair[1]);
    }
}

#[test]
fn a_platform_default_runs_carts_without_a_pick_of_their_own() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("Config")).unwrap();
    std::fs::write(
        d.path().join(slot_store::SELECTED_CORE_FILE),
        "*GBA=gpsp\nKept=mgba\n",
    )
    .unwrap();
    use slot_store::{core_for_platform, Core, Platform};
    assert_eq!(
        core_for_platform(d.path(), "Any", Platform::Gba),
        Core::Gpsp
    );
    assert_eq!(
        core_for_platform(d.path(), "Kept", Platform::Gba),
        Core::Mgba
    );
    assert_eq!(
        core_for_platform(d.path(), "Any", Platform::Gb),
        Core::Mgba,
        "gpSP cannot run it"
    );
}

#[test]
fn a_folder_core_sits_between_the_games_own_and_the_platforms() {
    let d = tempfile::tempdir().unwrap();
    let folder = d.path().join("Games/GBA/Pokemon Hacks");
    std::fs::create_dir_all(&folder).unwrap();
    let cart = slot_store::Cart {
        platform: slot_store::Platform::Gba,
        stem: "Lazarus".into(),
        rom: folder.join("Lazarus.gba"),
        label: None,
        title: String::new(),
        code: String::new(),
        shell: None,
    };
    let key = slot_store::folder_key(slot_store::Platform::Gba, "Pokemon Hacks");
    slot_store::write_default_core(d.path(), slot_store::Platform::Gba, slot_store::Core::Gpsp)
        .unwrap();
    assert_eq!(
        slot_store::core_for_cart(d.path(), &cart),
        slot_store::Core::Gpsp
    );
    slot_store::write_folder_core(d.path(), &key, Some(slot_store::Core::Mgba)).unwrap();
    assert_eq!(
        slot_store::core_for_cart(d.path(), &cart),
        slot_store::Core::Mgba,
        "the folder's core lost to the platform's"
    );
    slot_store::write_selected_core(d.path(), "Lazarus", slot_store::Core::Gpsp).unwrap();
    assert_eq!(
        slot_store::core_for_cart(d.path(), &cart),
        slot_store::Core::Gpsp,
        "the game's own pick lost to its folder's"
    );
    slot_store::write_folder_core(d.path(), &key, None).unwrap();
    assert_eq!(
        slot_store::folder_core(d.path(), &key, slot_store::Platform::Gba),
        None
    );
}

#[test]
fn folder_settings_survive_a_name_with_spaces_and_move_with_a_rename() {
    let d = tempfile::tempdir().unwrap();
    let (from, to) = ("GBA/Pokemon Hacks", "GBA/Hacks & Mods");
    let prefs = slot_store::FolderPrefs {
        cover: Some("Lazarus".into()),
        sort: slot_store::FolderSort::Recent,
        open_last: true,
        last: Some("Unbound".into()),
        paper: slot_store::FolderPaper::Deep,
    };
    slot_store::write_folder_prefs(d.path(), from, &prefs).unwrap();
    assert_eq!(slot_store::read_folder_prefs(d.path(), from), prefs);
    slot_store::write_folder_core(d.path(), from, Some(slot_store::Core::Mgba)).unwrap();
    slot_store::rename_folder_prefs(d.path(), from, to, slot_store::Platform::Gba).unwrap();
    assert_eq!(slot_store::read_folder_prefs(d.path(), to), prefs);
    assert_eq!(
        slot_store::read_folder_prefs(d.path(), from),
        slot_store::FolderPrefs::default()
    );
    assert_eq!(
        slot_store::folder_core(d.path(), to, slot_store::Platform::Gba),
        Some(slot_store::Core::Mgba)
    );
}
