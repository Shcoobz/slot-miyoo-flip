use slot_store::{pick, read_suggested, write_suggested, Platform, Recent};

fn game(stem: &str) -> Recent {
    Recent {
        platform: Platform::Gba,
        stem: stem.into(),
    }
}

#[test]
fn every_game_comes_up_once_before_any_comes_up_again() {
    let pool: Vec<Recent> = ["A", "B", "C", "D"].map(game).into();
    let mut suggested = Vec::new();
    let mut first: Vec<Recent> = (0..4)
        .map(|n| pick(&pool, &mut suggested, n * 7919).unwrap())
        .collect();
    first.sort_by(|a, b| a.stem.cmp(&b.stem));
    assert_eq!(first, pool, "a game came up twice before the rest had");
    let last = suggested.last().cloned();
    let again = pick(&pool, &mut suggested, 3).unwrap();
    assert_ne!(
        Some(again),
        last,
        "the pool restarted on the game it just offered"
    );
}

#[test]
fn the_deck_is_shared_but_a_smaller_pool_only_resets_itself() {
    let all: Vec<Recent> = ["A", "B", "C"].map(game).into();
    let unplayed: Vec<Recent> = ["C"].map(game).into();
    let mut suggested = vec![game("A"), game("C")];
    assert_eq!(
        pick(&all, &mut suggested, 0),
        Some(game("B")),
        "the shared deck was not honoured"
    );
    assert_eq!(pick(&unplayed, &mut suggested, 0), Some(game("C")));
    assert!(
        suggested.contains(&game("A")) && suggested.contains(&game("B")),
        "resetting the unplayed pool emptied the whole deck: {suggested:?}"
    );
    assert_eq!(pick(&[], &mut suggested, 0), None);
}

#[test]
fn the_deck_survives_a_restart() {
    let d = tempfile::tempdir().unwrap();
    let deck = vec![game("Golden Sun"), game("Pokemon Emerald")];
    write_suggested(d.path(), &deck).unwrap();
    assert_eq!(read_suggested(d.path()), deck);
}
