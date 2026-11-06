//! The NES and the Super Nintendo: their own shelves, their own pads, and none of the Game Boy's
//! screen settings.

mod common;

use std::path::Path;

use common::{app_playing_in, tmp_root_with_carts};
use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_store::{Cart, Platform};
use slot_ui::QuickRow;

fn app_with_platforms(carts: &[(Platform, &str)]) -> App {
    App::new(
        carts
            .iter()
            .map(|(platform, stem)| Cart {
                platform: *platform,
                stem: (*stem).to_string(),
                rom: format!(
                    "Games/{}/{stem}.{}",
                    platform.dir_name(),
                    platform.extensions()[0]
                )
                .into(),
                label: None,
                code: String::new(),
                shell: None,
                title: stem.to_uppercase(),
            })
            .collect(),
    )
}

fn write_cart(root: &Path, platform: Platform, stem: &str) {
    let dir = root.join("Games").join(platform.dir_name());
    std::fs::create_dir_all(&dir).unwrap();
    let rom = dir.join(format!("{stem}.{}", platform.extensions()[0]));
    std::fs::write(rom, vec![0u8; 0x200]).unwrap();
}

/// A card with two games of the platform, the first seated and playing.
fn playing(platform: Platform) -> (tempfile::TempDir, App) {
    let d = tmp_root_with_carts(&[]);
    write_cart(d.path(), platform, "Game");
    write_cart(d.path(), platform, "Zzz");
    let mut a = app_playing_in(d.path(), "Game");
    // The session tells the app the platform of the core it opened.
    a.set_platform(platform);
    assert!(
        matches!(a.phase(), Phase::Playing { .. }),
        "{platform:?}: the cart never seated"
    );
    assert_eq!(a.playing_platform(), platform);
    (d, a)
}

#[test]
fn the_nes_and_snes_stand_on_shelves_of_their_own() {
    let mut a = app_with_platforms(&[
        (Platform::Gba, "Emerald"),
        (Platform::Nes, "Zelda"),
        (Platform::Snes, "Super Metroid"),
    ]);
    // NES and SNES come first, the GBA last.
    assert_eq!(a.selected_stem(), Some("Zelda"));
    assert_eq!(a.shelf_platform_name(), Some("NES"));
    common::through(&mut a, Btn::R1);
    assert_eq!(a.selected_stem(), Some("Super Metroid"));
    assert_eq!(a.shelf_platform_name(), Some("Super Nintendo"));
    common::through(&mut a, Btn::R1);
    assert_eq!(a.selected_stem(), Some("Emerald"));
}

#[test]
fn a_scanned_card_shelves_nes_and_snes_games() {
    let d = tmp_root_with_carts(&["Emerald"]);
    write_cart(d.path(), Platform::Nes, "Zelda");
    write_cart(d.path(), Platform::Snes, "Super Metroid");
    common::clocked(d.path());
    let a = App::boot(d.path());
    let stems: Vec<_> = a.carts().map(|c| (c.platform, c.stem.clone())).collect();
    assert!(stems.contains(&(Platform::Nes, "Zelda".to_string())));
    assert!(stems.contains(&(Platform::Snes, "Super Metroid".to_string())));
}

#[test]
fn the_snes_shoulders_reach_the_game_and_the_nes_ones_stay_slots() {
    let (_d, snes) = playing(Platform::Snes);
    assert!(
        snes.taken_buttons().is_empty(),
        "Slot kept L1/R1 from a Super Nintendo game"
    );
    let (_d, nes) = playing(Platform::Nes);
    assert_eq!(nes.taken_buttons(), vec![Btn::L1, Btn::R1]);
}

#[test]
fn the_in_game_controls_show_only_the_playing_consoles_buttons() {
    let (_d, snes) = playing(Platform::Snes);
    let hidden = snes.in_game_hidden();
    for row in [
        QuickRow::GameX,
        QuickRow::GameY,
        QuickRow::GameL,
        QuickRow::GameR,
    ] {
        assert!(!hidden.contains(&row), "the SNES lost {row:?}");
    }
    for row in [QuickRow::GameTurboA, QuickRow::GameTurboB] {
        assert!(hidden.contains(&row), "the SNES offered {row:?}");
    }
    let (_d, nes) = playing(Platform::Nes);
    let hidden = nes.in_game_hidden();
    for row in [
        QuickRow::GameX,
        QuickRow::GameY,
        QuickRow::GameL,
        QuickRow::GameR,
    ] {
        assert!(hidden.contains(&row), "the NES offered {row:?}");
    }
}

#[test]
fn the_tv_consoles_offer_no_game_boy_screen_settings() {
    for platform in [Platform::Nes, Platform::Snes] {
        let (_d, a) = playing(platform);
        let hidden = a.in_game_hidden();
        for row in [
            QuickRow::Shader,
            QuickRow::ColourCorrection,
            QuickRow::Bezel,
            QuickRow::Palette,
        ] {
            assert!(hidden.contains(&row), "{platform:?} offered {row:?}");
        }
        assert_eq!(a.quick_value(QuickRow::Shader), None);
        assert!(!a.colour_correction());
        assert!(!a.sp_frame());
    }
}

#[test]
fn shader_and_colour_shortcuts_do_nothing_on_a_tv_console() {
    let (_d, mut a) = playing(Platform::Snes);
    let before = a.screen_shader();
    a.apply(Action::ShaderNext);
    a.apply(Action::ColourCorrectionToggle);
    assert_eq!(a.screen_shader(), before);
    assert!(!a.colour_correction());
}

#[test]
fn the_x_and_y_rows_show_on_the_shelf_only_with_snes_games() {
    let d = tmp_root_with_carts(&["Emerald", "Zzz"]);
    common::clocked(d.path());
    let a = App::boot(d.path());
    assert!(a.hidden_rows().contains(&QuickRow::GameX));
    write_cart(d.path(), Platform::Snes, "Super Metroid");
    let a = App::boot(d.path());
    assert!(!a.hidden_rows().contains(&QuickRow::GameX));
    assert!(!a.hidden_rows().contains(&QuickRow::GameY));
}

#[test]
fn the_snes_x_and_y_do_not_clash_with_the_turbo_buttons() {
    let d = tmp_root_with_carts(&["Emerald", "Zzz"]);
    write_cart(d.path(), Platform::Snes, "Super Metroid");
    common::clocked(d.path());
    let a = App::boot(d.path());
    assert_eq!(a.clash(QuickRow::GameX), None);
    assert_eq!(a.clash(QuickRow::GameTurboA), None);
}

#[test]
fn the_nes_and_snes_core_rows_hide_without_their_alternative_core() {
    let d = tmp_root_with_carts(&["Emerald", "Zzz"]);
    common::clocked(d.path());
    let a = App::boot(d.path());
    // No Nestopia or Snes9x 2005 on this card (unless a core is forced through the environment).
    if std::env::var_os("SLOT_CORE").is_none() {
        assert!(a.hidden_rows().contains(&QuickRow::NesCore));
        assert!(a.hidden_rows().contains(&QuickRow::SnesCore));
    }
}
