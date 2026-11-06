use slot::video_mode::{
    picture_uv, source_rect, tv_fit, video_mode_for_platform, write_video_mode, VideoMode,
};
use slot_gfx::{TvFit, WHOLE_TEXTURE};
use slot_store::Platform;

#[test]
fn a_tv_console_fills_the_screen_at_4_3_until_told_otherwise() {
    for p in [Platform::Nes, Platform::Snes] {
        assert_eq!(VideoMode::default_for(p), VideoMode::Stretch);
        assert_eq!(tv_fit(p, VideoMode::Stretch), Some(TvFit::Tv));
        assert_eq!(tv_fit(p, VideoMode::Actual), Some(TvFit::Sharp));
    }
}

#[test]
fn a_handheld_keeps_the_gba_screen_and_its_own_default() {
    for p in [Platform::Gba, Platform::Gb, Platform::Gbc] {
        assert_eq!(VideoMode::default_for(p), VideoMode::Actual);
        assert_eq!(tv_fit(p, VideoMode::Actual), None);
        assert_eq!(tv_fit(p, VideoMode::Stretch), None);
    }
}

#[test]
fn a_tv_consoles_whole_frame_is_picture() {
    for p in [Platform::Nes, Platform::Snes] {
        for mode in [VideoMode::Actual, VideoMode::Stretch] {
            assert_eq!(source_rect(p, mode), WHOLE_TEXTURE);
        }
        assert_eq!(picture_uv(p), [0.0, 0.0, 1.0, 1.0]);
    }
}

#[test]
fn the_game_boy_picture_is_still_its_160x144_in_the_gba_frame() {
    let [x0, y0, x1, y1] = picture_uv(Platform::Gb);
    let px = [x0 * 240.0, y0 * 160.0, x1 * 240.0, y1 * 160.0];
    for (got, want) in px.iter().zip([40.0, 8.0, 200.0, 152.0]) {
        assert!((got - want).abs() < 1e-3, "{px:?}");
    }
    assert_eq!(picture_uv(Platform::Gba), [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(
        source_rect(Platform::Gb, VideoMode::Stretch),
        [40.0 / 240.0, 0.0, 160.0 / 240.0, 1.0]
    );
    assert_eq!(
        source_rect(Platform::Gba, VideoMode::Stretch),
        WHOLE_TEXTURE
    );
}

#[test]
fn a_chosen_mode_wins_over_the_platforms_default() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(
        video_mode_for_platform(d.path(), "Zelda", Platform::Nes),
        VideoMode::Stretch
    );
    assert_eq!(
        video_mode_for_platform(d.path(), "Zelda", Platform::Gb),
        VideoMode::Actual
    );
    write_video_mode(d.path(), "Zelda", Platform::Nes, VideoMode::Actual).unwrap();
    assert_eq!(
        video_mode_for_platform(d.path(), "Zelda", Platform::Nes),
        VideoMode::Actual
    );
}

#[test]
fn a_game_boy_game_of_the_same_name_keeps_its_own_picture() {
    let d = tempfile::tempdir().unwrap();
    write_video_mode(
        d.path(),
        "Donkey Kong Country",
        Platform::Gbc,
        VideoMode::Actual,
    )
    .unwrap();
    assert_eq!(
        video_mode_for_platform(d.path(), "Donkey Kong Country", Platform::Snes),
        VideoMode::Stretch,
        "the Game Boy Color game's setting reached the Super Nintendo one"
    );
    assert_eq!(
        video_mode_for_platform(d.path(), "Donkey Kong Country", Platform::Gbc),
        VideoMode::Actual
    );
}
