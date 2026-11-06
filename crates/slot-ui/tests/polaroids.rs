use std::path::Path;

use slot_power::{Battery, Charge};
use slot_store::StateEntry;
use slot_ui::{
    photo_face, Draw, PhotoFace, Polaroids, Printed, TexId, OUT_W, PHOTO_H, PHOTO_W, PLATE_H,
};

fn entry(dir: &Path, stamp: &str) -> StateEntry {
    StateEntry {
        stamp: stamp.to_string(),
        state: dir.join(format!("{stamp}.state")),
        thumb: dir.join(format!("{stamp}.png")),
    }
}

fn switcher() -> Polaroids {
    Polaroids::new(Vec::new())
}

fn write_png(path: &Path, w: u32, h: u32, rgb: [u8; 3]) {
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().expect("png header");
    let pixels: Vec<u8> = std::iter::repeat_n(rgb, (w * h) as usize)
        .flatten()
        .collect();
    writer.write_image_data(&pixels).expect("png data");
}

fn pixel(face: &PhotoFace, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * face.w + x) * 4) as usize;
    [face.rgba[i], face.rgba[i + 1], face.rgba[i + 2]]
}

#[test]
fn relative_time_reads_off_the_stamp() {
    let now = "2026-08-09_14-36-05";
    assert_eq!(
        Polaroids::relative_time("2026-08-09_14-32-05", now),
        "4 min ago"
    );
    assert_eq!(
        Polaroids::relative_time("2026-08-09_13-36-05", now),
        "1 hr ago"
    );
    assert_eq!(
        Polaroids::relative_time("2026-08-09_11-36-05", now),
        "3 hr ago"
    );
}

#[test]
fn an_hour_before_midnight_is_not_yesterday() {
    assert_eq!(
        Polaroids::relative_time("2026-08-08_23-30-00", "2026-08-09_01-00-00"),
        "1 hr ago"
    );
}

#[test]
fn a_stamp_ahead_of_the_clock_does_not_read_as_a_lifetime_ago() {
    assert_eq!(
        Polaroids::relative_time("2026-08-09_15-00-00", "2026-08-09_14-36-05"),
        "just now"
    );
}

#[test]
fn an_entry_whose_thumbnail_is_missing_still_has_a_face() {
    let face = photo_face(&entry(Path::new("/nonexistent"), "2026-08-09_14-32-05"));
    assert_eq!((face.w, face.h), (PHOTO_W, PHOTO_H));
    assert!(
        face.rgba.chunks_exact(4).all(|p| p[3] == 255),
        "the blank photo is see through"
    );
}

#[test]
fn the_photo_fills_the_face_and_keeps_its_colours() {
    let d = tempfile::tempdir().expect("tempdir");
    let e = entry(d.path(), "2026-08-09_14-32-05");
    write_png(&e.thumb, PHOTO_W, PHOTO_H, [0xd0, 0x40, 0x20]);
    let face = photo_face(&e);
    for (x, y) in [
        (0, 0),
        (PHOTO_W / 2, PHOTO_H / 2),
        (PHOTO_W - 1, PHOTO_H - 1),
    ] {
        assert_eq!(pixel(&face, x, y), [0xd0, 0x40, 0x20], "at {x},{y}");
    }
}

#[test]
fn the_index_clamps_at_both_ends() {
    let d = Path::new("/nonexistent");
    let mut p = Polaroids::new(vec![
        entry(d, "2026-08-09_00-00-02"),
        entry(d, "2026-08-09_00-00-01"),
    ]);
    p.left();
    assert_eq!(p.index, 0);
    p.right();
    p.right();
    p.right();
    assert_eq!(p.index, 1);
    assert_eq!(
        p.selected().map(|e| e.stamp.as_str()),
        Some("2026-08-09_00-00-01")
    );
}

#[test]
fn the_gauge_stays_clear_of_the_title_on_the_left() {
    let p = switcher();
    let mut out = Vec::new();
    p.draw(
        Some(Battery {
            percent: 68,
            charge: Charge::Charging,
        }),
        Printed { face: None, w: 30 },
        Some(TexId::from_raw(7)),
        Printed::default(),
        &mut out,
    );
    let gauge: Vec<_> = out
        .iter()
        .filter_map(|d| match *d {
            Draw::Rect { x, y, w, .. } | Draw::Tex { x, y, w, .. }
                if y < PLATE_H && w < OUT_W as f32 =>
            {
                Some((x, w))
            }
            _ => None,
        })
        .collect();
    assert!(!gauge.is_empty(), "no gauge drew at the left margin at all");
    let leftmost = gauge.iter().map(|(x, _)| *x).fold(f32::MAX, f32::min);
    assert_eq!(leftmost, 16.0, "the plate margin is 16");
    let rightmost = gauge.iter().map(|(x, w)| x + w).fold(0.0, f32::max);
    assert!(
        rightmost < 180.0,
        "the gauge reached into the title's texture"
    );
}

#[test]
fn the_clock_stays_clear_of_the_title_on_the_right() {
    let p = switcher();
    let mut out = Vec::new();
    p.draw(
        None,
        Printed::default(),
        None,
        Printed { face: None, w: 40 },
        &mut out,
    );
    let clock: Vec<_> = out
        .iter()
        .filter_map(|d| match *d {
            Draw::Rect { x, y, w, .. } | Draw::Tex { x, y, w, .. } if y < PLATE_H && x > 540.0 => {
                Some((x, w))
            }
            _ => None,
        })
        .collect();
    assert!(!clock.is_empty(), "no clock drew past the title at all");
    let leftmost = clock.iter().map(|(x, _)| *x).fold(f32::MAX, f32::min);
    assert!(
        leftmost > 540.0,
        "the clock reached into the title's texture"
    );
    let rightmost = clock.iter().map(|(x, w)| x + w).fold(0.0, f32::max);
    assert_eq!(rightmost, OUT_W as f32 - 16.0, "the plate margin is 16");
}

#[test]
fn a_switcher_with_no_gauge_still_shows_its_clock() {
    let p = switcher();
    let mut out = Vec::new();
    p.draw(
        None,
        Printed::default(),
        None,
        Printed { face: None, w: 40 },
        &mut out,
    );
    assert!(
        out.iter()
            .any(|d| matches!(d, Draw::Rect { y, w, .. } if *y < PLATE_H && *w == 40.0)),
        "the clock's placeholder did not draw when there was no gauge"
    );
}

#[test]
fn nothing_on_the_plate_moves_when_the_charge_state_changes() {
    let p = switcher();
    let percent = Printed { face: None, w: 30 };
    let clock = Printed { face: None, w: 40 };
    let mut idle = Vec::new();
    p.draw(
        Some(Battery {
            percent: 68,
            charge: Charge::Discharging,
        }),
        percent,
        None,
        clock,
        &mut idle,
    );
    let mut charging = Vec::new();
    p.draw(
        Some(Battery {
            percent: 68,
            charge: Charge::Charging,
        }),
        percent,
        Some(TexId::from_raw(7)),
        clock,
        &mut charging,
    );
    for q in &idle {
        assert!(
            charging.contains(q),
            "{q:?} moved or vanished when charging started"
        );
    }
}

#[test]
fn a_game_boy_print_is_its_own_picture_at_ten_by_nine_never_resampled() {
    let d = tempfile::tempdir().unwrap();
    let e = entry(d.path(), "2026-10-22_10-00-00");
    // A Game Boy frame as the core leaves it: 160x144 in black, at (40, 8) of 240x160, with a
    // one-pixel checker so any resampling would grey it.
    let file = std::fs::File::create(&e.thumb).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), PHOTO_W, PHOTO_H);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut pixels = vec![0u8; (PHOTO_W * PHOTO_H * 3) as usize];
    for y in 8..152 {
        for x in 40..200 {
            let v = if (x + y) % 2 == 0 { 255 } else { 40 };
            let at = ((y * PHOTO_W + x) * 3) as usize;
            pixels[at..at + 3].copy_from_slice(&[v, v, v]);
        }
    }
    enc.write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();

    let face = photo_face(&e);
    assert!(face.game_boy);
    assert_eq!((face.w, face.h), (PHOTO_W, PHOTO_H));
    assert_eq!(
        pixel(&face, 40, 8),
        [255, 255, 255],
        "the picture was resampled"
    );
    assert_eq!(
        pixel(&face, 41, 8),
        [40, 40, 40],
        "the picture was resampled"
    );

    let mut p = Polaroids::games(vec![e]);
    p.set_faces(vec![TexId::from_raw(7)]);
    p.set_game_boy_faces(vec![true]);
    let mut out = Vec::new();
    p.draw_print(slot_ui::Canvas::of((640, 480)), &mut out);
    let Some(Draw::Part { w, h, uv, .. }) = out.first() else {
        panic!("the Game Boy part was not cut out: {out:?}");
    };
    assert!((w / h - 160.0 / 144.0).abs() < 0.01, "not 10:9: {w}x{h}");
    assert_eq!(
        *uv,
        [40.0 / 240.0, 8.0 / 160.0, 160.0 / 240.0, 144.0 / 160.0]
    );
}
