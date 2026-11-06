mod common;

use common::{boot, tmp_root_with_carts};
use slot::app::{App, Phase};
use slot_input::{Action, Btn};
use slot_ui::Draw;

fn to_lucky(a: &mut App) {
    for _ in 0..8 {
        if a.shelf_platform_name() == Some("Extras") {
            break;
        }
        a.apply(Action::NextShelf);
        a.update(0.5);
    }
    assert_eq!(a.shelf_platform_name(), Some("Extras"));
    a.apply(Action::GbaDown(Btn::Right));
    a.update(0.5);
}

#[test]
fn get_lucky_goes_from_the_dice_straight_to_the_game() {
    let d = tmp_root_with_carts(&["Emerald", "Fusion"]);
    let mut a = boot(d.path());
    to_lucky(&mut a);
    a.apply(Action::GbaDown(Btn::A));
    a.apply(Action::GbaUp(Btn::A));
    for _ in 0..120 {
        if matches!(a.phase(), Phase::Inserting { .. }) {
            break;
        }
        a.update(1.0 / 60.0);
    }
    assert!(
        matches!(a.phase(), Phase::Inserting { resumed: true, .. }),
        "the pick did not start its game"
    );
    for _ in 0..3 {
        let mut out = Vec::new();
        a.draw(&mut out);
        assert!(
            out.iter().all(|d| !matches!(d, Draw::Tex { .. })),
            "the console or its cartridge was drawn while the game started"
        );
        assert_eq!(a.screen_power(), 1.0, "the screen powered on from dark");
        a.update(1.0 / 60.0);
    }
}
