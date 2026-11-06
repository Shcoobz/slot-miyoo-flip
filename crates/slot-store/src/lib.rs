macro_rules! eprintln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

mod atomic;
mod bindings;
pub mod cart_shell;
mod config;
mod core;
pub mod gb;
mod gb_palette;
mod gba;
pub mod ini;
mod platform;
mod ring;
mod scan;
mod slot_state;
mod stamp;

pub use atomic::{atomic_write, sweep_temps};
pub use bindings::{
    read_carousel, read_controls, read_shortcuts, write_carousel, write_controls, write_shortcuts,
    Binding, CarouselAction, CarouselKeys, Controls, Device, GameButton, Modifier, Shortcut,
    ShortcutKeys,
};
pub use cart_shell::{Outline, ShellChoice, ShellFinish, CART_SHELL_FILE, LABELS_SHELL_FILE};
mod cheats;
pub use cheats::{
    cheat_file, parse_cheats, read_cheats, write_cheat_choices, Cheat, CHEATS_DIR,
    CHEAT_CHOICES_DIR,
};
pub use config::{move_config, CONFIG_DIR};
mod activity;
pub use activity::{
    activity, civil, local_day, log_play, play_time_text, played, read_activity, weekday, Activity,
    Session, ACTIVITY_FILE,
};
mod suggest;
pub use suggest::{
    extras_system, pick, read_suggested, write_extras_system, write_suggested, EXTRAS_FILE,
    SUGGESTED_FILE,
};
mod folders;
pub use core::{
    core_for, core_for_cart, core_for_platform, default_core, folder_core, read_selected_cores,
    write_default_core, write_folder_core, write_selected_core, Core, DEFAULT_CORE_KEY,
    SELECTED_CORE_FILE,
};
pub use folders::{
    folder_key, read_folder_prefs, rename_folder_prefs, write_folder_prefs, FolderPaper,
    FolderPrefs, FolderSort, FOLDERS_FILE,
};
pub use gb_palette::{GbColors, GbPalette};
pub use gba::{header_clean, header_code, header_title, header_title_code};
pub use platform::Platform;
pub use ring::{StateEntry, StateRing, RING_MAX};
pub use scan::{initial, is_hidden, listing, scan, sort_key, Cart, StoreError};
pub use slot_state::{
    read_slot_state, write_slot_state, Appearance, BezelPick, Recent, Shader, SlotState,
    WallpaperPick, WallpaperTint, AUTO_SLEEP_MIN, BLUE_LIGHT_MAX, BRIGHTNESS_MAX, DIM_SCREEN_S,
    FF_SPEEDS, FF_SPEED_DEFAULT, LID_OFF_MIN, RECENTS_MAX, SWITCHER_GAMES, UTC_OFFSET_MAX,
    UTC_OFFSET_MIN, VOLUME_MAX,
};
pub use stamp::{
    civil_from_days, days_from_civil, days_in_month, format_stamp, parse_stamp, stamp_now,
};
