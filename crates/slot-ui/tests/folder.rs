#[test]
fn a_long_folder_name_widens_its_strip_instead_of_losing_words() {
    let short = slot_ui::folder_title_face("RPG").expect("strip art");
    let long = slot_ui::folder_title_face("Final Fantasy").expect("strip art");
    assert!(long.w > short.w, "the strip kept its width for a long name");
    let (w, _) = slot_ui::folder_title_size(long.w);
    assert!(w <= 324.0, "the strip grew past its widest: {w}");
}
