use slot_gfx::{tv_dots, tv_rect, TvFit};

const FLIP: (u32, u32) = (640, 480);
const CANVAS: (u32, u32) = (720, 480);

#[test]
fn a_tv_picture_fills_the_flip_panel_at_4_3() {
    for frame in [(256, 240), (256, 224), (512, 224), (512, 448)] {
        assert_eq!(tv_rect(FLIP, frame, TvFit::Tv), (0, 0, 640, 480));
    }
}

#[test]
fn a_tv_picture_is_4_3_in_the_middle_of_the_720_canvas() {
    assert_eq!(tv_rect(CANVAS, (256, 224), TvFit::Tv), (40, 0, 640, 480));
}

#[test]
fn a_sharp_nes_picture_is_2x_and_fills_the_height() {
    assert_eq!(tv_rect(FLIP, (256, 240), TvFit::Sharp), (64, 0, 512, 480));
    assert_eq!(
        tv_rect(CANVAS, (256, 240), TvFit::Sharp),
        (104, 0, 512, 480)
    );
}

#[test]
fn a_sharp_snes_picture_is_2x_with_black_bands() {
    assert_eq!(tv_rect(FLIP, (256, 224), TvFit::Sharp), (64, 16, 512, 448));
    assert_eq!(
        tv_rect(CANVAS, (256, 224), TvFit::Sharp),
        (104, 16, 512, 448)
    );
}

#[test]
fn a_snes_hi_res_frame_keeps_the_same_place_as_its_low_res_one() {
    let lo = tv_rect(FLIP, (256, 224), TvFit::Sharp);
    assert_eq!(tv_rect(FLIP, (512, 224), TvFit::Sharp), lo);
    assert_eq!(tv_rect(FLIP, (512, 448), TvFit::Sharp), lo);
    assert_eq!(tv_dots((512, 448)), (256, 224));
    assert_eq!(tv_dots((256, 240)), (256, 240));
}

#[test]
fn a_nes_picture_cropped_at_the_sides_still_scales_by_2() {
    assert_eq!(tv_rect(FLIP, (240, 224), TvFit::Sharp), (80, 16, 480, 448));
}

#[test]
fn a_frame_bigger_than_the_area_is_held_inside_it() {
    let (x, y, w, h) = tv_rect((200, 150), (256, 240), TvFit::Sharp);
    assert!(x >= 0 && y >= 0 && w <= 200 && h <= 150);
    assert_eq!(tv_dots((0, 0)), (1, 1));
}

#[cfg(target_os = "macos")]
mod drawn {
    use slot_gfx::{Compositor, Draw, HeadlessSurface, ScreenEffect, TvFit};
    use std::sync::{Mutex, MutexGuard, PoisonError};

    static GL: Mutex<()> = Mutex::new(());

    fn compositor() -> Option<(MutexGuard<'static, ()>, HeadlessSurface, Compositor)> {
        let guard = GL.lock().unwrap_or_else(PoisonError::into_inner);
        let surface = HeadlessSurface::new().ok()?;
        let compositor = Compositor::new(&surface).ok()?;
        Some((guard, surface, compositor))
    }

    fn white(w: u32, h: u32) -> Vec<u8> {
        vec![0xff; (w * h * 4) as usize]
    }

    fn draw_tv(c: &mut Compositor, panel: (u32, u32), frame: (u32, u32), fit: TvFit) -> Vec<u8> {
        c.fit(panel);
        c.set_screen_effect(ScreenEffect::None);
        c.set_screen_power(1.0);
        c.set_tv_game(Some(fit));
        c.begin_frame();
        c.upload_game_sized(&white(frame.0, frame.1), frame.0, frame.1);
        c.draw_list(&[Draw::Game]);
        c.read_frame()
    }

    fn lit(px: &[u8], w: usize, x: usize, y: usize) -> bool {
        let o = (y * w + x) * 4;
        px[o..o + 3].iter().all(|c| *c > 200)
    }

    #[test]
    fn a_stretched_snes_picture_covers_the_whole_flip_panel() {
        let Some((_g, _s, mut c)) = compositor() else {
            return;
        };
        let px = draw_tv(&mut c, (640, 480), (256, 224), TvFit::Tv);
        for (x, y) in [(0, 0), (639, 0), (0, 479), (639, 479), (320, 240)] {
            assert!(lit(&px, 640, x, y), "({x}, {y}) is not picture");
        }
    }

    #[test]
    fn a_sharp_snes_picture_is_512x448_in_the_middle_of_the_flip_panel() {
        let Some((_g, _s, mut c)) = compositor() else {
            return;
        };
        let px = draw_tv(&mut c, (640, 480), (256, 224), TvFit::Sharp);
        assert_eq!(c.game_size(), (256, 224));
        assert!(lit(&px, 640, 64, 16) && lit(&px, 640, 575, 463));
        assert!(!lit(&px, 640, 63, 240), "the left band is not black");
        assert!(!lit(&px, 640, 576, 240), "the right band is not black");
        assert!(!lit(&px, 640, 320, 15), "the top band is not black");
        assert!(!lit(&px, 640, 320, 464), "the bottom band is not black");
    }

    #[test]
    fn a_sharp_nes_picture_is_512x480_in_the_middle_of_the_canvas() {
        let Some((_g, _s, mut c)) = compositor() else {
            return;
        };
        let px = draw_tv(&mut c, (720, 480), (256, 240), TvFit::Sharp);
        assert!(lit(&px, 720, 104, 0) && lit(&px, 720, 615, 479));
        assert!(!lit(&px, 720, 103, 240) && !lit(&px, 720, 616, 240));
    }

    #[test]
    fn a_handheld_game_after_a_tv_one_is_back_in_the_gba_screen() {
        let Some((_g, _s, mut c)) = compositor() else {
            return;
        };
        draw_tv(&mut c, (640, 480), (512, 448), TvFit::Tv);
        c.set_tv_game(None);
        c.set_sp_frame(true);
        c.begin_frame();
        c.upload_game(&white(240, 160));
        assert_eq!(c.game_size(), (240, 160));
        c.draw_list(&[Draw::Game]);
        let px = c.read_frame();
        assert!(lit(&px, 640, 0, 426) && !lit(&px, 640, 0, 427));
    }
}
