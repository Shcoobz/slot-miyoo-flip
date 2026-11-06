use slot_store::{cheat_file, parse_cheats, read_cheats, write_cheat_choices, Cheat, Platform};

const CHT: &str = r#"cheats = 3

cheat0_desc = "Infinite Health"
cheat0_code = "82003228+0064"
cheat0_enable = false

cheat1_desc = "Search result"
cheat1_enable = true

cheat2_desc = "Max Money"
cheat2_code = "A1B2C3D4"
cheat2_enable = true

cheat3_desc = "Past the count"
cheat3_code = "FFFF"
"#;

fn write_cht(root: &std::path::Path, stem: &str, text: &str) {
    let path = cheat_file(root, Platform::Gba, stem);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn a_cht_reads_as_its_coded_cheats_within_the_count() {
    assert_eq!(
        parse_cheats(CHT),
        vec![
            Cheat {
                index: 0,
                desc: "Infinite Health".into(),
                code: "82003228+0064".into(),
                enabled: false,
            },
            Cheat {
                index: 2,
                desc: "Max Money".into(),
                code: "A1B2C3D4".into(),
                enabled: true,
            },
        ]
    );
}

#[test]
fn a_cheat_without_a_description_is_named_by_its_number() {
    let cheats = parse_cheats("cheats = 1\ncheat0_code = \"1234\"\n");
    assert_eq!(cheats[0].desc, "Cheat 1");
}

#[test]
fn no_cht_means_no_cheats() {
    let d = tempfile::tempdir().unwrap();
    assert!(read_cheats(d.path(), Platform::Gba, "Emerald").is_empty());
}

#[test]
fn the_players_choices_outlast_the_files_defaults() {
    let d = tempfile::tempdir().unwrap();
    write_cht(d.path(), "Emerald", CHT);
    let mut cheats = read_cheats(d.path(), Platform::Gba, "Emerald");
    assert_eq!(
        cheats.iter().map(|c| c.enabled).collect::<Vec<_>>(),
        [false, true]
    );
    cheats[0].enabled = true;
    cheats[1].enabled = false;
    write_cheat_choices(d.path(), Platform::Gba, "Emerald", &cheats).unwrap();
    let again = read_cheats(d.path(), Platform::Gba, "Emerald");
    assert_eq!(
        again.iter().map(|c| c.enabled).collect::<Vec<_>>(),
        [true, false]
    );
    assert_eq!(
        std::fs::read_to_string(d.path().join("Config/cheats/GBA/Emerald.txt")).unwrap(),
        "0\n"
    );
}

#[test]
fn choosing_none_keeps_them_all_off() {
    let d = tempfile::tempdir().unwrap();
    write_cht(d.path(), "Emerald", CHT);
    let mut cheats = read_cheats(d.path(), Platform::Gba, "Emerald");
    for c in &mut cheats {
        c.enabled = false;
    }
    write_cheat_choices(d.path(), Platform::Gba, "Emerald", &cheats).unwrap();
    assert!(read_cheats(d.path(), Platform::Gba, "Emerald")
        .iter()
        .all(|c| !c.enabled));
}
