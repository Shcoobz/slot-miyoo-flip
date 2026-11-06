//! The NES and SNES (EU) carts of `NES SNES Carts.dc.html`, turn 2: their faces, outlines, labels,
//! shelf boxes, seats in the slot, open views and folder stacks.

use slot_store::{Cart, Platform};
use slot_ui::{
    board_face, cart_box, cart_face, label_size, mould_of, mould_silhouette, open_layout,
    picker_interior, seated_box, shelf_box, shell_for, stack_poses, Canvas, Mould, Placed,
    GBA_SHELF_ZOOM, NES_SHELL, SNES_SHELL,
};

fn cart(platform: Platform, stem: &str) -> Cart {
    let ext = platform.extensions()[0];
    Cart {
        platform,
        stem: stem.into(),
        rom: format!("Games/{}/{stem}.{ext}", platform.dir_name()).into(),
        label: None,
        code: String::new(),
        shell: None,
        title: stem.to_uppercase(),
    }
}

fn alpha(rgba: &[u8], w: u32, x: u32, y: u32) -> u8 {
    rgba[((y * w + x) * 4 + 3) as usize]
}

#[test]
fn the_faces_are_the_designs_size() {
    assert_eq!(cart_box(Platform::Nes), (290, 340));
    assert_eq!(cart_box(Platform::Snes), (360, 240));
    let nes = cart_face(&cart(Platform::Nes, "Super Mario Bros. 3 (Europe)"));
    assert_eq!((nes.w, nes.h), (290, 340));
    let snes = cart_face(&cart(Platform::Snes, "Donkey Kong Country (Europe)"));
    assert_eq!((snes.w, snes.h), (360, 240));
}

#[test]
fn the_labels_are_the_designs_wells() {
    assert_eq!(label_size(&cart(Platform::Nes, "Zelda")), (122, 204));
    assert_eq!(label_size(&cart(Platform::Snes, "Zelda")), (296, 98));
}

#[test]
fn the_moulds_follow_the_platform() {
    assert_eq!(mould_of(&cart(Platform::Nes, "A")), Mould::Nes);
    assert_eq!(mould_of(&cart(Platform::Snes, "A")), Mould::Snes);
    assert_eq!(slot_ui::gb_shell_of(&cart(Platform::Nes, "A")), None);
    assert_eq!(slot_ui::gb_shell_of(&cart(Platform::Snes, "A")), None);
}

#[test]
fn the_nes_outline_has_its_notched_top_corners() {
    let m = mould_silhouette(Mould::Nes);
    assert_eq!(m.len(), 290 * 340);
    // 4% of 340 down and 3% of 290 in: the notch is open, the shoulder below it plastic.
    assert_eq!(m[(4 * 290 + 3) as usize], 0, "the top left notch is filled");
    assert_eq!(
        m[(4 * 290 + 286) as usize],
        0,
        "the top right notch is filled"
    );
    assert_eq!(
        m[(30 * 290 + 3) as usize],
        255,
        "the shoulder below the notch is missing"
    );
    assert_eq!(m[(170 * 290 + 145) as usize], 255);
}

#[test]
fn the_snes_outline_has_rounded_corners() {
    let m = mould_silhouette(Mould::Snes);
    assert_eq!(m.len(), 360 * 240);
    assert_eq!(m[0], 0, "the top left corner is square");
    assert_eq!(m[359], 0, "the top right corner is square");
    assert_eq!(m[(120 * 360 + 180) as usize], 255);
}

#[test]
fn the_faces_are_clipped_to_their_outlines() {
    let nes = cart_face(&cart(Platform::Nes, "Metroid"));
    assert_eq!(alpha(&nes.rgba, nes.w, 2, 2), 0);
    assert_eq!(
        alpha(&nes.rgba, nes.w, 180, 100),
        255,
        "the label is not opaque"
    );
    let snes = cart_face(&cart(Platform::Snes, "Metroid"));
    assert_eq!(alpha(&snes.rgba, snes.w, 0, 0), 0);
    assert_eq!(
        alpha(&snes.rgba, snes.w, 180, 60),
        255,
        "the label is not opaque"
    );
}

#[test]
fn the_default_shells_are_the_designs_greys() {
    assert_eq!(shell_for(&cart(Platform::Nes, "A")), NES_SHELL);
    assert_eq!(shell_for(&cart(Platform::Snes, "A")), SNES_SHELL);
    let luma = |c: [u8; 3]| c[0] as u32 + c[1] as u32 + c[2] as u32;
    assert!(
        luma(NES_SHELL.colour) < luma(SNES_SHELL.colour),
        "the NES is the darker grey"
    );
    // The plain shoulder of the NES front, right of the grip strip, comes back near the design's
    // #8e8e92 gradient.
    let nes = cart_face(&cart(Platform::Nes, "A"));
    let i = ((200 * nes.w + 270) * 4) as usize;
    for c in 0..3 {
        let got = nes.rgba[i + c] as i32;
        assert!(
            (got - 0x8a).abs() < 16,
            "channel {c} of the NES shoulder is {got:#x}"
        );
    }
}

#[test]
fn nes_stands_like_a_pak_and_snes_like_a_gba_cart() {
    assert_eq!(shelf_box(Platform::Nes), (290.0, 340.0));
    assert_eq!(
        shelf_box(Platform::Snes),
        (360.0 * GBA_SHELF_ZOOM, 240.0 * GBA_SHELF_ZOOM)
    );
    assert_eq!(shelf_box(Platform::Snes).0, shelf_box(Platform::Gba).0);
}

#[test]
fn every_cart_seats_at_the_mouths_width_and_its_own_aspect() {
    for p in Platform::ALL {
        let (sw, sh) = seated_box(p);
        let (w, h) = cart_box(p);
        assert_eq!(sw, 240, "{p:?}");
        let want = 240.0 * h as f32 / w as f32;
        assert!(
            (sh as f32 - want).abs() <= 1.0,
            "{p:?} seats {sh} tall, not {want}"
        );
    }
    assert_eq!(seated_box(Platform::Nes), (240, 281));
    assert_eq!(seated_box(Platform::Snes), (240, 160));
}

fn inside(outer: Placed, x: f32, y: f32, w: f32, h: f32) -> bool {
    x >= outer.x && y >= outer.y && x + w <= outer.x + outer.w && y + h <= outer.y + outer.h
}

#[test]
fn the_nes_opens_where_the_design_puts_it() {
    let open = open_layout(Platform::Nes);
    let board = open.board_rest(Canvas::FRAMED);
    assert_eq!(
        board,
        Placed {
            x: 215.0,
            y: 18.0,
            w: 290.0,
            h: 340.0
        }
    );
    let lid = open.lid_rest(Canvas::FRAMED);
    assert_eq!((lid.x, lid.y, lid.w, lid.h), (44.0, 112.0, 145.0, 170.0));
}

#[test]
fn the_snes_opens_at_its_shelf_size_beside_its_lid() {
    let open = open_layout(Platform::Snes);
    let board = open.board_rest(Canvas::FRAMED);
    assert_eq!((board.w, board.h), shelf_box(Platform::Snes));
    assert!(board.y + board.h <= 364.0, "the board runs into the legend");
    let lid = open.lid_rest(Canvas::FRAMED);
    assert!(
        lid.x >= 0.0 && lid.x + lid.w < board.x,
        "the lid lies on the board"
    );
    assert!(
        board.x + board.w <= Canvas::FRAMED.w,
        "the board runs off the screen"
    );
}

#[test]
fn the_sockets_and_chip_sit_on_the_board() {
    for p in [Platform::Nes, Platform::Snes] {
        let open = open_layout(p);
        let board = open.board_rest(Canvas::FRAMED);
        let zoom = open.zoom(board);
        for i in 0..2 {
            let (x, y) = open.on_board(board, open.socket_u[i], open.socket_v);
            assert!(
                inside(board, x, y, 64.0 * zoom, 46.0 * zoom),
                "{p:?} socket {i}"
            );
            let (x, y) = open.on_board(board, open.chip_u[i], open.chip_v);
            assert!(
                inside(board, x, y, 59.0 * zoom, 41.0 * zoom),
                "{p:?} chip {i}"
            );
        }
    }
}

#[test]
fn the_boards_carry_the_title_on_their_rom() {
    for (p, (x, y, w, h)) in [
        (Platform::Nes, (188, 148, 64, 58)),
        (Platform::Snes, (52, 58, 104, 56)),
    ] {
        let face = board_face(&cart(p, "Super Mario World (Europe)"));
        assert_eq!((face.w, face.h), cart_box(p), "{p:?}");
        let blank = slot_ui::open_board(mould_of(&cart(p, "A")));
        let marked = (y..y + h)
            .flat_map(|yy| (x..x + w).map(move |xx| (xx, yy)))
            .filter(|&(xx, yy)| {
                let i = ((yy * face.w + xx) * 4) as usize;
                face.rgba[i..i + 3] != blank.rgba[i..i + 3]
            })
            .count();
        assert!(marked > 20, "{p:?}: the ROM chip carries no title");
        // Above the board the sheet is clear, so the back plastic shows.
        assert_eq!(alpha(&face.rgba, face.w, 145, 4), 0, "{p:?}");
    }
}

#[test]
fn an_opened_nes_or_snes_back_is_solid() {
    for p in [Platform::Nes, Platform::Snes] {
        let back = picker_interior(&cart(p, "A"));
        assert_eq!((back.w, back.h), cart_box(p));
        assert_eq!(
            alpha(&back.rgba, back.w, back.w / 2, back.h / 2),
            255,
            "{p:?}"
        );
    }
}

#[test]
fn the_stacks_stand_in_their_carts_box() {
    for p in [Platform::Nes, Platform::Snes] {
        let (w, h) = shelf_box(p);
        let slot = Placed {
            x: 100.0,
            y: 50.0,
            w,
            h,
        };
        let poses = stack_poses(p, slot);
        let cover = poses.carts[0].at;
        let (cx, cy) = (cover.x + cover.w / 2.0, cover.y + cover.h / 2.0);
        assert!(
            inside(slot, cx, cy, 0.0, 0.0),
            "{p:?}: the cover stands outside its box"
        );
        assert!(
            cover.w < slot.w && cover.h < slot.h,
            "{p:?}: the cover outgrows its box"
        );
        assert!(
            (cover.w / cover.h - w / h).abs() < 0.01,
            "{p:?}: the cover is stretched"
        );
    }
}
