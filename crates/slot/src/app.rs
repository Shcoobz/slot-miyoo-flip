use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use slot_gfx::{Canvas, OUT_H, OUT_W};
use slot_input::{Action, Btn, RawEvent, MENU_DOUBLE_TAP_MS, MUTE_CHORD_MS};
use slot_power::{Battery, Charge, LedState, LidPolicy, Power, RepairResult};
use slot_retro::LinkChannel;
use slot_store::{
    format_stamp, parse_stamp, read_slot_state, scan, write_slot_state, BezelPick, Cart, Core,
    GbColors, Platform, Recent, Shader, SlotState, StateEntry, StateRing, WallpaperPick,
    WallpaperTint, BLUE_LIGHT_MAX, BRIGHTNESS_MAX, FF_SPEEDS, RING_MAX, VOLUME_MAX,
};
use slot_ui::{
    cart_box, draw_empty_slot, draw_footer, draw_ground, draw_slot_name, draw_sticker, ease,
    folder_title_size, footer_ink, grown, lift_of, open_layout, repeat_gap, shelf_cart_of,
    ClockPicker, Draw, FfState, GbShell, Hud, HudKind, Icon, LinkBadge, Millis, Placed, Polaroids,
    PowerChoice, QuickMenu, QuickMenuFaces, QuickRow, QuickValue, Refusal, Shelf, SlotChrome,
    Stack, TexId, Toast, CHIP_H, CHIP_W, HINT_EDGE, MOUTH_H, REPEAT_DELAY_MS, SHADOW_H, SHADOW_W,
    SOCKET_H, SOCKET_W, TURN_PAD,
};
use slot_ui::{draw_pose, shelf_box, stack_poses, Pose};

use crate::audio::Sfx;
use crate::core_picker::{Chip, CorePicker, Outcome, Press};
use crate::link_kind::{link_carried, link_dual, link_kind, serial_option, LinkKind};
use crate::link_radio::{radio_jobs, LinkRole, RadioJob, RadioJobs};
use crate::link_screen::LinkSprites;
use crate::link_start::{link_port, LinkFail, LinkProgress, LinkStarter, LinkStep};
use crate::persist::{self, Snapshot};
use crate::video_mode::{self, VideoMode};
use slot_store::{
    read_carousel, read_controls, read_shortcuts, write_carousel, write_controls, write_shortcuts,
    Binding, CarouselKeys, Controls, Device, GameButton, Modifier, ShortcutKeys, AUTO_SLEEP_MIN,
    DIM_SCREEN_S, LID_OFF_MIN, SWITCHER_GAMES,
};

mod cheats;
mod extras_menu;
mod played_plate;
mod power_saving;
mod report;

pub use power_saving::{cpu_cap, CPU_FULL_KHZ, CPU_LIGHT_KHZ};

pub const RECORDING: &str = "Recording";
pub const CLASH_MARK: &str = "!";

#[derive(Copy, Clone, Debug)]
struct BindCapture {
    row: QuickRow,
    menu: bool,
    first: Option<Device>,
}

pub const INSERT_S: f32 = 0.73;
const INSERT_HOLD_S: f32 = 0.28;

pub const SEATED_AT: f32 = INSERT_S - INSERT_HOLD_S;

pub const EJECT_S: f32 = SEATED_AT;

const EJECT_HOLD_S: f32 = 0.35;

const POWER_ON_S: f32 = 0.22;

const POWER_OFF_S: f32 = 0.16;

const FOOTER_SLIDE_S: f32 = POWER_ON_S;

const BLANK_WARM_S: f32 = 1.5;
const BLANK_DIM: f32 = 0.1;

const VOLUME_STEP: u8 = 5;

const AUTOSAVE_MS: Millis = 60_000;
const CARD_CHECK_MS: Millis = 2_000;

const BATTERY_CRITICAL: u8 = 5;

const REPAIR_BATTERY_MIN: u8 = 20;

const BATTERY_POLL_MS: Millis = 10_000;
const MENU_REVEAL_MS: Millis = 180;

const CHARGE_POLL_MS: Millis = 1_000;
const HEADPHONES_POLL_MS: Millis = 500;

const BATTERY_LOW: u8 = 20;

pub const UNDO_GRACE_MS: Millis = 30_000;

pub const LINK_LOST_MS: Millis = 2000;

const PLAY_HOLD_MS: Millis = 500;
const GAMES_MAX: usize = 10;

const SHUTDOWN_SHOW_MS: Millis = 250;

const REPAIR_GAP: u32 = 12;

const CORE_PICKER_RECEDE: f32 = 0.26;

const SLOT_NAME_IN_MS: Millis = 200;
const SLOT_NAME_HOLD_MS: Millis = 1200;
const SLOT_NAME_OUT_MS: Millis = 800;
const SLOT_NAME_ALPHA: f32 = 1.0;
const CORE_PICKER_DIM: f32 = 0.25 / (1.0 - CORE_PICKER_RECEDE);
const CORE_LEGEND_Y: f32 = OUT_H as f32 - MOUTH_H * 2.0;
const CORE_LEGEND_GAP: f32 = 24.0;
const LINK_TEXT_Y: f32 = 44.0;
const LINK_LEGEND_Y: f32 = OUT_H as f32 - MOUTH_H;
pub const LINK_SCRIM: [f32; 4] = [0.0, 0.0, 0.0, 0.5];
const LINK_LEGEND_GAP: f32 = 40.0;
const LINK_LEGEND_EDGE: f32 = 0.0;

fn link_keys_w(faces: &[(TexId, u32)]) -> f32 {
    faces
        .iter()
        .map(|(_, w)| w.saturating_sub(HINT_EDGE) as f32)
        .sum::<f32>()
        + LINK_LEGEND_GAP * faces.len().saturating_sub(1) as f32
}
const LID_SHADOW_ALPHA: f32 = 0.8;
const FACES_WAIT_MS: Millis = 1500;

const ALERT_HOLD: f32 = 0.45;
const ALERT_GONE: f32 = 0.9;

const CLOCK_FLOOR: i64 = 1_577_836_800;

pub enum PendingUndo {
    Save {
        stamp: String,
        evicted: Option<(String, Vec<u8>, Vec<u8>)>,
    },
    Load {
        prior: Vec<u8>,
    },
}

struct LinkSession {
    client_id: u16,
    lost_at: Option<Millis>,
}

struct LinkStarting {
    starter: LinkStarter,
    client_id: u16,
}

struct Reload {
    stem: String,
    role: LinkRow,
    cancelled: bool,
    from: LinkKind,
    from_serial: &'static str,
    fallback: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GameMenu {
    Pick(LinkRow),
    Working {
        role: LinkRow,
        step: LinkStep,
        since: Millis,
    },
    Linked {
        role: LinkRow,
        worked: Millis,
        since: Millis,
        opened: bool,
    },
    Failed {
        role: LinkRow,
        fail: LinkFail,
        worked: Millis,
        since: Millis,
    },
    Unplug {
        role: LinkRow,
        since: Millis,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkRow {
    Host,
    Join,
}

impl LinkRow {
    pub const ALL: [LinkRow; 2] = [LinkRow::Host, LinkRow::Join];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            LinkRow::Host => "Host",
            LinkRow::Join => "Join",
        }
    }

    pub fn role(self) -> LinkRole {
        match self {
            LinkRow::Host => LinkRole::Host,
            LinkRow::Join => LinkRole::Join,
        }
    }

    pub fn client_id(self) -> u16 {
        match self {
            LinkRow::Host => 0,
            LinkRow::Join => 1,
        }
    }

    pub fn other(self) -> LinkRow {
        match self {
            LinkRow::Host => LinkRow::Join,
            LinkRow::Join => LinkRow::Host,
        }
    }

    pub fn from_client_id(id: u16) -> LinkRow {
        if id == 0 {
            LinkRow::Host
        } else {
            LinkRow::Join
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkLegend {
    Cancel,
    Mode,
    Swap,
    Link,
    Ok,
    Back,
    EndLink,
}

impl LinkLegend {
    pub const ALL: [LinkLegend; 7] = [
        LinkLegend::Cancel,
        LinkLegend::Mode,
        LinkLegend::Swap,
        LinkLegend::Link,
        LinkLegend::Ok,
        LinkLegend::Back,
        LinkLegend::EndLink,
    ];

    pub fn index(self) -> usize {
        self as usize
    }
}

pub const LINKED_HOLD_MS: Millis = 1000;

pub const UNPLUG_HOLD_MS: Millis = 420;

#[derive(Debug)]
pub enum Phase {
    SetClock {
        picker: ClockPicker,
        seed: i64,
        from_menu: bool,
    },
    Shelf,
    QuickMenu {
        row: QuickRow,
    },
    Inserting {
        cart: String,
        t: f32,
        core_ready: bool,
        resumed: bool,
        clean: bool,
    },
    Playing {
        cart: String,
    },
    Ejecting {
        cart: String,
        t: f32,
    },
    Polaroids {
        cart: String,
    },
    About,
    Doze {
        cart: Option<String>,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Rack {
    /// Activity, Get lucky and Never played: no carts of its own.
    Extras,
    Recents,
    Favorites,
    Of(Platform),
    /// The games of one folder on a platform's shelf, opened from it with A.
    Folder(Platform),
}

impl Rack {
    fn platform(self) -> Option<Platform> {
        match self {
            Rack::Of(p) | Rack::Folder(p) => Some(p),
            Rack::Extras | Rack::Recents | Rack::Favorites => None,
        }
    }

    fn key(self) -> String {
        match self {
            Rack::Extras => "extras".into(),
            Rack::Recents => "recents".into(),
            Rack::Favorites => "favorites".into(),
            Rack::Of(p) => p.dir_name().to_ascii_lowercase(),
            Rack::Folder(p) => format!("folder-{}", p.dir_name().to_ascii_lowercase()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Rack::Extras => "Extras",
            Rack::Recents => "Recents",
            Rack::Favorites => "Favorites",
            Rack::Of(p) | Rack::Folder(p) => p.name(),
        }
    }
}

const EXTRAS_AT: usize = 0;
/// How stiffly the Extras row springs to the item under the cursor, as the shelf does.
const EXTRA_OMEGA: f32 = 16.0;
const RECENTS_AT: usize = 1;
/// Play is written to the card once this much of it has gathered.
const PLAY_LOG_SECS: f32 = 60.0;
/// How long a shut lid waits before sleeping for real: long enough for the last saves to land
/// and for a lid opened again straight away.
const SUSPEND_AFTER_MS: Millis = 3000;
/// How long the shelf must rest on a cart before that cart is saved as where to start next.
const SHELF_SETTLE_MS: Millis = 2000;

/// How many of a folder's carts its stack shows.
const STACK_CARTS: usize = 3;
/// The opened folder's name strip: its top on a 720 canvas over GBA carts, and over paks, and its
/// turn.
const FOLDER_TITLE_Y_GBA: f32 = 22.0;
const FOLDER_TITLE_Y_GB: f32 = 2.0;
const FOLDER_TITLE_TURN: f32 = -2.0;

/// Opening a folder takes this long; closing plays the same moves backwards.
const FOLDER_OPEN_MS: f32 = 1100.0;
/// How high the carts rise off each other before they fan out, and the side carts' scale there.
const FOLDER_LIFT: f32 = 24.0;
const SIDE_CART: f32 = 0.78;

/// The rows of a folder's START menu, top to bottom.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum FolderRow {
    Rename,
    Cover,
    Sort,
    Core,
    Open,
    Paper,
}

impl FolderRow {
    const ALL: [FolderRow; 6] = [
        FolderRow::Rename,
        FolderRow::Cover,
        FolderRow::Sort,
        FolderRow::Core,
        FolderRow::Open,
        FolderRow::Paper,
    ];

    fn label(self) -> &'static str {
        match self {
            FolderRow::Rename => "Rename",
            FolderRow::Cover => "Cover",
            FolderRow::Sort => "Sort",
            FolderRow::Core => "Core",
            FolderRow::Open => "Open at",
            FolderRow::Paper => "Paper",
        }
    }
}

/// The menu opens over this long and closes faster; the band's loop glides between rows.
const FOLDER_MENU_OPEN_MS: f32 = 420.0;
const FOLDER_MENU_CLOSE_MS: f32 = 300.0;
const FOLDER_LOOP_GLIDE_MS: f32 = 300.0;
/// The stack parks small over the sheet, centred at this height.
const FOLDER_PARKED_SCALE: f32 = 0.3;
const FOLDER_PARKED_Y: f32 = 62.0;

/// The name being written in the name entry.
#[derive(Clone, Debug)]
struct NameEntry {
    name: String,
    lower: bool,
    cursor: slot_ui::Cursor,
}

/// START on a folder: its settings, and Rename's name entry over them.
#[derive(Clone, Debug)]
struct FolderMenu {
    parent: usize,
    item: usize,
    folder: usize,
    row: usize,
    /// The first row shown; the sheet holds four.
    first: usize,
    opened: Millis,
    closing: Option<Millis>,
    /// Where the loop glides from, in rows below the first shown, and when it set off.
    glide: (f32, Millis),
    entry: Option<NameEntry>,
}

/// A folder opening onto its shelf, or closing back into its stack.
#[derive(Copy, Clone, Debug)]
struct FolderMotion {
    /// The shelf the folder stands on, and its place there.
    parent: usize,
    item: usize,
    /// The shelf of the folder's own games.
    folder: usize,
    start: Millis,
    opening: bool,
}
const FAVORITES_AT: usize = 2;

pub struct App {
    phase: Phase,
    shelves: Vec<(Rack, Shelf)>,
    shelf_at: usize,
    orphaned: Vec<TexId>,
    played_plate: played_plate::PlayedPlate,
    report: report::Report,
    folder_band: Option<TexId>,
    folder_motion: Option<FolderMotion>,
    /// The Extras item under the cursor, counted without wrapping so the row scrolls on, and
    /// where the row stands as it springs there.
    extra_pos: i64,
    extra_scroll: f32,
    extra_vel: f32,
    extra_faces: [Option<TexId>; 3],
    /// The slate's three dice and the blister pack's bubble, drawn over them and moved apart
    /// when they open.
    extra_dice: [Option<TexId>; 3],
    extra_bubble: Option<TexId>,
    /// A pick in motion: the item, when A was pressed, and the game it lands on.
    extra_motion: Option<(slot_ui::Extra, Millis, Recent)>,
    /// A pick's game is starting: no cartridge, no console, the screen goes straight to it.
    straight_in: bool,
    /// The receipt and the blister pack need drawing again: play was logged or a pick made.
    extras_stale: bool,
    /// START on Get lucky or Never played: its system setting.
    extras_menu: extras_menu::ExtrasMenu,
    /// The shelf and cart last seen under the cursor, and since when.
    shelf_seen: Option<((usize, String), Millis)>,
    /// Each folder's shelf and its settings.
    folder_prefs: Vec<(usize, slot_store::FolderPrefs)>,
    folder_menu: Option<FolderMenu>,
    folder_menu_face: Option<TexId>,
    folder_entry_face: Option<TexId>,
    /// The menu or entry sheet needs drawing again after a change.
    folder_sheet_stale: bool,
    folder_menu_loop: Option<TexId>,
    folder_key_loops: [Option<TexId>; 2],
    folder_legend: Vec<(TexId, u32)>,
    entry_legend: Vec<(TexId, u32)>,
    /// Bumped whenever carts change places on a shelf, so faces in flight are dropped.
    shelf_epoch: u64,
    /// Each opened folder's name strip, by the folder's shelf.
    /// Each opened folder's name strip, with its face's width.
    folder_titles: Vec<(usize, TexId, u32)>,
    play_held: Option<Millis>,
    held_dir: Option<(Btn, Millis, usize)>,
    refusal: Option<Refusal>,
    refused_from: Option<f32>,
    alert_face: Option<TexId>,
    shutdown_faces: Vec<(TexId, u32, u32)>,
    repair_faces: Vec<(TexId, u32, u32)>,
    core_picker: Option<CorePicker>,
    core_board_face: Option<TexId>,
    core_interior_face: Option<TexId>,
    core_lid_face: Option<TexId>,
    core_faces_stem: Option<String>,
    core_socket_faces: Vec<TexId>,
    core_chip_faces: Vec<TexId>,
    core_blank_chip_face: Option<TexId>,
    core_chip_shadow_face: Option<TexId>,
    core_legend_faces: Vec<(TexId, u32)>,
    game_menu: Option<GameMenu>,
    link_sprites: Option<LinkSprites>,
    link_shot: Option<TexId>,
    link_hardware: LinkKind,
    last_role: LinkRow,
    link_choices: HashMap<String, LinkKind>,
    link_loaded: Option<&'static str>,
    link_reload: Option<(String, &'static str)>,
    reload: Option<Reload>,
    link_menu_faces: Vec<(TexId, u32, u32)>,
    link_linked_face: Option<(TexId, u32, u32)>,
    link_step_faces: Vec<(TexId, u32, u32)>,
    link_fail_faces: Vec<(TexId, u32, u32)>,
    link_legend_faces: Vec<(TexId, u32)>,
    starting: Option<LinkStarting>,
    link_transport: Option<(u16, Box<dyn LinkChannel>)>,
    restarting: bool,
    repairing: bool,
    repair_prompt: bool,
    reset_prompt: Option<QuickRow>,
    reset_faces: Vec<(TexId, u32, u32)>,
    delete_prompt: bool,
    /// The game Y on its open cartridge offered to delete, waiting on X to confirm.
    game_delete: Option<Recent>,
    /// The prompt's title naming the game, its detail and its legends.
    game_delete_faces: Vec<(TexId, u32, u32)>,
    delete_faces: Vec<(TexId, u32, u32)>,
    act_at: Millis,
    root: Option<PathBuf>,
    state: SlotState,
    left_stick: bool,
    controls: Controls,
    capture: Option<BindCapture>,
    plugged_face: Option<TexId>,
    /// The playing game's cheats and the in-game menu's page of them.
    cheats: cheats::Cheats,
    binding_faces: HashMap<String, [(TexId, u32, u32); 2]>,
    carousel: CarouselKeys,
    shortcuts: ShortcutKeys,
    vol_before: Vec<(u8, bool, Millis)>,
    snapshot: Option<Box<dyn Snapshot>>,
    core: Core,
    colour_pending: Option<bool>,
    palette_live: bool,
    palette_pending: Option<GbColors>,
    /// When the player last pressed anything, for Auto Sleep and Dim Screen.
    pressed_at: Millis,
    saving: power_saving::Saving,
    /// The doze came from Auto Sleep, so with the lid open any press wakes.
    slept_idle: bool,
    /// The wall clock when the lid closed, which keeps running through a deep sleep.
    dozed_wall: Option<i64>,
    /// The game being played now, since when (wall clock), and the play not yet written.
    play_session: Option<(i64, Recent, f32)>,
    /// The device said no to a deep sleep; the screen just stays off.
    no_suspend: bool,
    link_player: Option<u8>,
    platform: Platform,
    named_core: bool,
    video_mode: VideoMode,
    link: Option<LinkSession>,
    sfx: Option<Sfx>,
    polaroids: Option<Polaroids>,
    games: Option<Vec<Recent>>,
    games_after_eject: bool,
    states_after_insert: bool,
    pending: Option<(PendingUndo, Millis)>,
    legend_faces: Vec<TexId>,
    games_legend_faces: Vec<TexId>,
    undo_face: Option<TexId>,
    clock_faces: Option<(TexId, TexId)>,
    clock_back: Option<(TexId, u32)>,
    quick_menu_faces: Option<QuickMenuFaces>,
    quick_clock_faces: Option<[(TexId, u32, u32); 2]>,
    quick_wallpaper_faces: Option<[(TexId, u32, u32); 2]>,
    quick_palette_faces: Option<[(TexId, u32, u32); 2]>,
    quick_bezel_faces: [Option<[(TexId, u32, u32); 2]>; Platform::ALL.len()],
    game_bezel: Option<TexId>,
    in_game_menu: Option<QuickRow>,
    /// 0 while the pause footer is off the bottom, 1 when it sits in the carousel's place.
    footer: f32,
    footer_row: Option<QuickRow>,
    states_from_menu: bool,
    menu_tap: Option<(Millis, Option<QuickRow>)>,
    sticker_face: Option<TexId>,
    ground: Option<TexId>,
    blur: Option<TexId>,
    wallpaper: Option<TexId>,
    random_wallpaper: Option<Option<String>>,
    battery_percent: slot_ui::Printed,
    bolt: Option<TexId>,
    hud_bolt: Option<TexId>,
    slot_faces: Option<(TexId, TexId)>,
    shelf_platform: slot_ui::Printed,
    shelf_title: slot_ui::Printed,
    name_pending: bool,
    slot_letter: Option<char>,
    shelf_named: Option<Millis>,
    shelf_clock: slot_ui::Printed,
    hud: Hud,
    screen: f32,
    blank_s: f32,
    lift: f32,
    sp_strip: Option<TexId>,
    eject_overlay: f32,
    panel: (u32, u32),
    game_ready: bool,
    clock: f64,
    power: Option<Power>,
    dozed_at: Millis,
    woke_on_press: bool,
    autosave_at: Millis,
    pending_save: Option<std::thread::JoinHandle<()>>,
    card_writes: Arc<crate::card_log::CardWrites>,
    resume_unread: bool,
    card_read_only: bool,
    card_check_at: Millis,
    battery_at: Millis,
    charge_at: Millis,
    headphones: bool,
    headphones_at: Millis,
    battery: Option<Battery>,
    last_led: Option<LedState>,
    powering_off: bool,
    radio: Box<dyn RadioJobs>,
    state_writer: crate::state_writer::StateWriter,
}

const FACE_AHEAD: i32 = 8;
const FACE_KEEP: i32 = 12;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FaceWant {
    pub shelf: usize,
    pub index: usize,
    pub on_screen: bool,
}

/// The systems' shelves, in the order they follow Extras, Recents and Favorites.
const SHELF_ORDER: [Platform; 5] = [
    Platform::Nes,
    Platform::Snes,
    Platform::Gb,
    Platform::Gbc,
    Platform::Gba,
];

fn shelves_of(carts: Vec<Cart>) -> Vec<(Rack, Shelf)> {
    let mut rows: Vec<(Rack, Vec<Cart>)> = [Rack::Extras, Rack::Recents, Rack::Favorites]
        .into_iter()
        .chain(SHELF_ORDER.iter().map(|p| Rack::Of(*p)))
        .map(|r| (r, Vec::new()))
        .collect();
    let mut folders: Vec<(Platform, String, Vec<Cart>)> = Vec::new();
    for cart in carts {
        if let Some(name) = cart.folder().map(str::to_string) {
            match folders
                .iter_mut()
                .find(|(p, n, _)| *p == cart.platform && *n == name)
            {
                Some((_, _, games)) => games.push(cart),
                None => folders.push((cart.platform, name, vec![cart])),
            }
            continue;
        }
        if let Some((_, row)) = rows.iter_mut().find(|(r, _)| *r == Rack::Of(cart.platform)) {
            row.push(cart);
        }
    }
    // Each folder stands on its platform's shelf as an entry named after it, in name order with
    // the games; its own games make a shelf of their own at the end, reached only through it.
    let first_folder = rows.len();
    let mut dirs = Vec::new();
    for (platform, name, games) in &folders {
        let dir = games[0].rom.parent().unwrap_or(&games[0].rom).to_path_buf();
        if let Some((_, row)) = rows.iter_mut().find(|(r, _)| *r == Rack::Of(*platform)) {
            row.push(Cart {
                platform: games[0].platform,
                stem: name.clone(),
                rom: dir.clone(),
                label: None,
                title: name.clone(),
                code: String::new(),
                shell: None,
            });
        }
        dirs.push(dir);
    }
    // Folders come first on their shelf, by name, then the loose games by name.
    for (_, row) in rows.iter_mut() {
        row.sort_by_key(|c| (!dirs.contains(&c.rom), slot_store::sort_key(&c.stem)));
    }
    let mut shelves: Vec<(Rack, Shelf)> = rows
        .into_iter()
        .map(|(rack, carts)| (rack, Shelf::new(carts)))
        .collect();
    for (_, shelf) in shelves.iter_mut() {
        for i in 0..shelf.carts.len() {
            if let Some(k) = dirs.iter().position(|d| *d == shelf.carts[i].rom) {
                shelf.set_folder(i, first_folder + k);
            }
        }
    }
    shelves.extend(
        folders
            .into_iter()
            .map(|(platform, _, games)| (Rack::Folder(platform), Shelf::new(games))),
    );
    shelves
}

fn shift_down(draws: &mut [Draw], dy: f32) {
    if dy == 0.0 {
        return;
    }
    for draw in draws {
        match draw {
            Draw::Rect { y, .. }
            | Draw::Tex { y, .. }
            | Draw::Part { y, .. }
            | Draw::Turned { y, .. } => *y += dy,
            Draw::Game | Draw::Shot { .. } => {}
        }
    }
}

impl App {
    pub fn new(carts: Vec<Cart>) -> Self {
        let shelves = shelves_of(carts);
        let shelf_at = shelves
            .iter()
            .position(|(_, s)| !s.carts.is_empty())
            .unwrap_or(0);
        let state_writer = crate::state_writer::StateWriter::spawn();
        let card_writes = state_writer.card_writes();
        App {
            radio: radio_jobs(),
            state_writer,
            phase: Phase::Shelf,
            shelves,
            shelf_at,
            orphaned: Vec::new(),
            played_plate: Default::default(),
            report: Default::default(),
            folder_band: None,
            folder_motion: None,
            extra_pos: 0,
            extra_scroll: 0.0,
            extra_vel: 0.0,
            extra_faces: [None; 3],
            extra_dice: [None; 3],
            extra_bubble: None,
            extra_motion: None,
            straight_in: false,
            extras_stale: true,
            extras_menu: Default::default(),
            shelf_seen: None,
            folder_prefs: Vec::new(),
            folder_menu: None,
            folder_menu_face: None,
            folder_entry_face: None,
            folder_sheet_stale: false,
            folder_menu_loop: None,
            folder_key_loops: [None; 2],
            folder_legend: Vec::new(),
            entry_legend: Vec::new(),
            shelf_epoch: 0,
            folder_titles: Vec::new(),
            play_held: None,
            held_dir: None,
            refusal: None,
            refused_from: None,
            alert_face: None,
            shutdown_faces: Vec::new(),
            repair_faces: Vec::new(),
            core_picker: None,
            core_board_face: None,
            core_interior_face: None,
            core_lid_face: None,
            core_faces_stem: None,
            core_socket_faces: Vec::new(),
            core_chip_faces: Vec::new(),
            core_blank_chip_face: None,
            core_chip_shadow_face: None,
            core_legend_faces: Vec::new(),
            game_menu: None,
            link_sprites: None,
            link_shot: None,
            link_hardware: LinkKind::Cable,
            last_role: LinkRow::Host,
            link_choices: HashMap::new(),
            link_loaded: None,
            link_reload: None,
            reload: None,
            link_menu_faces: Vec::new(),
            link_linked_face: None,
            link_step_faces: Vec::new(),
            link_fail_faces: Vec::new(),
            link_legend_faces: Vec::new(),
            starting: None,
            link_transport: None,
            restarting: false,
            repairing: false,
            repair_prompt: false,
            reset_prompt: None,
            reset_faces: Vec::new(),
            delete_prompt: false,
            game_delete: None,
            game_delete_faces: Vec::new(),
            delete_faces: Vec::new(),
            act_at: 0,
            root: None,
            state: SlotState::default(),
            left_stick: false,
            controls: Controls::default(),
            capture: None,
            plugged_face: None,
            cheats: cheats::Cheats::default(),
            binding_faces: HashMap::new(),
            carousel: CarouselKeys::default(),
            shortcuts: ShortcutKeys::default(),
            vol_before: Vec::new(),
            snapshot: None,
            core: Core::default(),
            colour_pending: None,
            palette_live: false,
            palette_pending: None,
            pressed_at: 0,
            saving: power_saving::Saving::default(),
            slept_idle: false,
            dozed_wall: None,
            play_session: None,
            no_suspend: false,
            link_player: None,
            platform: Platform::default(),
            named_core: false,
            video_mode: VideoMode::default(),
            link: None,
            sfx: None,
            polaroids: None,
            games: None,
            games_after_eject: false,
            states_after_insert: false,
            pending: None,
            legend_faces: Vec::new(),
            games_legend_faces: Vec::new(),
            undo_face: None,
            clock_faces: None,
            clock_back: None,
            quick_menu_faces: None,
            quick_clock_faces: None,
            quick_wallpaper_faces: None,
            quick_palette_faces: None,
            quick_bezel_faces: [None; Platform::ALL.len()],
            game_bezel: None,
            in_game_menu: None,
            footer: 0.0,
            footer_row: None,
            states_from_menu: false,
            menu_tap: None,
            sticker_face: None,
            ground: None,
            blur: None,
            wallpaper: None,
            random_wallpaper: None,
            battery_percent: slot_ui::Printed::default(),
            bolt: None,
            hud_bolt: None,
            slot_faces: None,
            shelf_platform: slot_ui::Printed::default(),
            shelf_title: slot_ui::Printed::default(),
            name_pending: false,
            slot_letter: None,
            shelf_named: None,
            shelf_clock: slot_ui::Printed::default(),
            hud: Hud::new(),
            screen: 0.0,
            blank_s: 0.0,
            lift: 1.0,
            sp_strip: None,
            eject_overlay: 0.0,
            panel: (0, 0),
            game_ready: false,
            clock: 0.0,
            power: None,
            dozed_at: 0,
            woke_on_press: false,
            autosave_at: AUTOSAVE_MS,
            pending_save: None,
            card_writes,
            resume_unread: false,
            card_read_only: false,
            card_check_at: 0,
            battery_at: BATTERY_POLL_MS,
            charge_at: CHARGE_POLL_MS,
            headphones: false,
            headphones_at: 0,
            battery: None,
            last_led: None,
            powering_off: false,
        }
    }

    pub fn boot(root: &Path) -> Self {
        crate::root::ensure(root);
        crate::boot_time::mark("root");
        let carts = scan(root).unwrap_or_default();
        crate::boot_time::mark("scan");
        let mut app = App::new(carts);
        app.root = Some(root.to_path_buf());
        app.state = read_slot_state(root);
        app.carousel = read_carousel(root);
        app.shortcuts = read_shortcuts(root);
        app.controls = read_controls(root);
        app.restock_recents();
        app.restock_favorites();
        app.load_folders();
        if let Some((at, i)) = app.remembered_seat() {
            app.shelf_at = at;
            app.shelves[at].1.select(i);
        } else if !app.shelves[RECENTS_AT].1.carts.is_empty() {
            app.shelf_at = RECENTS_AT;
        }
        if app.state.clock_set {
            app.start();
        } else {
            app.phase = clock_screen(system_secs(), 0, false);
        }
        app
    }

    fn start(&mut self) {
        let seated = if self.single_cart() {
            Some((self.shelf_at, 0))
        } else {
            let platform = self.state.cart_platform;
            // A card that never named the cart's system was left by a slot that ran GBA carts
            // first, so a GBA cart of that name is the one it meant, whatever shelf comes first.
            self.state.cart.as_deref().and_then(|stem| match platform {
                Some(_) => self.find_seat(stem, platform),
                None => self
                    .find_seat(stem, Some(Platform::Gba))
                    .or_else(|| self.find_seat(stem, None)),
            })
        };
        self.phase = Phase::Shelf;
        match seated {
            Some((at, i)) => {
                self.shelf_at = at;
                self.shelf_mut().select(i);
                self.insert(false);
                if let Phase::Inserting { resumed, t, .. } = &mut self.phase {
                    *resumed = true;
                    *t = INSERT_S;
                }
            }
            None => {
                self.state.cart = None;
                self.state.cart_platform = None;
                self.name_pending = true;
            }
        }
    }

    fn shelf(&self) -> &Shelf {
        &self.shelves[self.shelf_at].1
    }

    fn shelf_mut(&mut self) -> &mut Shelf {
        &mut self.shelves[self.shelf_at].1
    }

    fn find_seat(&self, stem: &str, platform: Option<Platform>) -> Option<(usize, usize)> {
        let here = self
            .shelf()
            .carts
            .iter()
            .position(|c| c.stem == stem && platform.is_none_or(|p| c.platform == p));
        here.map(|i| (self.shelf_at, i))
            .or_else(|| self.seat_of(stem, platform))
    }

    fn switch_shelf(&mut self, by: i32) {
        if self.in_folder() {
            return;
        }
        if let Some(to) = self.next_shelf(by) {
            self.go_to_shelf(to);
        }
    }

    fn go_to_shelf(&mut self, to: usize) {
        self.extras_stale |= to == EXTRAS_AT;
        self.shelf_mut().release_hold();
        self.play_held = None;
        self.shelf_at = to;
        self.shelf_platform = slot_ui::Printed::default();
        self.slot_letter = None;
        self.shelf_named = None;
        self.name_pending = true;
    }

    fn track_held_dir(&mut self, action: Action) {
        match action {
            Action::GbaDown(b @ (Btn::Up | Btn::Down | Btn::Left | Btn::Right)) => {
                if self.held_dir.map(|(held, ..)| held) != Some(b) {
                    self.held_dir = Some((b, self.now() + REPEAT_DELAY_MS, 0));
                }
            }
            Action::GbaUp(b) if self.held_dir.is_some_and(|(held, ..)| held == b) => {
                self.held_dir = None;
            }
            _ => {}
        }
    }

    fn repeat_held_dir(&mut self) {
        let Some((btn, due, fired)) = self.held_dir else {
            return;
        };
        if self.now() < due {
            return;
        }
        if !self.repeats(btn) {
            self.held_dir = None;
            return;
        }
        self.held_dir = Some((btn, self.now() + repeat_gap(fired), fired + 1));
        self.apply(Action::GbaDown(btn));
    }

    fn repeats(&self, btn: Btn) -> bool {
        let vertical = matches!(btn, Btn::Up | Btn::Down);
        if self.game_menu.is_some() {
            return false;
        }
        match self.phase {
            Phase::SetClock { .. } => true,
            Phase::QuickMenu { .. } => vertical,
            Phase::Shelf if self.games.is_some() => !vertical,
            Phase::Shelf => vertical && self.core_picker.is_none(),
            Phase::Polaroids { .. } => !vertical,
            Phase::Playing { .. } => vertical && self.in_game_menu.is_some(),
            _ => false,
        }
    }

    fn jump_letter(&mut self, dir: i32) {
        let before = self.shelf().index;
        match dir > 0 {
            true => self.shelf_mut().jump_next_letter(),
            false => self.shelf_mut().jump_prev_letter(),
        }
        if self.shelf().index == before {
            return;
        }
        let Some(letter) = self.selected_stem().map(slot_store::initial) else {
            return;
        };
        if self.slot_letter != Some(letter) {
            self.shelf_platform = slot_ui::Printed::default();
        }
        self.slot_letter = Some(letter);
        self.shelf_named = None;
        self.name_pending = true;
    }

    fn slot_name_alpha(&self) -> f32 {
        let Some(at) = self.shelf_named else {
            return 0.0;
        };
        let t = self.now().saturating_sub(at);
        let level = if t < SLOT_NAME_IN_MS {
            t as f32 / SLOT_NAME_IN_MS as f32
        } else if t < SLOT_NAME_IN_MS + SLOT_NAME_HOLD_MS {
            1.0
        } else {
            let out = t - SLOT_NAME_IN_MS - SLOT_NAME_HOLD_MS;
            1.0 - (out as f32 / SLOT_NAME_OUT_MS as f32).min(1.0)
        };
        SLOT_NAME_ALPHA * ease(level)
    }

    fn next_shelf(&self, by: i32) -> Option<usize> {
        let n = self.shelves.len() as i32;
        (1..n)
            .map(|step| (self.shelf_at as i32 + by * step).rem_euclid(n) as usize)
            .find(|at| {
                let stocked = match self.shelves[*at].0 {
                    Rack::Extras => self
                        .shelves
                        .iter()
                        .any(|(r, s)| r.platform().is_some() && !s.carts.is_empty()),
                    _ => !self.shelves[*at].1.carts.is_empty(),
                };
                stocked && !matches!(self.shelves[*at].0, Rack::Folder(_))
            })
    }

    fn in_folder(&self) -> bool {
        matches!(self.shelves[self.shelf_at].0, Rack::Folder(_))
    }

    fn selected_folder(&self) -> Option<usize> {
        self.shelf().folder_at(self.shelf().index)
    }

    /// Opens the folder under the cursor onto its own shelf; false when the entry is a game.
    fn open_folder(&mut self) -> bool {
        let Some(at) = self.selected_folder() else {
            return false;
        };
        let (parent, item) = (self.shelf_at, self.shelf().index);
        let landing = self
            .prefs_of(at)
            .filter(|p| p.open_last)
            .and_then(|p| p.last.clone())
            .and_then(|last| self.shelves[at].1.carts.iter().position(|c| c.stem == last))
            .unwrap_or(0);
        self.go_to_shelf(at);
        self.shelf_mut().select(landing);
        self.folder_motion = Some(FolderMotion {
            parent,
            item,
            folder: at,
            start: self.now(),
            opening: true,
        });
        true
    }

    /// The folder in motion, with how far along the opening it stands in milliseconds (a closing
    /// runs that clock backwards).
    fn folder_moving(&self) -> Option<(FolderMotion, f32)> {
        let m = self.folder_motion?;
        let gone = self.now().saturating_sub(m.start) as f32;
        if gone >= FOLDER_OPEN_MS {
            return None;
        }
        Some((
            m,
            if m.opening {
                gone
            } else {
                FOLDER_OPEN_MS - gone
            },
        ))
    }

    fn close_folder(&mut self) {
        if !self.in_folder() {
            return;
        }
        let here = self.shelf_at;
        let parent = self
            .shelves
            .iter()
            .enumerate()
            .find_map(|(at, (_, shelf))| {
                (0..shelf.carts.len())
                    .find(|i| shelf.folder_at(*i) == Some(here))
                    .map(|i| (at, i))
            });
        if let Some((at, i)) = parent {
            self.go_to_shelf(at);
            self.shelf_mut().select(i);
            self.folder_motion = Some(FolderMotion {
                parent: at,
                item: i,
                folder: here,
                start: self.now(),
                opening: false,
            });
        }
    }

    fn remembered_seat(&self) -> Option<(usize, usize)> {
        let key = self.state.shelf.as_deref()?;
        let cart = self.state.shelf_cart.as_ref()?;
        let at = (0..self.shelves.len()).find(|at| self.rack_key(*at) == key)?;
        let i = self.shelves[at]
            .1
            .carts
            .iter()
            .position(|c| c.platform == cart.platform && c.stem == cart.stem)?;
        Some((at, i))
    }

    fn remember_shelf(&mut self) {
        let Some(cart) = self.shelf().carts.get(self.shelf().index) else {
            return;
        };
        let (shelf, shelf_cart) = (
            Some(self.rack_key(self.shelf_at)),
            Some(Recent {
                platform: cart.platform,
                stem: cart.stem.clone(),
            }),
        );
        if (&self.state.shelf, &self.state.shelf_cart) == (&shelf, &shelf_cart) {
            return;
        }
        self.state.shelf = shelf;
        self.state.shelf_cart = shelf_cart;
        self.persist();
    }

    /// A shelf's key in slot.state; a folder's names the folder, `folder:GBA/Pokemon Hacks`.
    fn rack_key(&self, at: usize) -> String {
        match self.shelves[at].0 {
            Rack::Folder(_) => format!("folder:{}", self.folder_key(at).unwrap_or_default()),
            rack => rack.key(),
        }
    }

    /// Saves where the shelf stands once it has rested on a cart a while, so the next start
    /// comes back to it however Slot was left: lid, battery or a restart from outside.
    fn remember_settled_shelf(&mut self) {
        let here = match (&self.phase, self.selected_stem()) {
            (Phase::Shelf, Some(stem))
                if self.folder_menu.is_none() && self.folder_moving().is_none() =>
            {
                (self.shelf_at, stem.to_string())
            }
            _ => return,
        };
        let now = self.now();
        match &self.shelf_seen {
            Some((seen, since)) if *seen == here => {
                if now.saturating_sub(*since) >= SHELF_SETTLE_MS {
                    self.remember_shelf();
                }
            }
            _ => self.shelf_seen = Some((here, now)),
        }
    }

    fn seat_of(&self, stem: &str, platform: Option<Platform>) -> Option<(usize, usize)> {
        self.shelves
            .iter()
            .enumerate()
            .filter(|(_, (r, _))| {
                platform.is_none_or(|want| r.platform().is_none_or(|p| p == want))
            })
            .find_map(|(at, (_, shelf))| {
                shelf
                    .carts
                    .iter()
                    .position(|c| c.stem == stem && platform.is_none_or(|p| c.platform == p))
                    .map(|i| (at, i))
            })
    }

    pub fn confirm_clock(&mut self) {
        let Phase::SetClock {
            picker,
            seed,
            from_menu,
        } = &self.phase
        else {
            return;
        };
        let (moved, offset, from_menu) = (picker.secs() - *seed, picker.offset_min(), *from_menu);
        let utc = self.utc_secs() + moved;
        if let Some(power) = &mut self.power {
            power.set_clock(utc);
        }
        self.state.utc_offset_min = offset as i16;
        self.state.clock_set = true;
        self.persist();
        if from_menu {
            self.phase = Phase::QuickMenu {
                row: QuickRow::DateTime,
            };
        } else {
            self.start();
        }
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn wall_secs(&self) -> i64 {
        self.utc_secs() + i64::from(self.state.utc_offset_min) * 60
    }

    fn utc_secs(&self) -> i64 {
        self.power.as_ref().map_or_else(system_secs, |p| p.now())
    }

    pub fn picker(&self) -> Option<&ClockPicker> {
        match &self.phase {
            Phase::SetClock { picker, .. } => Some(picker),
            _ => None,
        }
    }

    pub fn quick_menu(&self) -> Option<QuickRow> {
        match self.phase {
            Phase::QuickMenu { row } => Some(row),
            _ => None,
        }
    }

    pub fn in_game_menu(&self) -> Option<QuickRow> {
        self.in_game_menu
    }

    pub fn in_game_hidden(&self) -> Vec<QuickRow> {
        let platform = self.playing_platform();
        let no_bezel = (platform == Platform::Gba && !self.framed_panel()) || platform.on_a_tv();
        let mut hidden = Vec::new();
        if !self.palette_live {
            hidden.push(QuickRow::Palette);
        }
        if no_bezel {
            hidden.push(QuickRow::Bezel);
        }
        // The TV consoles have no LCD shaders or colour correction to pick.
        if platform.on_a_tv() {
            hidden.extend([QuickRow::Shader, QuickRow::ColourCorrection]);
        } else {
            // The handhelds keep their picture keys, L1 and R1, and no row for it.
            hidden.push(QuickRow::Picture);
        }
        if !self.has_cheats() {
            hidden.push(QuickRow::Cheats);
        }
        if self.single_cart() {
            hidden.push(QuickRow::ExitGame);
        }
        hidden.extend(self.hidden_tv_core_rows());
        // Only the buttons the playing console has: L/R on GBA and SNES, X/Y on SNES, turbo on
        // the rest.
        let buttons = GameButton::of(platform);
        hidden.extend(
            QuickRow::CONTROLS
                .into_iter()
                .filter(|r| r.game_button().is_some_and(|g| !buttons.contains(&g))),
        );
        if !self.left_stick {
            hidden.push(QuickRow::LeftStick);
        }
        if !self.state.turbo {
            hidden.extend([QuickRow::GameTurboA, QuickRow::GameTurboB]);
        }
        hidden
    }

    pub fn gb_colors_label(&self) -> &'static str {
        self.state.gb_colors().label()
    }

    pub fn set_quick_palette_faces(&mut self, dim: (TexId, u32, u32), lit: (TexId, u32, u32)) {
        self.quick_palette_faces = Some([dim, lit]);
    }

    fn playing_row(&self, row: QuickRow) -> QuickRow {
        match (row, self.playing_platform()) {
            (QuickRow::Shader, Platform::Gba) => QuickRow::GbaShader,
            (QuickRow::Shader, Platform::Gb) => QuickRow::GbShader,
            (QuickRow::Shader, Platform::Gbc) => QuickRow::GbcShader,
            (QuickRow::ColourCorrection, Platform::Gba) => QuickRow::GbaColourCorrection,
            (QuickRow::ColourCorrection, Platform::Gb) => QuickRow::GbColourCorrection,
            (QuickRow::ColourCorrection, Platform::Gbc) => QuickRow::GbcColourCorrection,
            (QuickRow::Bezel, Platform::Gba) => QuickRow::GbaBezel,
            (QuickRow::Bezel, Platform::Gb) => QuickRow::GbBezel,
            (QuickRow::Bezel, Platform::Gbc) => QuickRow::GbcBezel,
            (QuickRow::PlayBrightness, _) => QuickRow::Brightness,
            (QuickRow::PlayBlueLight, _) => QuickRow::BlueLight,
            (row, _) => row,
        }
    }

    pub fn quick_value(&self, row: QuickRow) -> Option<QuickValue> {
        match row {
            QuickRow::Shader
            | QuickRow::ColourCorrection
            | QuickRow::PlayBrightness
            | QuickRow::PlayBlueLight => Some(self.playing_row(row))
                .filter(|playing| *playing != row)
                .and_then(|playing| self.quick_value(playing)),
            QuickRow::Brightness => Some(QuickValue::level(self.state.brightness)),
            QuickRow::BlueLight => Some(match self.state.blue_light {
                0 => QuickValue::Off,
                n => QuickValue::level(n),
            }),
            QuickRow::ScreenGeneral => None,
            QuickRow::FastForward => QuickValue::speed(self.state.ff_speed),
            QuickRow::FastForwardSound => Some(QuickValue::flag(self.state.ff_sound)),
            QuickRow::GbaColourCorrection => {
                Some(QuickValue::flag(self.state.colour_correction_gba))
            }
            QuickRow::GbColourCorrection => Some(QuickValue::flag(self.state.colour_correction_gb)),
            QuickRow::GbcColourCorrection => {
                Some(QuickValue::flag(self.state.colour_correction_gbc))
            }
            QuickRow::GbaShader => Some(shader_value(self.state.shader_gba)),
            QuickRow::GbShader => Some(shader_value(self.state.shader_gb)),
            QuickRow::GbcShader => Some(shader_value(self.state.shader_gbc)),
            QuickRow::Rumble => Some(QuickValue::flag(self.state.rumble)),
            QuickRow::LeftStick => Some(QuickValue::flag(self.state.left_stick_dpad)),
            QuickRow::EjectSave => Some(QuickValue::flag(self.state.eject_save)),
            QuickRow::SwitcherGames => QuickValue::count(self.state.switcher_games),
            QuickRow::AutoSleep => QuickValue::AUTO_SLEEP
                .into_iter()
                .find(|v| v.minutes().unwrap_or(0) == self.state.auto_sleep_min),
            QuickRow::Picture => Some(match self.video_mode {
                VideoMode::Stretch => QuickValue::Stretch,
                VideoMode::Actual => QuickValue::Sharp,
            }),
            QuickRow::DimScreen => QuickValue::DIM_SCREEN
                .into_iter()
                .find(|v| v.seconds() == Some(self.state.dim_screen_s)),
            QuickRow::GbaCore => Some(QuickValue::core(self.default_core(Platform::Gba))),
            QuickRow::GbCore => Some(QuickValue::core(self.default_core(Platform::Gb))),
            QuickRow::NesCore => Some(QuickValue::core(self.default_core(Platform::Nes))),
            QuickRow::SnesCore => Some(QuickValue::core(self.default_core(Platform::Snes))),
            QuickRow::Turbo => Some(QuickValue::flag(self.state.turbo)),
            QuickRow::Rewind => Some(QuickValue::flag(self.state.rewind)),
            QuickRow::Recents => Some(if self.state.show_recents {
                QuickValue::Show
            } else {
                QuickValue::Hide
            }),
            QuickRow::ShowTitles => Some(if self.state.show_titles {
                QuickValue::Show
            } else {
                QuickValue::Hide
            }),
            QuickRow::WallpaperTint => Some(match self.state.wallpaper_tint {
                WallpaperTint::Off => QuickValue::Off,
                WallpaperTint::Low => QuickValue::Low,
                WallpaperTint::Medium => QuickValue::Medium,
                WallpaperTint::High => QuickValue::High,
            }),
            QuickRow::LidOff => QuickValue::LID_OFF
                .into_iter()
                .find(|v| v.minutes() == Some(self.lid_off_min())),
            QuickRow::Theme => Some(if self.light() {
                QuickValue::Light
            } else {
                QuickValue::Dark
            }),
            QuickRow::DateTime
            | QuickRow::About
            | QuickRow::Screen
            | QuickRow::Game
            | QuickRow::Wallpaper
            | QuickRow::Resume
            | QuickRow::States
            | QuickRow::Settings
            | QuickRow::ExitGame
            | QuickRow::PlayScreen
            | QuickRow::PlayGameplay
            | QuickRow::Palette
            | QuickRow::GbPalette
            | QuickRow::ScreenGba
            | QuickRow::ScreenGbc
            | QuickRow::ScreenGb
            | QuickRow::GbBezel
            | QuickRow::GbcBezel
            | QuickRow::GbaBezel
            | QuickRow::Bezel
            | QuickRow::General
            | QuickRow::ShelfPrev
            | QuickRow::ShelfNext
            | QuickRow::ShelfFavorite
            | QuickRow::PlayControls
            | QuickRow::Shortcuts
            | QuickRow::GameUp
            | QuickRow::GameDown
            | QuickRow::GameLeft
            | QuickRow::GameRight
            | QuickRow::GameA
            | QuickRow::GameB
            | QuickRow::GameX
            | QuickRow::GameY
            | QuickRow::GameSelect
            | QuickRow::GameStart
            | QuickRow::GameL
            | QuickRow::GameR
            | QuickRow::GameTurboA
            | QuickRow::GameTurboB
            | QuickRow::ScSave
            | QuickRow::ScLoad
            | QuickRow::ScBrightUp
            | QuickRow::ScBrightDown
            | QuickRow::ScBlueDown
            | QuickRow::ScBlueUp
            | QuickRow::ScColour
            | QuickRow::ScPaletteBack
            | QuickRow::ScPaletteNext
            | QuickRow::ScShaderNext
            | QuickRow::ScShaderBack
            | QuickRow::ScFastForward
            | QuickRow::ScRewind
            | QuickRow::Controls
            | QuickRow::ControlsShelf
            | QuickRow::ControlsInGame
            | QuickRow::Cheats
            | QuickRow::CheatList => None,
        }
    }

    pub fn set_quick_menu_faces(&mut self, faces: QuickMenuFaces) {
        self.quick_menu_faces = Some(faces);
    }

    pub fn set_quick_clock_faces(&mut self, dim: (TexId, u32, u32), lit: (TexId, u32, u32)) {
        self.quick_clock_faces = Some([dim, lit]);
    }

    pub fn set_quick_wallpaper_faces(&mut self, dim: (TexId, u32, u32), lit: (TexId, u32, u32)) {
        self.quick_wallpaper_faces = Some([dim, lit]);
    }

    pub fn set_quick_bezel_faces(
        &mut self,
        platform: Platform,
        dim: (TexId, u32, u32),
        lit: (TexId, u32, u32),
    ) {
        self.quick_bezel_faces[bezel_slot(platform)] = Some([dim, lit]);
    }

    pub fn bezel_files(&self, platform: Platform) -> Vec<String> {
        self.root()
            .map(|root| crate::bezel::list(root, platform))
            .unwrap_or_default()
    }

    fn bezel_choices(&self, platform: Platform) -> Vec<BezelPick> {
        let mut choices = vec![BezelPick::None];
        // The built-in Game Boy frame is drawn round the GBA-shaped screen; a 4:3 panel shows the
        // Game Boy at full height instead, so there only the player's own bezels are offered.
        if platform != Platform::Gba && !self.framed_panel() {
            choices.push(BezelPick::Default);
        }
        if self.framed_panel() && platform == Platform::Gba {
            choices.push(BezelPick::Ags102);
        }
        choices.extend(self.bezel_files(platform).into_iter().map(BezelPick::File));
        choices
    }

    /// The handhelds' bezel picks; the TV consoles have none and show their 4:3 picture bare.
    fn bezel_pick(&mut self, platform: Platform) -> Option<&mut BezelPick> {
        match platform {
            Platform::Gba => Some(&mut self.state.bezel_gba),
            Platform::Gb => Some(&mut self.state.bezel_gb),
            Platform::Gbc => Some(&mut self.state.bezel_gbc),
            Platform::Nes | Platform::Snes => None,
        }
    }

    pub fn resolved_bezel(&self, platform: Platform) -> BezelPick {
        let pick = match platform {
            Platform::Gba => &self.state.bezel_gba,
            Platform::Gb => &self.state.bezel_gb,
            Platform::Gbc => &self.state.bezel_gbc,
            Platform::Nes | Platform::Snes => return BezelPick::None,
        };
        match self.bezel_choices(platform).contains(pick) {
            true => pick.clone(),
            false => BezelPick::None,
        }
    }

    fn cycle_bezel(&mut self, platform: Platform, right: bool) {
        let choices = self.bezel_choices(platform);
        let key = self.resolved_bezel(platform).key();
        let at = choices.iter().position(|c| c.key() == key).unwrap_or(0);
        let n = choices.len();
        let to = choices[if right {
            (at + 1) % n
        } else {
            (at + n - 1) % n
        }]
        .clone();
        if let Some(pick) = self.bezel_pick(platform) {
            *pick = to;
        }
    }

    pub fn sp_frame(&self) -> bool {
        self.resolved_bezel(self.playing_platform()) == BezelPick::Ags102
    }

    pub fn set_game_bezel(&mut self, tex: Option<TexId>) {
        self.game_bezel = tex;
    }

    pub fn shown_game_bezel(&self) -> Option<TexId> {
        if self.video_mode == VideoMode::Stretch {
            return None;
        }
        self.game_bezel
    }

    pub fn wallpaper_files(&self) -> Vec<String> {
        self.root().map(crate::wallpaper::list).unwrap_or_default()
    }

    pub fn wallpaper(&self) -> WallpaperPick {
        self.state.wallpaper.clone()
    }

    pub fn resolved_wallpaper(&self) -> WallpaperPick {
        match &self.state.wallpaper {
            WallpaperPick::File(name) if !self.wallpaper_files().iter().any(|n| n == name) => {
                WallpaperPick::Default
            }
            other => other.clone(),
        }
    }

    pub fn shown_wallpaper(&mut self) -> Option<String> {
        match self.resolved_wallpaper() {
            WallpaperPick::File(name) => Some(name),
            WallpaperPick::Random => self.random_wallpaper(),
            _ => None,
        }
    }

    fn random_wallpaper(&mut self) -> Option<String> {
        if self.random_wallpaper.is_none() {
            let seed = self.wall_secs().unsigned_abs();
            let pick = self
                .root()
                .and_then(|root| crate::wallpaper::pick(root, seed))
                .and_then(|path| Some(path.file_name()?.to_str()?.to_owned()));
            self.random_wallpaper = Some(pick);
        }
        self.random_wallpaper.clone().flatten()
    }

    fn cycle_wallpaper(&mut self, right: bool) {
        let mut choices = vec![
            WallpaperPick::None,
            WallpaperPick::Random,
            WallpaperPick::Default,
            WallpaperPick::Blur,
        ];
        choices.extend(self.wallpaper_files().into_iter().map(WallpaperPick::File));
        let key = self.resolved_wallpaper().key();
        let at = choices.iter().position(|c| c.key() == key).unwrap_or(2);
        let n = choices.len();
        let next = if right {
            (at + 1) % n
        } else {
            (at + n - 1) % n
        };
        self.state.wallpaper = choices[next].clone();
    }

    fn grounds(&self) -> [Option<(TexId, u32)>; 3] {
        let ground = self.ground.map(|t| (t, OUT_W));
        let blur = self.blur.map(|t| (t, OUT_W)).or(ground);
        let own = self
            .wallpaper
            .map(|t| (t, self.wallpaper_size().0))
            .or(ground);
        [ground, blur, own]
    }

    fn menu_ground(&self) -> Option<(TexId, u32)> {
        let [_, blur, own] = self.grounds();
        match &self.state.wallpaper {
            WallpaperPick::None => None,
            WallpaperPick::Default | WallpaperPick::Blur => blur,
            WallpaperPick::Random if !self.random_drawn() => blur,
            WallpaperPick::File(_) | WallpaperPick::Random => own,
        }
    }

    fn draw_background(&self, out: &mut Vec<Draw>) {
        let [ground, blur, own] = self.grounds();
        let face = match &self.state.wallpaper {
            WallpaperPick::None => None,
            WallpaperPick::Default => ground,
            WallpaperPick::Random if !self.random_drawn() => ground,
            WallpaperPick::Blur => blur,
            WallpaperPick::File(_) | WallpaperPick::Random => own,
        };
        draw_ground(self.canvas(), face, out);
        let alpha = tint_alpha(self.state.wallpaper_tint);
        if alpha > 0.0 && self.own_wallpaper_shown() {
            out.push(Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: self.canvas().w,
                h: OUT_H as f32,
                colour: [0.0, 0.0, 0.0, alpha],
            });
        }
    }

    fn own_wallpaper_shown(&self) -> bool {
        match self.state.wallpaper {
            WallpaperPick::File(_) => self.wallpaper.is_some(),
            WallpaperPick::Random => self.random_drawn() && self.wallpaper.is_some(),
            _ => false,
        }
    }

    pub fn hidden_rows(&self) -> Vec<QuickRow> {
        let own = matches!(
            self.state.wallpaper,
            WallpaperPick::File(_) | WallpaperPick::Random
        );
        let mut hidden = Vec::new();
        if !own {
            hidden.push(QuickRow::WallpaperTint);
        }
        if !self.framed_panel() {
            hidden.push(QuickRow::GbaBezel);
        }
        if !self
            .root()
            .is_some_and(|root| crate::core::installed(root, Core::Gambatte))
        {
            hidden.push(QuickRow::GbCore);
        }
        hidden.extend(self.hidden_tv_core_rows());
        // X and Y are Super Nintendo buttons: they show only when there are SNES games.
        if !self.has_games_of(Platform::Snes) {
            hidden.extend([QuickRow::GameX, QuickRow::GameY]);
        }
        if !self.left_stick {
            hidden.push(QuickRow::LeftStick);
        }
        if !self.state.turbo {
            hidden.extend([QuickRow::GameTurboA, QuickRow::GameTurboB]);
        }
        hidden
    }

    /// The NES and SNES core rows show only when the console's alternative core is installed.
    fn hidden_tv_core_rows(&self) -> Vec<QuickRow> {
        [
            (QuickRow::NesCore, Platform::Nes),
            (QuickRow::SnesCore, Platform::Snes),
        ]
        .into_iter()
        .filter(|(_, platform)| {
            let alternative = Core::pair(*platform)[1];
            !self
                .root()
                .is_some_and(|root| crate::core::installed(root, alternative))
        })
        .map(|(row, _)| row)
        .collect()
    }

    fn has_games_of(&self, platform: Platform) -> bool {
        self.shelves.iter().any(|(rack, shelf)| {
            matches!(rack, Rack::Of(p) | Rack::Folder(p) if *p == platform)
                && !shelf.carts.is_empty()
        })
    }

    fn default_core(&self, platform: Platform) -> Core {
        self.root().map_or_else(
            || Core::default_for(platform),
            |root| slot_store::default_core(root, platform),
        )
    }

    /// Swaps a platform's default core for the other in its pair; Game Boy Color follows Game Boy.
    fn flip_default_core(&mut self, row: QuickRow) {
        let Some(root) = self.root().map(Path::to_path_buf) else {
            return;
        };
        let platforms: &[Platform] = match row {
            QuickRow::GbaCore => &[Platform::Gba],
            QuickRow::NesCore => &[Platform::Nes],
            QuickRow::SnesCore => &[Platform::Snes],
            _ => &[Platform::Gb, Platform::Gbc],
        };
        let pair = Core::pair(platforms[0]);
        let to = match self.default_core(platforms[0]) == pair[0] {
            true => pair[1],
            false => pair[0],
        };
        for &platform in platforms {
            let written = slot_store::write_default_core(&root, platform, to);
            if let Err(e) = &written {
                eprintln!("slot: core: could not write the default core: {e}");
            }
            self.card_writes.wrote("selected_core.txt", &written);
        }
    }

    pub fn lid_off_min(&self) -> u8 {
        match (self.state.lid_off_min, &self.power) {
            (Some(min), _) => min,
            (None, Some(power)) => (power.timeout().as_secs() / 60) as u8,
            (None, None) => 3,
        }
    }

    fn random_drawn(&self) -> bool {
        matches!(self.random_wallpaper, Some(Some(_)))
    }

    pub fn set_sticker_face(&mut self, face: TexId) {
        self.sticker_face = Some(face);
    }

    pub fn set_clock_faces(&mut self, line: TexId, hint: TexId) {
        self.clock_faces = Some((line, hint));
    }

    pub fn set_clock_back_face(&mut self, back: (TexId, u32)) {
        self.clock_back = Some(back);
    }

    pub fn set_star_face(&mut self, face: TexId, w: u32, h: u32) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_star(face, w, h);
        }
    }

    fn mark_stars(&mut self) {
        let state = &self.state;
        for (_, shelf) in &mut self.shelves {
            shelf.mark_starred(|c| state.is_favorite(c.platform, &c.stem));
        }
    }

    pub fn set_cart_shadow(&mut self, face: TexId) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_shadow(face);
        }
    }

    pub fn set_gb_cart_shadows(&mut self, notched: TexId, rounded: TexId) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_gb_shadow(GbShell::Notched, notched);
            shelf.set_gb_shadow(GbShell::Rounded, rounded);
        }
    }

    pub fn set_cart_backing(&mut self, face: TexId) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_backing(face);
        }
    }

    pub fn set_gb_cart_backings(&mut self, notched: TexId, rounded: TexId) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_gb_backing(GbShell::Notched, notched);
            shelf.set_gb_backing(GbShell::Rounded, rounded);
        }
    }

    /// The fade shadow of a mould without a setter of its own (the NES and SNES carts).
    pub fn set_mould_shadow(&mut self, mould: slot_ui::Mould, face: TexId) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_mould_shadow(mould, face);
        }
    }

    /// The board behind a clear cart of that mould (the NES and SNES carts).
    pub fn set_mould_backing(&mut self, mould: slot_ui::Mould, face: TexId) {
        for (_, shelf) in &mut self.shelves {
            shelf.set_mould_backing(mould, face);
        }
    }

    pub fn set_ground(&mut self, face: TexId) {
        self.ground = Some(face);
    }

    pub fn set_blur(&mut self, face: TexId) {
        self.blur = Some(face);
    }

    pub fn set_wallpaper(&mut self, face: TexId) {
        self.wallpaper = Some(face);
    }

    pub fn set_bolt_face(&mut self, bolt: TexId) {
        self.bolt = Some(bolt);
    }

    pub fn set_hud_bolt_face(&mut self, bolt: TexId) {
        self.hud_bolt = Some(bolt);
    }

    pub fn set_slot_faces(&mut self, back: TexId, front: TexId) {
        self.slot_faces = Some((back, front));
    }

    pub fn set_shelf_platform_face(&mut self, face: TexId, w: u32) {
        self.shelf_platform = slot_ui::Printed::new(face, w);
    }

    pub fn clear_shelf_platform(&mut self) {
        self.shelf_platform = slot_ui::Printed::default();
    }

    /// The shelf's name has a face to draw; the game's title waits for it after a switch.
    pub fn shelf_platform_drawn(&self) -> bool {
        self.shelf_platform.face.is_some()
    }

    pub fn slot_text(&self) -> Option<String> {
        match self.slot_letter {
            Some(letter) => Some(letter.to_string()),
            None => self.shelf_platform_name().map(str::to_string),
        }
    }

    pub fn title_text(&self) -> Option<String> {
        if !self.state.show_titles {
            return None;
        }
        let shelf = self.shelf();
        shelf.carts.get(shelf.index).map(slot_ui::label_text)
    }

    pub fn set_shelf_title_face(&mut self, face: TexId, w: u32) {
        self.shelf_title = slot_ui::Printed::new(face, w);
    }

    pub fn clear_shelf_title(&mut self) {
        self.shelf_title = slot_ui::Printed::default();
    }

    fn title_alpha(&self) -> f32 {
        if !self.state.show_titles || (self.name_pending && self.slot_text().is_some()) {
            return 0.0;
        }
        let named = self.shelf_named.map(|at| self.now().saturating_sub(at));
        if named.is_some_and(|t| t < SLOT_NAME_IN_MS + SLOT_NAME_HOLD_MS) {
            return 0.0;
        }
        1.0 - self.slot_name_alpha() / SLOT_NAME_ALPHA
    }

    pub fn shelf_platform_name(&self) -> Option<&'static str> {
        self.next_shelf(1)?;
        Some(self.shelves[self.shelf_at].0.name())
    }

    pub fn set_battery_percent_face(&mut self, face: TexId, w: u32) {
        self.battery_percent = slot_ui::Printed::new(face, w);
    }

    pub fn set_shelf_clock_face(&mut self, face: TexId, w: u32) {
        self.shelf_clock = slot_ui::Printed::new(face, w);
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn carts(&self) -> impl Iterator<Item = &Cart> {
        self.shelves
            .iter()
            .filter(|(r, _)| r.platform().is_some())
            .flat_map(|(_, shelf)| {
                shelf
                    .carts
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| shelf.folder_at(*i).is_none())
                    .map(|(_, cart)| cart)
            })
    }

    pub fn seated_cart(&self) -> Option<&Cart> {
        let stem = match &self.phase {
            Phase::Inserting { cart, .. }
            | Phase::Playing { cart }
            | Phase::Ejecting { cart, .. }
            | Phase::Polaroids { cart } => cart,
            Phase::Doze { cart: Some(cart) } => cart,
            _ => return None,
        };
        self.shelf().carts.iter().find(|c| c.stem == *stem)
    }

    pub fn single_cart(&self) -> bool {
        self.carts().count() == 1
    }

    pub fn set_faces(&mut self, faces: Vec<TexId>) {
        let mut faces = faces.into_iter();
        for (_, shelf) in &mut self.shelves {
            let n = shelf.carts.len();
            shelf.set_faces(faces.by_ref().take(n).collect());
        }
    }

    pub fn face_wants(&self) -> Vec<FaceWant> {
        let here = self.shelf();
        let screen = here.on_screen();
        let mut wants: Vec<(usize, usize)> = screen.iter().map(|i| (self.shelf_at, *i)).collect();
        for at in self.neighbour_shelves() {
            wants.extend(self.shelves[at].1.on_screen().into_iter().map(|i| (at, i)));
        }
        wants.extend(
            here.around(FACE_AHEAD)
                .into_iter()
                .filter(|i| !screen.contains(i))
                .map(|i| (self.shelf_at, i)),
        );
        // A folder shows the first three carts of its own shelf.
        let stacked: Vec<(usize, usize)> = wants
            .iter()
            .filter_map(|(at, i)| self.shelves[*at].1.folder_at(*i))
            .flat_map(|f| self.stack_indices(f).into_iter().map(move |k| (f, k)))
            .collect();
        wants.retain(|(at, i)| self.shelves[*at].1.folder_at(*i).is_none());
        wants.extend(stacked);
        wants
            .into_iter()
            .filter(|(at, i)| self.shelves[*at].1.face(*i).is_none())
            .map(|(shelf, index)| FaceWant {
                shelf,
                index,
                on_screen: shelf == self.shelf_at && screen.contains(&index),
            })
            .collect()
    }

    pub fn shelf_epoch(&self) -> u64 {
        self.shelf_epoch
    }

    fn prefs_of(&self, folder: usize) -> Option<&slot_store::FolderPrefs> {
        self.folder_prefs
            .iter()
            .find(|(at, _)| *at == folder)
            .map(|(_, p)| p)
    }

    fn folder_name(&self, folder: usize) -> Option<String> {
        Some(
            self.shelves
                .get(folder)?
                .1
                .carts
                .first()?
                .folder()?
                .to_string(),
        )
    }

    fn folder_platform(&self, folder: usize) -> Option<Platform> {
        self.shelves.get(folder)?.0.platform()
    }

    fn folder_key(&self, folder: usize) -> Option<String> {
        Some(slot_store::folder_key(
            self.folder_platform(folder)?,
            &self.folder_name(folder)?,
        ))
    }

    /// Reads every folder's settings and puts its games in the order it asks for.
    fn load_folders(&mut self) {
        let Some(root) = self.root.clone() else {
            return;
        };
        self.folder_prefs = (0..self.shelves.len())
            .filter(|at| matches!(self.shelves[*at].0, Rack::Folder(_)))
            .filter_map(|at| {
                Some((
                    at,
                    slot_store::read_folder_prefs(&root, &self.folder_key(at)?),
                ))
            })
            .collect();
        for at in self
            .folder_prefs
            .iter()
            .map(|(at, _)| *at)
            .collect::<Vec<_>>()
        {
            self.resort_folder(at);
        }
    }

    /// Changes a folder's settings and writes them to the card.
    fn change_prefs(&mut self, folder: usize, change: impl FnOnce(&mut slot_store::FolderPrefs)) {
        let Some(key) = self.folder_key(folder) else {
            return;
        };
        let Some((_, prefs)) = self.folder_prefs.iter_mut().find(|(at, _)| *at == folder) else {
            return;
        };
        change(prefs);
        if let Some(root) = &self.root {
            if let Err(e) = slot_store::write_folder_prefs(root, &key, prefs) {
                eprintln!("slot: folders: {key}: {e}");
            }
        }
    }

    /// Puts a folder's games in its chosen order, keeping the selected one selected.
    fn resort_folder(&mut self, folder: usize) {
        let Some(sort) = self.prefs_of(folder).map(|p| p.sort) else {
            return;
        };
        let recents: Vec<(Platform, String)> = self
            .state
            .recents
            .iter()
            .map(|r| (r.platform, r.stem.clone()))
            .collect();
        let shelf = &self.shelves[folder].1;
        let selected = shelf.carts.get(shelf.index).map(|c| c.stem.clone());
        let mut carts = shelf.carts.clone();
        let az = |c: &Cart| slot_store::sort_key(&c.stem);
        match sort {
            slot_store::FolderSort::Az => carts.sort_by_key(az),
            slot_store::FolderSort::Platform => carts.sort_by_key(|c| (c.platform as u8, az(c))),
            slot_store::FolderSort::Recent => carts.sort_by_key(|c| {
                let played = recents
                    .iter()
                    .position(|(p, s)| *p == c.platform && *s == c.stem);
                (played.unwrap_or(usize::MAX), az(c))
            }),
        }
        if carts
            .iter()
            .map(|c| &c.stem)
            .eq(shelf.carts.iter().map(|c| &c.stem))
        {
            return;
        }
        let old = self.shelves[folder].1.restock(carts);
        self.orphaned.extend(old);
        let shelf = &mut self.shelves[folder].1;
        if let Some(i) = selected.and_then(|s| shelf.carts.iter().position(|c| c.stem == s)) {
            shelf.select(i);
        }
        self.shelf_epoch += 1;
    }

    /// The folder's carts its stack shows: the cover in front, then the next ones on its shelf.
    fn stack_indices(&self, folder: usize) -> Vec<usize> {
        let carts = &self.shelves[folder].1.carts;
        let cover = self
            .prefs_of(folder)
            .and_then(|p| p.cover.as_deref())
            .and_then(|stem| carts.iter().position(|c| c.stem == stem))
            .unwrap_or(0);
        std::iter::once(cover)
            .chain((0..carts.len()).filter(|i| *i != cover))
            .take(STACK_CARTS)
            .collect()
    }

    /// The paper a folder's name is written on: its own platform's, or the one it picked.
    fn paper_of(&self, folder: usize) -> Option<Platform> {
        let own = self.folder_platform(folder)?;
        Some(
            match self.prefs_of(folder).map(|p| p.paper).unwrap_or_default() {
                slot_store::FolderPaper::Auto => own,
                slot_store::FolderPaper::Light => Platform::Gba,
                slot_store::FolderPaper::Cuts => Platform::Gb,
                slot_store::FolderPaper::Deep => Platform::Gbc,
            },
        )
    }

    fn open_folder_menu(&mut self) {
        let Some(folder) = self.selected_folder() else {
            return;
        };
        let now = self.now();
        self.folder_menu = Some(FolderMenu {
            parent: self.shelf_at,
            item: self.shelf().index,
            folder,
            row: 0,
            first: 0,
            opened: now,
            closing: None,
            glide: (0.0, now),
            entry: None,
        });
        self.folder_sheet_stale = true;
    }

    fn folder_menu_rows(&self, folder: usize) -> Vec<slot_ui::MenuRow> {
        let prefs = self.prefs_of(folder).cloned().unwrap_or_default();
        let platform = self.folder_platform(folder).unwrap_or(Platform::Gba);
        let carts = &self.shelves[folder].1.carts;
        let cover = self
            .stack_indices(folder)
            .first()
            .and_then(|i| carts.get(*i));
        let core = self
            .root
            .as_ref()
            .and_then(|root| slot_store::folder_core(root, &self.folder_key(folder)?, platform));
        FolderRow::ALL
            .iter()
            .map(|row| {
                let value = match row {
                    FolderRow::Rename => String::new(),
                    FolderRow::Cover => cover
                        .map(|c| slot_ui::clean_label(&c.stem))
                        .unwrap_or_default(),
                    FolderRow::Sort => prefs.sort.text().to_string(),
                    FolderRow::Core => core.map_or("Default", |c| c.text()).to_string(),
                    FolderRow::Open => if prefs.open_last { "Last" } else { "First" }.to_string(),
                    FolderRow::Paper => prefs.paper.text().to_string(),
                };
                slot_ui::MenuRow {
                    label: row.label().to_string(),
                    value,
                    arrows: *row != FolderRow::Rename,
                }
            })
            .collect()
    }

    fn folder_menu_input(&mut self, action: Action) {
        let Some(menu) = self.folder_menu.clone() else {
            return;
        };
        if menu.closing.is_some() {
            return;
        }
        if menu.entry.is_some() {
            return self.name_entry_input(action);
        }
        let row = FolderRow::ALL[menu.row];
        match action {
            Action::GbaDown(Btn::Up) => self.move_folder_row(-1),
            Action::GbaDown(Btn::Down) => self.move_folder_row(1),
            Action::GbaDown(Btn::Left) => self.change_folder_row(row, -1),
            Action::GbaDown(Btn::Right) => self.change_folder_row(row, 1),
            Action::GbaDown(Btn::A) if row == FolderRow::Rename => {
                let name = self.folder_name(menu.folder).unwrap_or_default();
                if let Some(m) = &mut self.folder_menu {
                    m.entry = Some(NameEntry {
                        name,
                        lower: false,
                        cursor: (0, 0),
                    });
                }
                self.folder_sheet_stale = true;
            }
            Action::GbaDown(Btn::A) => self.change_folder_row(row, 1),
            Action::GbaDown(Btn::B) | Action::GbaDown(Btn::Start) => {
                let now = self.now();
                if let Some(m) = &mut self.folder_menu {
                    m.closing = Some(now);
                }
            }
            _ => {}
        }
    }

    fn move_folder_row(&mut self, by: i32) {
        let now = self.now();
        let Some(m) = &mut self.folder_menu else {
            return;
        };
        let rows = FolderRow::ALL.len() as i32;
        let was = (m.row - m.first) as f32;
        let from = glide_at(m.glide, was, now);
        m.row = (m.row as i32 + by).rem_euclid(rows) as usize;
        let shown = slot_ui::MENU_ROWS_SHOWN;
        let first = m.first.clamp(m.row.saturating_sub(shown - 1), m.row);
        if first != m.first {
            m.first = first;
            self.folder_sheet_stale = true;
        }
        let shift = m.first as f32 - (m.row as f32 - (m.row - m.first) as f32);
        m.glide = (from - shift, now);
    }

    fn change_folder_row(&mut self, row: FolderRow, by: i32) {
        let Some(folder) = self.folder_menu.as_ref().map(|m| m.folder) else {
            return;
        };
        let step = |at: usize, n: usize| (at as i32 + by).rem_euclid(n as i32) as usize;
        match row {
            FolderRow::Rename => return,
            FolderRow::Cover => {
                let carts = &self.shelves[folder].1.carts;
                let now = self.stack_indices(folder).first().copied().unwrap_or(0);
                let stem = carts
                    .get(step(now, carts.len().max(1)))
                    .map(|c| c.stem.clone());
                self.change_prefs(folder, |p| p.cover = stem);
            }
            FolderRow::Sort => {
                let all = slot_store::FolderSort::ALL;
                self.change_prefs(folder, |p| {
                    let at = all.iter().position(|s| *s == p.sort).unwrap_or(0);
                    p.sort = all[step(at, all.len())];
                });
                self.resort_folder(folder);
            }
            FolderRow::Core => {
                let (Some(root), Some(key), Some(platform)) = (
                    self.root.clone(),
                    self.folder_key(folder),
                    self.folder_platform(folder),
                ) else {
                    return;
                };
                let mut choices = vec![None];
                choices.extend(
                    Core::pair(platform)
                        .into_iter()
                        .filter(|c| crate::core::installed(&root, *c))
                        .map(Some),
                );
                let now = slot_store::folder_core(&root, &key, platform);
                let at = choices.iter().position(|c| *c == now).unwrap_or(0);
                let to = choices[step(at, choices.len())];
                if let Err(e) = slot_store::write_folder_core(&root, &key, to) {
                    eprintln!("slot: folders: {key}: core: {e}");
                }
            }
            FolderRow::Open => self.change_prefs(folder, |p| p.open_last = !p.open_last),
            FolderRow::Paper => {
                let all = slot_store::FolderPaper::ALL;
                self.change_prefs(folder, |p| {
                    let at = all.iter().position(|v| *v == p.paper).unwrap_or(0);
                    p.paper = all[step(at, all.len())];
                });
                self.drop_folder_papers(folder);
            }
        }
        self.folder_sheet_stale = true;
    }

    /// Lets go of a folder's paper and name strip, to be drawn again on its new paper or name.
    fn drop_folder_papers(&mut self, folder: usize) {
        for at in 0..self.shelves.len() {
            for i in 0..self.shelves[at].1.carts.len() {
                if self.shelves[at].1.folder_at(i) != Some(folder) {
                    continue;
                }
                let stack = self.shelves[at].1.stack(i).copied().unwrap_or_default();
                self.orphaned.extend(stack.paper);
                self.shelves[at].1.set_stack(
                    i,
                    Stack {
                        paper: None,
                        ..stack
                    },
                );
            }
        }
        let (gone, kept): (Vec<_>, Vec<_>) = self
            .folder_titles
            .drain(..)
            .partition(|(at, _, _)| *at == folder);
        self.orphaned
            .extend(gone.into_iter().map(|(_, tex, _)| tex));
        self.folder_titles = kept;
    }

    fn name_entry_input(&mut self, action: Action) {
        let Some(entry) = self.folder_menu.as_mut().and_then(|m| m.entry.as_mut()) else {
            return;
        };
        let mut done = None;
        match action {
            Action::GbaDown(Btn::Up) => entry.cursor = slot_ui::step_cursor(entry.cursor, 0, -1),
            Action::GbaDown(Btn::Down) => entry.cursor = slot_ui::step_cursor(entry.cursor, 0, 1),
            Action::GbaDown(Btn::Left) => entry.cursor = slot_ui::step_cursor(entry.cursor, -1, 0),
            Action::GbaDown(Btn::Right) => entry.cursor = slot_ui::step_cursor(entry.cursor, 1, 0),
            Action::GbaDown(Btn::Start) => entry.cursor = slot_ui::END_KEY,
            Action::GbaDown(Btn::Select) => entry.lower = !entry.lower,
            Action::GbaDown(Btn::B) => {
                if entry.name.pop().is_none() {
                    done = Some(None);
                }
            }
            Action::GbaDown(Btn::A) => match slot_ui::entry_key(entry.lower, entry.cursor) {
                slot_ui::EntryKey::Char(c) => {
                    if entry.name.chars().count() < slot_ui::NAME_MAX {
                        entry.name.push(c);
                    }
                }
                slot_ui::EntryKey::Letters => entry.lower = !entry.lower,
                slot_ui::EntryKey::End => done = Some(Some(entry.name.trim().to_string())),
            },
            _ => return,
        }
        self.folder_sheet_stale = true;
        let Some(named) = done else {
            return;
        };
        let folder = self.folder_menu.as_ref().map(|m| m.folder);
        if let (Some(name), Some(folder)) = (named, folder) {
            if !name.is_empty() && Some(&name) != self.folder_name(folder).as_ref() {
                if let Err(e) = self.rename_folder(folder, &name) {
                    eprintln!("slot: folders: rename to {name:?}: {e}");
                    return self.refuse();
                }
            }
        }
        if let Some(m) = &mut self.folder_menu {
            m.entry = None;
        }
    }

    /// Renames the folder on the card and everywhere the shelves keep its path.
    fn rename_folder(&mut self, folder: usize, name: &str) -> std::io::Result<()> {
        let (Some(root), Some(old_key), Some(platform)) = (
            self.root.clone(),
            self.folder_key(folder),
            self.folder_platform(folder),
        ) else {
            return Err(std::io::ErrorKind::NotFound.into());
        };
        let Some(old) = self.shelves[folder]
            .1
            .carts
            .first()
            .and_then(|c| c.rom.parent().map(Path::to_path_buf))
        else {
            return Err(std::io::ErrorKind::NotFound.into());
        };
        let new = old.with_file_name(name);
        if new.exists() {
            return Err(std::io::ErrorKind::AlreadyExists.into());
        }
        std::fs::rename(&old, &new)?;
        let new_key = slot_store::folder_key(platform, name);
        if let Err(e) = slot_store::rename_folder_prefs(&root, &old_key, &new_key, platform) {
            eprintln!("slot: folders: {old_key} -> {new_key}: {e}");
        }
        for (_, shelf) in self.shelves.iter_mut() {
            for cart in shelf.carts.iter_mut() {
                if cart.rom == old {
                    cart.rom = new.clone();
                    cart.stem = name.to_string();
                    cart.title = name.to_string();
                } else if cart.rom.parent() == Some(old.as_path()) {
                    if let Some(file) = cart.rom.file_name() {
                        cart.rom = new.join(file);
                    }
                }
            }
        }
        self.drop_folder_papers(folder);
        self.resort_parent_of(folder);
        Ok(())
    }

    /// Puts a renamed folder back in name order on its platform's shelf.
    fn resort_parent_of(&mut self, folder: usize) {
        let Some((at, _)) = self.shelves.iter().enumerate().find_map(|(at, (_, s))| {
            (0..s.carts.len())
                .find(|i| s.folder_at(*i) == Some(folder))
                .map(|i| (at, i))
        }) else {
            return;
        };
        let shelf = &self.shelves[at].1;
        let links: Vec<(PathBuf, usize)> = (0..shelf.carts.len())
            .filter_map(|i| Some((shelf.carts[i].rom.clone(), shelf.folder_at(i)?)))
            .collect();
        let mut carts = shelf.carts.clone();
        carts.sort_by_key(|c| {
            let folder = links.iter().any(|(rom, _)| *rom == c.rom);
            (!folder, slot_store::sort_key(&c.stem))
        });
        let old = self.shelves[at].1.restock(carts);
        self.orphaned.extend(old);
        let shelf = &mut self.shelves[at].1;
        for i in 0..shelf.carts.len() {
            if let Some((_, f)) = links.iter().find(|(rom, _)| *rom == shelf.carts[i].rom) {
                shelf.set_folder(i, *f);
            }
        }
        let item = (0..shelf.carts.len()).find(|i| shelf.folder_at(*i) == Some(folder));
        if let Some(i) = item {
            shelf.select(i);
        }
        if let (Some(m), Some(i)) = (&mut self.folder_menu, item) {
            m.item = i;
        }
        self.shelf_epoch += 1;
    }

    /// The menu or name entry sheet to draw again: its paper, the folder's name, and either the
    /// rows from the first shown or the name so far.
    pub fn folder_sheet_want(&mut self) -> Option<FolderSheet> {
        if !std::mem::take(&mut self.folder_sheet_stale) {
            return None;
        }
        let m = self.folder_menu.as_ref()?;
        let paper = self.paper_of(m.folder)?;
        Some(match &m.entry {
            Some(e) => FolderSheet::Entry {
                paper,
                name: e.name.clone(),
                lower: e.lower,
            },
            None => FolderSheet::Menu {
                paper,
                name: self.folder_name(m.folder).unwrap_or_default(),
                rows: self.folder_menu_rows(m.folder),
                first: m.first,
            },
        })
    }

    pub fn set_folder_sheet(&mut self, entry: bool, tex: TexId) {
        let slot = match entry {
            true => &mut self.folder_entry_face,
            false => &mut self.folder_menu_face,
        };
        if let Some(old) = slot.replace(tex) {
            self.orphaned.push(old);
        }
    }

    pub fn wants_folder_ui(&self) -> bool {
        self.folder_menu_loop.is_none()
            && self
                .shelves
                .iter()
                .any(|(r, _)| matches!(r, Rack::Folder(_)))
    }

    pub fn set_folder_ui(
        &mut self,
        menu_loop: TexId,
        key_loops: [TexId; 2],
        legend: Vec<(TexId, u32)>,
        entry_legend: Vec<(TexId, u32)>,
    ) {
        self.folder_menu_loop = Some(menu_loop);
        self.folder_key_loops = key_loops.map(Some);
        self.folder_legend = legend;
        self.entry_legend = entry_legend;
    }

    /// The menu over the shelf: neighbours away, the stack parked small at the top, its paper
    /// grown into the sheet and its band into the loop; or Rename's name entry.
    fn draw_folder_menu(&self, out: &mut Vec<Draw>) {
        let Some(m) = &self.folder_menu else {
            return;
        };
        let canvas = self.canvas();
        let now = self.now();
        let open = match m.closing {
            Some(at) => 1.0 - (now.saturating_sub(at) as f32 / FOLDER_MENU_CLOSE_MS).min(1.0),
            None => (now.saturating_sub(m.opened) as f32 / FOLDER_MENU_OPEN_MS).min(1.0),
        };
        let open = ease(open);
        let parent = &self.shelves[m.parent].1;
        let Some(cover) = parent.carts.get(m.item) else {
            return;
        };
        let platform = cover.platform;
        parent.draw_row(canvas, Some(&cover.stem), 0.0, open, 1.0, out);

        let (rest, scale) = parent.selected_at(canvas);
        let shelf_slot = shelf_cart_of(platform, rest, scale);
        let parked = Placed {
            x: canvas.w / 2.0 - shelf_slot.w * FOLDER_PARKED_SCALE / scale / 2.0,
            y: FOLDER_PARKED_Y - shelf_slot.h * FOLDER_PARKED_SCALE / scale / 2.0,
            w: shelf_slot.w * FOLDER_PARKED_SCALE / scale,
            h: shelf_slot.h * FOLDER_PARKED_SCALE / scale,
        };
        let slot = lerp_placed(shelf_slot, parked, open);
        let stack = parent.stack(m.item).copied().unwrap_or_default();
        slot_ui::draw_stack(
            platform,
            slot,
            &Stack {
                paper: None,
                band: None,
                ..stack
            },
            1.0,
            out,
        );

        let poses = stack_poses(platform, shelf_slot);
        let sheet = slot_ui::menu_pose(canvas);
        let paper = Pose {
            at: lerp_placed(poses.paper.at, sheet.at, open),
            turn: poses.paper.turn + (sheet.turn - poses.paper.turn) * open,
        };
        if let Some(tex) = stack.paper {
            draw_pose(paper, tex, 1.0 - open, out);
        }
        let entering = m.entry.is_some();
        if let Some(tex) = self.folder_menu_face.filter(|_| !entering) {
            draw_pose(paper, tex, open, out);
        }
        let slot_now = glide_at(m.glide, (m.row - m.first) as f32, now);
        let ring = slot_ui::menu_loop_pose(canvas, slot_now);
        let band = Pose {
            at: lerp_placed(poses.band.at, ring.at, open),
            turn: poses.band.turn + (ring.turn - poses.band.turn) * open,
        };
        if let Some(tex) = stack.band {
            draw_pose(band, tex, 1.0 - open, out);
        }
        if let Some(tex) = self.folder_menu_loop.filter(|_| !entering) {
            draw_pose(band, tex, open, out);
        }
        if let Some(e) = &m.entry {
            if let Some(tex) = self.folder_entry_face {
                draw_pose(slot_ui::entry_pose(canvas), tex, 1.0, out);
            }
            let (pose, wide) = slot_ui::entry_loop_pose(canvas, e.cursor);
            if let Some(tex) = self.folder_key_loops[usize::from(wide)] {
                draw_pose(pose, tex, 1.0, out);
            }
            self.draw_legend_row(&self.entry_legend, &[true; 4], 1.0, out);
            return;
        }
        let changes = FolderRow::ALL[m.row] != FolderRow::Rename;
        self.draw_legend_row(&self.folder_legend, &[true, changes, true], open, out);
    }

    /// A centred row of footer legends at the core picker's height, those `shown`.
    fn draw_legend_row(
        &self,
        faces: &[(TexId, u32)],
        shown: &[bool],
        alpha: f32,
        out: &mut Vec<Draw>,
    ) {
        let seen = |w: u32| w.saturating_sub(HINT_EDGE) as f32;
        let row: Vec<&(TexId, u32)> = faces
            .iter()
            .zip(shown)
            .filter(|(_, s)| **s)
            .map(|(f, _)| f)
            .collect();
        let total: f32 = row.iter().map(|(_, w)| seen(*w)).sum::<f32>()
            + CORE_LEGEND_GAP * row.len().saturating_sub(1) as f32;
        let mut x = self.canvas().centred(total);
        for (tex, w) in row {
            out.push(Draw::Tex {
                x: x.round(),
                y: CORE_LEGEND_Y,
                w: *w as f32,
                h: MOUTH_H,
                tex: *tex,
                alpha,
            });
            x += seen(*w) + CORE_LEGEND_GAP;
        }
    }

    fn on_extras(&self) -> bool {
        self.shelf_at == EXTRAS_AT && matches!(self.phase, Phase::Shelf)
    }

    fn selected_extra(&self) -> slot_ui::Extra {
        slot_ui::Extra::ALL[self.extra_pos.rem_euclid(3) as usize]
    }

    fn extra_key(item: slot_ui::Extra) -> &'static str {
        match item {
            slot_ui::Extra::Activity => "activity",
            slot_ui::Extra::Lucky => "lucky",
            slot_ui::Extra::Unplayed => "unplayed",
        }
    }

    /// Whether a game was ever started: a save or state on the card, a recent, or play logged.
    fn ever_played(&self, cart: &Cart, logged: &[slot_store::Session]) -> bool {
        let game = Recent {
            platform: cart.platform,
            stem: cart.stem.clone(),
        };
        if self.state.recents.contains(&game) || logged.iter().any(|s| s.game == game) {
            return true;
        }
        let Some(root) = &self.root else {
            return false;
        };
        let saves = crate::root::saves_dir(root).join(cart.platform.dir_name());
        ["srm", "sav"]
            .iter()
            .any(|ext| saves.join(format!("{}.{ext}", cart.stem)).is_file())
    }

    /// The games an Extras pick draws from, for its system setting.
    fn extra_pool(&self, item: slot_ui::Extra) -> Vec<Recent> {
        let system = self
            .root
            .as_ref()
            .and_then(|root| slot_store::extras_system(root, Self::extra_key(item)));
        let logged = self
            .root
            .as_ref()
            .map(|r| slot_store::read_activity(r))
            .unwrap_or_default();
        self.carts()
            .filter(|c| system.is_none_or(|p| c.platform == p))
            .filter(|c| item != slot_ui::Extra::Unplayed || !self.ever_played(c, &logged))
            .map(|c| Recent {
                platform: c.platform,
                stem: c.stem.clone(),
            })
            .collect()
    }

    /// A on an Extras item. Get lucky and Never played put a game from the deck straight in.
    fn open_extra(&mut self) {
        let item = self.selected_extra();
        if item == slot_ui::Extra::Activity {
            return self.open_report();
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let pool = self.extra_pool(item);
        let mut deck = slot_store::read_suggested(&root);
        let roll = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let Some(game) = slot_store::pick(&pool, &mut deck, roll) else {
            return self.refuse();
        };
        if let Err(e) = slot_store::write_suggested(&root, &deck) {
            eprintln!("slot: extras: {e}");
        }
        self.extras_stale = true;
        self.extra_motion = Some((item, self.now(), game));
    }

    /// How long a pick plays before its game goes in: the dice fall, or the bubble tears off.
    fn extra_motion_ms(item: slot_ui::Extra) -> f32 {
        match item {
            slot_ui::Extra::Lucky => 950.0,
            _ => 750.0,
        }
    }

    /// Once a pick has played out, its game starts at once, unseen, as a game left in the slot
    /// does at power on. It goes in from its own shelf, which is where ejecting comes back to.
    fn finish_extra_motion(&mut self) {
        let Some((item, start, _)) = &self.extra_motion else {
            return;
        };
        if (self.now().saturating_sub(*start) as f32) < Self::extra_motion_ms(*item) {
            return;
        }
        let Some((_, _, game)) = self.extra_motion.take() else {
            return;
        };
        let Some((at, i)) = self.seat_of(&game.stem, Some(game.platform)) else {
            return;
        };
        self.go_to_shelf(at);
        self.shelves[at].1.select(i);
        self.insert(!self.state.eject_save);
        if let Phase::Inserting { resumed, t, .. } = &mut self.phase {
            *resumed = true;
            *t = INSERT_S;
            self.straight_in = true;
            self.screen = 1.0;
            self.lift = 1.0;
        }
    }

    /// The receipt's week and the blister pack's count, to draw them again when stale.
    pub fn extras_want(&mut self) -> Option<(ExtrasWeek, usize)> {
        if !self.extras_stale || self.root.is_none() {
            return None;
        }
        self.extras_stale = false;
        let root = self.root.clone()?;
        let offset = i32::from(self.state.utc_offset_min);
        let now = self.power.as_ref().map_or_else(system_secs, |p| p.now());
        let today = slot_store::local_day(now, offset);
        let a = slot_store::activity(&slot_store::read_activity(&root), today, offset);
        let monday = today - slot_store::weekday(today) as i64;
        let (_, m0, d0) = slot_store::civil(monday);
        let (_, m1, d1) = slot_store::civil(monday + 6);
        let range = match m0 == m1 {
            true => format!("{} {d0}–{d1}", MONTHS[m0 as usize - 1]),
            false => format!(
                "{} {d0}–{} {d1}",
                MONTHS[m0 as usize - 1],
                MONTHS[m1 as usize - 1]
            ),
        };
        let week = ExtrasWeek {
            range,
            days: a.week,
            labels: a.week.map(|s| {
                if s >= 60 {
                    slot_store::play_time_text(s)
                } else {
                    String::new()
                }
            }),
            total: slot_store::play_time_text(a.this_week),
        };
        Some((week, self.extra_pool(slot_ui::Extra::Unplayed).len()))
    }

    pub fn wants_extra_parts(&self) -> bool {
        self.extra_bubble.is_none()
    }

    pub fn set_extra_parts(&mut self, dice: [Option<TexId>; 3], bubble: Option<TexId>) {
        self.extra_dice = dice;
        self.extra_bubble = bubble;
    }

    pub fn set_extra_faces(&mut self, faces: [Option<TexId>; 3]) {
        for (slot, tex) in self.extra_faces.iter_mut().zip(faces) {
            if let Some(tex) = tex {
                if let Some(old) = slot.replace(tex) {
                    self.orphaned.push(old);
                }
            }
        }
    }

    /// The Extras row: the selected item centred, its neighbours half off the screen.
    fn draw_extras(&self, out: &mut Vec<Draw>) {
        let canvas = self.canvas();
        let at = self.extra_scroll.round() as i64;
        for pos in [at - 1, at + 1, at] {
            let offset = pos as f32 - self.extra_scroll;
            let item = slot_ui::Extra::ALL[pos.rem_euclid(3) as usize];
            let Some(tex) = self.extra_faces[pos.rem_euclid(3) as usize] else {
                continue;
            };
            let mut at = slot_ui::extra_box(canvas, item, offset);
            let (aside, shown) = self.report_aside(offset);
            at.x += aside;
            let alpha = self.extra_fade() * shown;
            slot_ui::draw_extra(item, at, tex, alpha, out);
            self.draw_extra_parts(item, at, alpha, out);
        }
    }

    /// The dice resting on the slate, and the bubble over the blister pack's card.
    fn draw_extra_parts(&self, item: slot_ui::Extra, at: Placed, alpha: f32, out: &mut Vec<Draw>) {
        // How far the pick has come, when this item is the one picking.
        let moved = match &self.extra_motion {
            Some((picking, start, _)) if *picking == item => {
                self.now().saturating_sub(*start) as f32
            }
            _ => 0.0,
        };
        let k = at.w / slot_ui::extra_box(self.canvas(), item, 0.0).w;
        match item {
            slot_ui::Extra::Lucky => {
                // Off the slate and down the screen, each spinning its own way (design: 760 ms).
                const FALL: [(f32, f32, f32); 3] = [
                    (-60.0, 330.0, 760.0),
                    (10.0, 310.0, -980.0),
                    (70.0, 340.0, 1120.0),
                ];
                let t = ease_out((moved / 760.0).min(1.0));
                for (die, tex) in self.extra_dice.iter().enumerate() {
                    let Some(tex) = tex else {
                        continue;
                    };
                    let rest = slot_ui::die_rest(at, die);
                    let (dx, dy, spin) = FALL[die];
                    let pose = Pose {
                        at: Placed {
                            x: rest.x + dx * k * t,
                            y: rest.y + dy * k * t,
                            ..rest
                        },
                        turn: (spin * t).to_radians(),
                    };
                    draw_pose(pose, *tex, alpha, out);
                }
            }
            slot_ui::Extra::Unplayed => {
                let Some(tex) = self.extra_bubble else {
                    return;
                };
                // Torn off up and to the right (design: 420 ms).
                let t = ease_out((moved / 420.0).min(1.0));
                let mut pose = slot_ui::bubble_pose(at);
                pose.at.x += 150.0 * k * t;
                pose.at.y -= 230.0 * k * t;
                pose.turn += 38f32.to_radians() * t;
                draw_pose(pose, tex, alpha * (1.0 - t), out);
            }
            slot_ui::Extra::Activity => {}
        }
    }

    /// The row fades as a pick finishes: from 600 ms for the dice, 420 ms for the bubble.
    fn extra_fade(&self) -> f32 {
        let Some((item, start, _)) = &self.extra_motion else {
            return 1.0;
        };
        let from = match item {
            slot_ui::Extra::Lucky => 600.0,
            _ => 420.0,
        };
        let gone = self.now().saturating_sub(*start) as f32 - from;
        1.0 - (gone / (Self::extra_motion_ms(*item) - from)).clamp(0.0, 1.0)
    }

    fn selected_game(&self) -> Option<Recent> {
        let cart = self.shelf().carts.get(self.shelf().index)?;
        Some(Recent {
            platform: cart.platform,
            stem: cart.stem.clone(),
        })
    }

    /// The game whose delete prompt needs its faces: its name, to ask about it by name.
    pub fn game_delete_want(&self) -> Option<String> {
        let game = self
            .game_delete
            .as_ref()
            .filter(|_| self.game_delete_faces.is_empty())?;
        Some(slot_ui::clean_label(&game.stem))
    }

    pub fn set_game_delete_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.game_delete_faces = faces;
    }

    fn forget_game_delete(&mut self) {
        self.game_delete = None;
        let faces = std::mem::take(&mut self.game_delete_faces);
        self.orphaned
            .extend(faces.into_iter().map(|(tex, _, _)| tex));
    }

    /// Takes a game off the card: its rom, its label and cheats, its own settings and every list
    /// that names it. Its saves and states stay. A folder left without games goes with it.
    fn delete_game(&mut self, game: &Recent) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let Some(cart) = self
            .carts()
            .find(|c| c.platform == game.platform && c.stem == game.stem)
            .cloned()
        else {
            return;
        };
        if let Err(e) = std::fs::remove_file(&cart.rom) {
            eprintln!("slot: delete {}: {e}", cart.rom.display());
            return self.refuse();
        }
        let system = cart.platform.dir_name();
        let extras = [
            cart.label.clone(),
            Some(
                root.join("Cheats")
                    .join(system)
                    .join(format!("{}.cht", cart.stem)),
            ),
        ];
        for path in extras.into_iter().flatten() {
            if path.is_file() {
                if let Err(e) = std::fs::remove_file(&path) {
                    eprintln!("slot: delete {}: {e}", path.display());
                }
            }
        }
        let picture = crate::video_mode::video_mode_key(&cart.stem, cart.platform);
        for (file, key) in [
            (slot_store::SELECTED_CORE_FILE, cart.stem.as_str()),
            (crate::video_mode::VIDEO_MODE_FILE, picture.as_str()),
        ] {
            if let Err(e) = slot_store::ini::put(&root, file, key, None) {
                eprintln!("slot: delete: {file}: {e}");
            }
        }
        let mut deck = slot_store::read_suggested(&root);
        deck.retain(|g| g != game);
        if let Err(e) = slot_store::write_suggested(&root, &deck) {
            eprintln!("slot: delete: {e}");
        }
        self.state.recents.retain(|g| g != game);
        self.state.favorites.retain(|g| g != game);
        if self.state.shelf_cart.as_ref() == Some(game) {
            self.state.shelf_cart = None;
        }
        self.persist();
        if let Some(dir) = cart.rom.parent().filter(|_| cart.folder().is_some()) {
            // Only an empty folder goes; anything else in it keeps it.
            let _ = std::fs::remove_dir(dir);
        }
        self.core_picker = None;
        self.take_off_shelves(game);
        self.restock_recents();
        self.restock_favorites();
        self.extras_stale = true;
    }

    /// Takes a deleted game off its shelf, and a folder it emptied off the shelf it stood on.
    fn take_off_shelves(&mut self, game: &Recent) {
        let seats: Vec<(usize, usize)> = (0..self.shelves.len())
            .filter(|at| self.shelves[*at].0.platform().is_some())
            .filter_map(|at| {
                let shelf = &self.shelves[at].1;
                (0..shelf.carts.len())
                    .find(|i| {
                        shelf.folder_at(*i).is_none()
                            && shelf.carts[*i].platform == game.platform
                            && shelf.carts[*i].stem == game.stem
                    })
                    .map(|i| (at, i))
            })
            .collect();
        for (at, i) in seats {
            self.remove_from_shelf(at, i);
            if self.shelves[at].1.carts.is_empty() && matches!(self.shelves[at].0, Rack::Folder(_))
            {
                let parent = self.shelves.iter().enumerate().find_map(|(p, (_, s))| {
                    (0..s.carts.len())
                        .find(|j| s.folder_at(*j) == Some(at))
                        .map(|j| (p, j))
                });
                if let Some((p, j)) = parent {
                    if self.shelf_at == at {
                        self.go_to_shelf(p);
                    }
                    self.remove_from_shelf(p, j);
                }
            }
        }
        if self.shelf().carts.is_empty() && self.shelf_at != EXTRAS_AT {
            let to = self.next_shelf(1).unwrap_or(0);
            self.go_to_shelf(to);
        }
    }

    /// Takes the cart at `i` off shelf `at`, keeping its folders tied to their games.
    fn remove_from_shelf(&mut self, at: usize, i: usize) {
        let shelf = &self.shelves[at].1;
        let links: Vec<(PathBuf, usize)> = (0..shelf.carts.len())
            .filter_map(|k| Some((shelf.carts[k].rom.clone(), shelf.folder_at(k)?)))
            .collect();
        let mut carts = shelf.carts.clone();
        carts.remove(i);
        let old = self.shelves[at].1.restock(carts);
        self.orphaned.extend(old);
        let shelf = &mut self.shelves[at].1;
        for k in 0..shelf.carts.len() {
            if let Some((_, f)) = links.iter().find(|(rom, _)| *rom == shelf.carts[k].rom) {
                shelf.set_folder(k, *f);
            }
        }
        if !shelf.carts.is_empty() {
            shelf.select(i.min(shelf.carts.len() - 1));
        }
        self.shelf_epoch += 1;
    }

    /// Folders on any shelf still without their paper: shelf, entry, platform and name.
    pub fn folder_paper_wants(&self) -> Vec<(usize, usize, Platform, Platform, String)> {
        let mut out = Vec::new();
        for (at, (_, shelf)) in self.shelves.iter().enumerate() {
            for (i, cart) in shelf.carts.iter().enumerate() {
                let bare = shelf.stack(i).is_some_and(|s| s.paper.is_none());
                if let (Some(f), true) = (shelf.folder_at(i), bare) {
                    let paper = self.paper_of(f).unwrap_or(cart.platform);
                    out.push((at, i, cart.platform, paper, cart.stem.clone()));
                }
            }
        }
        out
    }

    pub fn set_folder_paper(&mut self, shelf: usize, index: usize, tex: TexId) {
        if let Some((_, s)) = self.shelves.get_mut(shelf) {
            let stack = s.stack(index).copied().unwrap_or_default();
            s.set_stack(
                index,
                Stack {
                    paper: Some(tex),
                    ..stack
                },
            );
        }
    }

    pub fn wants_folder_band(&self) -> bool {
        self.folder_band.is_none()
            && self
                .shelves
                .iter()
                .any(|(r, _)| matches!(r, Rack::Folder(_)))
    }

    pub fn set_folder_band(&mut self, tex: TexId) {
        self.folder_band = Some(tex);
    }

    /// The opened folder, when its name strip is still to be made: its shelf and name.
    pub fn folder_title_want(&self) -> Option<(usize, String)> {
        if !self.in_folder()
            || self
                .folder_titles
                .iter()
                .any(|(at, _, _)| *at == self.shelf_at)
        {
            return None;
        }
        let name = self.shelf().carts.first()?.folder()?.to_string();
        Some((self.shelf_at, name))
    }

    pub fn set_folder_title(&mut self, shelf: usize, tex: TexId, w: u32) {
        self.folder_titles.push((shelf, tex, w));
    }

    /// Copies each folder's first carts' faces onto its stack, as they come and go.
    pub fn refresh_folder_stacks(&mut self) {
        let band = self.folder_band;
        for at in 0..self.shelves.len() {
            for i in 0..self.shelves[at].1.carts.len() {
                let Some(f) = self.shelves[at].1.folder_at(i) else {
                    continue;
                };
                let picks = self.stack_indices(f);
                let folder = &self.shelves[f].1;
                let carts = [0, 1, 2].map(|k| picks.get(k).and_then(|i| folder.face(*i)));
                let paper = self.shelves[at].1.stack(i).and_then(|s| s.paper);
                self.shelves[at]
                    .1
                    .set_stack(i, Stack { carts, paper, band });
            }
        }
    }

    fn draw_folder_title(&self, folder: usize, alpha: f32, out: &mut Vec<Draw>) {
        let Rack::Folder(platform) = self.shelves[folder].0 else {
            return;
        };
        let Some(&(_, tex, face_w)) = self.folder_titles.iter().find(|(at, _, _)| *at == folder)
        else {
            return;
        };
        let (w, h) = folder_title_size(face_w);
        let top = match platform {
            // The NES and SNES folders use the GBA's title height until their own art sets one.
            Platform::Gba | Platform::Nes | Platform::Snes => FOLDER_TITLE_Y_GBA,
            Platform::Gb | Platform::Gbc => FOLDER_TITLE_Y_GB,
        };
        out.push(Draw::Turned {
            x: self.canvas().centred(w),
            y: top - (h - 40.0) / 2.0,
            w,
            h,
            tex,
            alpha,
            turn: FOLDER_TITLE_TURN.to_radians(),
        });
    }

    /// The stack coming apart into its shelf (design: band off, paper off, carts lift and fan
    /// out, then the name strip), or, closing, the same backwards.
    fn draw_folder_motion(&self, out: &mut Vec<Draw>) {
        let Some((m, t)) = self.folder_moving() else {
            return;
        };
        let canvas = self.canvas();
        let phase = |from: f32, span: f32| ease(((t - from) / span).clamp(0.0, 1.0));
        let parent = &self.shelves[m.parent].1;
        let folder = &self.shelves[m.folder].1;
        let Some(cover) = parent.carts.get(m.item) else {
            return;
        };
        let platform = cover.platform;
        parent.draw_row(canvas, Some(&cover.stem), 0.0, phase(0.0, 420.0), 1.0, out);

        let (rest, scale) = parent.selected_at(canvas);
        let poses = stack_poses(platform, shelf_cart_of(platform, rest, scale));
        let stack = parent.stack(m.item).copied().unwrap_or_default();
        let (bw, bh) = shelf_box(platform);
        let in_row = |offset: f32, scale: f32| Placed {
            x: canvas.w / 2.0 + offset * bw - bw * scale / 2.0,
            y: slot_ui::foot_y(bh) - bh * scale,
            w: bw * scale,
            h: bh * scale,
        };
        let n = folder.carts.len();
        let (lift, fan) = (phase(240.0, 320.0) * FOLDER_LIFT, phase(480.0, 560.0));
        // The cover lands in the middle, the next cart on its right; the third is the left
        // neighbour only when the folder holds exactly three.
        let third = if n == 3 { -1.0 } else { 2.0 };
        let targets = [(0.0, 1.0), (1.0, SIDE_CART), (third, SIDE_CART)];
        if n >= 2 && n != 3 {
            if let Some(tex) = folder.face(n - 1) {
                let at = in_row(-1.0, SIDE_CART);
                draw_pose(Pose { at, turn: 0.0 }, tex, fan, out);
            }
        }
        for k in (0..3).rev() {
            let Some(tex) = stack.carts[k] else {
                continue;
            };
            let from = poses.carts[k];
            let lifted = Placed {
                y: from.at.y - lift,
                ..from.at
            };
            let (offset, scale) = targets[k];
            let at = lerp_placed(lifted, in_row(offset, scale), fan);
            let turn = from.turn * (1.0 - fan);
            draw_pose(Pose { at, turn }, tex, 1.0, out);
        }
        if let Some(tex) = stack.paper {
            let a = phase(80.0, 380.0);
            let p = poses.paper;
            let at = Placed {
                x: p.at.x - 170.0 * a,
                y: p.at.y + 60.0 * a,
                ..p.at
            };
            let turn = p.turn - 38f32.to_radians() * a;
            draw_pose(Pose { at, turn }, tex, 1.0 - a, out);
        }
        if let Some(tex) = stack.band {
            let a = phase(0.0, 320.0);
            let b = poses.band;
            let w = b.at.w * (1.0 - 0.5 * a);
            let at = Placed {
                x: b.at.x + (b.at.w - w) / 2.0 + 240.0 * a,
                y: b.at.y - 110.0 * a,
                w,
                h: b.at.h,
            };
            let turn = b.turn + 55f32.to_radians() * a;
            draw_pose(Pose { at, turn }, tex, 1.0 - a, out);
        }
        self.draw_folder_title(m.folder, phase(900.0, 200.0), out);
    }

    pub fn face_cart(&self, shelf: usize, index: usize) -> Option<&Cart> {
        self.shelves.get(shelf)?.1.carts.get(index)
    }

    pub fn set_cart_face(&mut self, shelf: usize, index: usize, tex: TexId) -> Option<TexId> {
        self.shelves.get_mut(shelf)?.1.set_face(index, tex)
    }

    pub fn set_cart_interior(&mut self, shelf: usize, index: usize, tex: TexId) -> Option<TexId> {
        self.shelves.get_mut(shelf)?.1.set_interior(index, tex)
    }

    pub fn shed_faces(&mut self) -> Vec<TexId> {
        let neighbours = self.neighbour_shelves();
        let mut shed = std::mem::take(&mut self.orphaned);
        let mut stacked = Vec::new();
        for (at, (_, shelf)) in self.shelves.iter().enumerate() {
            let shown = if at == self.shelf_at {
                shelf.around(FACE_KEEP)
            } else if neighbours.contains(&at) {
                shelf.on_screen()
            } else {
                Vec::new()
            };
            stacked.extend(shown.into_iter().filter_map(|i| shelf.folder_at(i)));
        }
        let stacked: Vec<(usize, Vec<usize>)> = stacked
            .into_iter()
            .map(|f| (f, self.stack_indices(f)))
            .collect();
        for (at, (_, shelf)) in self.shelves.iter_mut().enumerate() {
            let mut keep = if at == self.shelf_at {
                shelf.around(FACE_KEEP)
            } else if neighbours.contains(&at) {
                shelf.on_screen()
            } else {
                Vec::new()
            };
            if let Some((_, picks)) = stacked.iter().find(|(f, _)| *f == at) {
                keep.extend(picks);
            }
            for i in 0..shelf.carts.len() {
                if !keep.contains(&i) {
                    shed.extend(shelf.take_face(i));
                    shed.extend(shelf.take_interior(i));
                }
            }
        }
        shed
    }

    fn neighbour_shelves(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for by in [1, -1] {
            if let Some(at) = self.next_shelf(by) {
                if !out.contains(&at) {
                    out.push(at);
                }
            }
        }
        out
    }

    pub fn set_snapshot(&mut self, snapshot: Box<dyn Snapshot>) {
        self.snapshot = Some(snapshot);
    }

    pub fn set_core(&mut self, core: Core) {
        self.core = core;
    }

    pub fn set_platform(&mut self, platform: Platform) {
        self.platform = platform;
    }

    pub fn set_named_core(&mut self, named: bool) {
        self.named_core = named;
    }

    pub fn set_video_mode(&mut self, mode: VideoMode) {
        self.video_mode = mode;
    }

    pub fn source_rect(&self) -> [f32; 4] {
        if self.tall_game().is_some() {
            let [x0, y0, x1, y1] = self.picture_rect();
            return [x0, y0, x1 - x0, y1 - y0];
        }
        video_mode::source_rect(self.platform, self.video_mode)
    }

    /// A Game Boy picture on a 4:3 panel takes the panel's full height: its own shape when
    /// actual, the whole panel when stretched. Its bezels frame it from the sides only. The TV
    /// consoles take their own 4:3 path instead (`tv_fit`).
    pub fn tall_game(&self) -> Option<f32> {
        if self.platform == Platform::Gba || self.platform.on_a_tv() || !self.framed_panel() {
            return None;
        }
        let (w, h) = self.platform.picture();
        Some(match self.video_mode {
            VideoMode::Actual => w as f32 / h as f32,
            VideoMode::Stretch => f32::INFINITY,
        })
    }

    /// L1 and R1 are Slot's picture keys on a console without shoulder buttons (Game Boy, NES);
    /// on the GBA and the Super Nintendo they are the game's L and R.
    fn slot_owns_the_shoulders(&self) -> bool {
        matches!(self.phase, Phase::Playing { .. })
            && !GameButton::of(self.platform).contains(&GameButton::L)
    }

    pub fn taken_buttons(&self) -> Vec<Btn> {
        if !self.slot_owns_the_shoulders() {
            return Vec::new();
        }
        [Btn::L1, Btn::R1]
            .into_iter()
            .filter(|b| !self.bound_to_game(*b))
            .collect()
    }

    fn bound_to_game(&self, b: Btn) -> bool {
        GameButton::of(self.platform).iter().any(|g| {
            self.controls
                .game(*g)
                .is_some_and(|x| x.modifier.is_none() && crate::input::device_btn(x.button) == b)
        })
    }

    pub fn controls(&self) -> Controls {
        self.controls
    }

    pub fn shortcuts(&self) -> ShortcutKeys {
        self.shortcuts
    }

    pub fn set_plugged_face(&mut self, tex: TexId) {
        self.plugged_face = Some(tex);
    }

    pub fn capturing(&self) -> bool {
        self.capture.is_some()
    }

    pub fn capture_row(&self) -> Option<QuickRow> {
        self.capture.map(|c| c.row)
    }

    fn start_capture(&mut self, row: QuickRow) {
        self.capture = Some(BindCapture {
            row,
            menu: false,
            first: None,
        });
    }

    pub fn capture_input(&mut self, ev: RawEvent) {
        let Some(mut c) = self.capture else {
            return;
        };
        let pairs = c.row.game_button().is_none();
        match ev {
            RawEvent::Down(Btn::Menu) if c.first.is_none() => c.menu = true,
            RawEvent::Up(Btn::Menu) => c.menu = false,
            RawEvent::Down(b) => {
                if let Some(button) = crate::input::btn_device(b) {
                    let binding = match (c.menu, c.first) {
                        (true, _) => Binding::with(Modifier::Menu, button),
                        (false, Some(first)) => Binding::pair(first, button),
                        (false, None) if pairs => {
                            c.first = Some(button);
                            self.capture = Some(c);
                            return;
                        }
                        (false, None) => Binding::plain(button),
                    };
                    return self.bind(c.row, binding);
                }
            }
            RawEvent::Up(b) => {
                if let Some(first) = c.first.filter(|f| crate::input::btn_device(b) == Some(*f)) {
                    return self.bind(c.row, Binding::plain(first));
                }
            }
        }
        self.capture = Some(c);
    }

    fn bind(&mut self, row: QuickRow, binding: Option<Binding>) {
        self.capture = None;
        self.set_binding(row, binding);
        if let Some(other) = self.clash(row) {
            let now = self.now();
            self.hud.toast(Toast::Duplicate(other), now);
        }
    }

    /// The shown row bound to the same button as `row`; a hidden one, such as the SNES X/Y on a
    /// card with no SNES games, takes no button.
    pub fn clash(&self, row: QuickRow) -> Option<QuickRow> {
        let hidden = self.hidden_rows();
        if hidden.contains(&row) {
            return None;
        }
        self.binding_table()
            .clash(row)
            .filter(|other| !hidden.contains(other))
    }

    pub fn binding_table(&self) -> crate::input::Table {
        crate::input::Table::new(
            &self.controls,
            &self.shortcuts,
            &self.carousel,
            self.state.turbo,
        )
    }

    pub fn input_context(&self) -> slot_input::Context {
        let overlaid = self.game_menu_open()
            || self.in_game_menu().is_some()
            || self.repair_prompt()
            || self.shutting_down();
        let shelf = matches!(self.phase, Phase::Shelf)
            && self.games.is_none()
            && self.core_picker.is_none()
            && !self.repair_prompt
            && self.reset_prompt.is_none();
        match self.phase {
            Phase::Playing { .. } if !overlaid => slot_input::Context::Game,
            _ if shelf => slot_input::Context::Shelf,
            _ => slot_input::Context::Menu,
        }
    }

    fn binding_of(&self, row: QuickRow) -> Option<Binding> {
        if let Some(g) = row.game_button() {
            return self.controls.game(g);
        }
        if let Some(s) = row.shortcut() {
            return self.shortcuts.get(s);
        }
        row.carousel_action().and_then(|a| self.carousel.get(a))
    }

    fn reset_page(&mut self, row: QuickRow) {
        self.reset_prompt = None;
        let (what, written) = if row.game_button().is_some() {
            self.controls = Controls::default();
            let Some(root) = &self.root else {
                return;
            };
            ("controls", write_controls(root, &self.controls))
        } else if row.shortcut().is_some() {
            self.shortcuts = ShortcutKeys::default();
            let Some(root) = &self.root else {
                return;
            };
            ("shortcuts", write_shortcuts(root, &self.shortcuts))
        } else {
            self.carousel = CarouselKeys::default();
            let Some(root) = &self.root else {
                return;
            };
            ("carousel", write_carousel(root, &self.carousel))
        };
        if let Err(e) = &written {
            eprintln!("slot: {what}: {e}");
        }
        self.card_writes.wrote(what, &written);
    }

    fn page_is_default(&self, row: QuickRow) -> bool {
        if row.game_button().is_some() {
            self.controls == Controls::default()
        } else if row.shortcut().is_some() {
            self.shortcuts == ShortcutKeys::default()
        } else {
            self.carousel == CarouselKeys::default()
        }
    }

    pub fn reset_prompt(&self) -> bool {
        self.reset_prompt.is_some()
    }

    fn reset_prompt_faces(&self, row: QuickRow) -> Vec<(TexId, u32, u32)> {
        let at = if row.game_button().is_some() {
            0
        } else if row.shortcut().is_some() {
            2
        } else {
            4
        };
        match self.reset_faces[..] {
            [_, _, _, _, _, _, cancel, reset] => {
                vec![
                    self.reset_faces[at],
                    self.reset_faces[at + 1],
                    cancel,
                    reset,
                ]
            }
            _ => Vec::new(),
        }
    }

    fn set_binding(&mut self, row: QuickRow, binding: Option<Binding>) {
        if let Some(g) = row.game_button() {
            self.controls.set_game(g, binding);
            if let Some(root) = &self.root {
                let written = write_controls(root, &self.controls);
                if let Err(e) = &written {
                    eprintln!("slot: controls: {e}");
                }
                self.card_writes.wrote("controls", &written);
            }
        } else if let Some(s) = row.shortcut() {
            self.shortcuts.set(s, binding);
            if let Some(root) = &self.root {
                let written = write_shortcuts(root, &self.shortcuts);
                if let Err(e) = &written {
                    eprintln!("slot: shortcuts: {e}");
                }
                self.card_writes.wrote("shortcuts", &written);
            }
        } else if let Some(a) = row.carousel_action() {
            self.carousel.set(a, binding);
            if let Some(root) = &self.root {
                let written = write_carousel(root, &self.carousel);
                if let Err(e) = &written {
                    eprintln!("slot: carousel: {e}");
                }
                self.card_writes.wrote("carousel", &written);
            }
        }
    }

    pub fn binding_texts(&self) -> Vec<String> {
        let mut texts: Vec<String> = QuickRow::ALL
            .iter()
            .filter(|r| r.binds())
            .map(|r| self.bound_text(*r))
            .collect();
        texts.push(RECORDING.to_string());
        texts.sort();
        texts.dedup();
        texts
    }

    pub fn has_binding_face(&self, text: &str) -> bool {
        self.binding_faces.contains_key(text)
    }

    pub fn add_binding_face(&mut self, text: String, faces: [(TexId, u32, u32); 2]) {
        self.binding_faces.insert(text, faces);
    }

    fn bound_faces(&self) -> Vec<Option<[(TexId, u32, u32); 2]>> {
        QuickRow::ALL
            .iter()
            .map(|r| {
                if !r.binds() {
                    return None;
                }
                let text = match self.capture_row() == Some(*r) {
                    true => RECORDING.to_string(),
                    false => self.bound_text(*r),
                };
                self.binding_faces.get(&text).copied()
            })
            .collect()
    }

    fn bound_text(&self, row: QuickRow) -> String {
        let text = Binding::text(self.binding_of(row));
        match self.clash(row) {
            Some(_) => format!("{CLASH_MARK}{text}"),
            None => text,
        }
    }

    fn description(&self, row: QuickRow) -> Option<(TexId, u32, u32)> {
        let faces = self.quick_menu_faces.as_ref()?;
        if row.shortcut().is_some() || row.carousel_action().is_some() {
            Some(faces.descriptions[1])
        } else if row.binds() {
            Some(faces.descriptions[0])
        } else {
            None
        }
    }

    pub fn carousel(&self) -> CarouselKeys {
        self.carousel
    }

    pub fn takes_from_the_game(&self, action: Action) -> bool {
        match action {
            Action::GbaDown(btn) | Action::GbaUp(btn) => self.taken_buttons().contains(&btn),
            _ => false,
        }
    }

    fn set_picture(&mut self, mode: VideoMode) {
        if self.video_mode == mode {
            return;
        }
        self.video_mode = mode;
        let (Some(root), Phase::Playing { cart }) = (self.root.clone(), &self.phase) else {
            return;
        };
        let written = video_mode::write_video_mode(&root, cart, self.platform, mode);
        if let Err(e) = &written {
            eprintln!("slot: video: could not write video_mode.txt: {e}");
        }
        self.card_writes.wrote("video_mode.txt", &written);
    }

    pub fn set_link_loaded(&mut self, serial: &'static str) {
        self.link_loaded = Some(serial);
    }

    pub fn link_mode(&self, stem: &str) -> (LinkKind, &'static str) {
        let Some((cart, auto)) = self.auto_link(stem) else {
            return (LinkKind::Cable, "auto");
        };
        let chosen = self.link_choices.get(stem).copied().unwrap_or(auto);
        (chosen, serial_option(chosen, auto, &cart.code, &cart.title))
    }

    pub fn link_active(&self) -> bool {
        self.link.is_some()
    }

    pub fn begin_link(&mut self, client_id: u16) {
        self.link = Some(LinkSession {
            client_id,
            lost_at: None,
        });
        self.sync_link_badge();
    }

    pub fn link_client_id(&self) -> Option<u16> {
        self.link.as_ref().map(|s| s.client_id)
    }

    pub fn may_rewind(&self) -> bool {
        self.state.rewind && !self.link_active()
    }

    pub fn turbo(&self) -> bool {
        self.state.turbo
    }

    pub fn records_rewind(&self) -> bool {
        self.state.rewind
    }

    pub fn picture_rect(&self) -> [f32; 4] {
        crate::video_mode::picture_uv(self.platform)
    }

    /// How a TV console's 4:3 picture sits on the panel; None for the handhelds.
    pub fn tv_fit(&self) -> Option<slot_gfx::TvFit> {
        crate::video_mode::tv_fit(self.playing_platform(), self.video_mode)
    }

    pub fn may_load_state(&self) -> bool {
        !self.link_active()
    }

    pub fn may_fast_forward(&self) -> bool {
        !self.link_active()
    }

    pub fn end_link(&mut self) {
        self.link = None;
        self.link_player = None;
        self.sync_link_badge();
        self.radio.ask(RadioJob::Down);
        self.radio.ask(RadioJob::Cool);
    }

    pub fn peer_lost(&mut self) {
        if matches!(self.game_menu, Some(GameMenu::Linked { .. })) {
            self.game_menu = None;
        }
        let now = self.now();
        if let Some(session) = &mut self.link {
            session.lost_at.get_or_insert(now);
        }
        self.sync_link_badge();
    }

    pub fn bios_mismatch(&mut self) {
        if !self.link_active() {
            return;
        }
        let role = self
            .link_client_id()
            .map_or(self.last_role, LinkRow::from_client_id);
        self.end_link();
        self.hud.toast(Toast::BiosMismatch, self.now());
        self.unplug(role);
    }

    pub fn peer_ended(&mut self) {
        if !self.link_active() {
            return;
        }
        let role = self
            .link_client_id()
            .map_or(self.last_role, LinkRow::from_client_id);
        self.end_link();
        self.hud.toast(Toast::PeerEnded, self.now());
        self.unplug(role);
    }

    pub fn link_badge(&self) -> LinkBadge {
        self.hud.link()
    }

    pub fn set_link_badge_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_link_faces(faces);
    }

    fn sync_link_badge(&mut self) {
        let badge = match &self.link {
            None => LinkBadge::Off,
            Some(s) => match (s.client_id == 0, s.lost_at.is_some()) {
                (true, false) => LinkBadge::Hosting,
                (false, false) => LinkBadge::Joined,
                (true, true) => LinkBadge::HostingLost,
                (false, true) => LinkBadge::JoinedLost,
            },
        };
        self.hud.set_link(badge);
    }

    pub fn take_sfx(&mut self) -> Option<Sfx> {
        self.sfx.take()
    }

    pub fn set_power(&mut self, mut power: Power) {
        power.set_backlight(self.state.brightness);
        self.headphones = power.headphones();
        power.route_audio(self.headphones);
        self.hud.set_headphones(self.headphones);
        let secs = power.now();
        if matches!(self.phase, Phase::SetClock { .. }) || secs < CLOCK_FLOOR {
            self.phase = clock_screen(secs, 0, false);
        }
        self.copy_card_log(&power);
        if let Some(result) = power.take_repair_result() {
            let toast = match result {
                RepairResult::Repaired | RepairResult::Clean => Toast::CardRepaired,
                RepairResult::Failed => Toast::CardRepairFailed,
            };
            self.hud.toast(toast, self.now());
        }
        self.power = Some(power);
        self.battery_at = self.now();
        self.charge_at = self.now();
        self.last_led = None;
    }

    fn copy_card_log(&self, power: &Power) {
        let (Some(log), Some(root)) = (power.card_log(), &self.root) else {
            return;
        };
        if let Err(e) = crate::card_log::copy_to_card(&log, root) {
            eprintln!("slot: card log: {e}");
        }
    }

    fn log_card(&self, alarm: bool) {
        let Some(power) = &self.power else {
            return;
        };
        let Some(log) = power.card_log() else {
            return;
        };
        let kernel = if alarm {
            power.card_kernel_lines()
        } else {
            Vec::new()
        };
        let write_error = self.card_writes.last_error();
        let entry = crate::card_log::entry(&crate::card_log::CardEvent {
            secs: power.now(),
            alarm,
            read_only: self.card_read_only,
            write_error: write_error.as_deref(),
            cart: self.seated(),
            kernel: &kernel,
        });
        if let Err(e) = crate::card_log::append(&log, &entry) {
            eprintln!("slot: card log: {e}");
        }
    }

    pub fn set_rumble(&mut self, strength: u16) {
        if let Some(power) = &mut self.power {
            power.set_rumble(strength);
        }
    }

    pub fn rumble_enabled(&self) -> bool {
        self.state.rumble
    }

    pub fn set_left_stick(&mut self, present: bool) {
        self.left_stick = present;
    }

    pub fn left_stick_dpad(&self) -> bool {
        self.left_stick && self.state.left_stick_dpad
    }

    pub fn ff_speed(&self) -> u8 {
        self.state.ff_speed
    }

    pub fn ff_sound(&self) -> bool {
        self.state.ff_sound
    }

    pub fn screen_shader(&self) -> Shader {
        match self.playing_platform() {
            Platform::Gba => self.state.shader_gba,
            Platform::Gb => self.state.shader_gb,
            Platform::Gbc => self.state.shader_gbc,
            Platform::Nes | Platform::Snes => Shader::Off,
        }
    }

    pub fn colour_correction(&self) -> bool {
        self.colour_correction_for(self.playing_platform())
    }

    pub fn playing_platform(&self) -> Platform {
        self.seated_cart().map_or(self.platform, |c| c.platform)
    }

    pub fn colour_correction_for(&self, platform: Platform) -> bool {
        match platform {
            Platform::Gba => self.state.colour_correction_gba,
            Platform::Gb => self.state.colour_correction_gb,
            Platform::Gbc => self.state.colour_correction_gbc,
            Platform::Nes | Platform::Snes => false,
        }
    }

    pub fn light(&self) -> bool {
        self.state.appearance.light()
    }

    pub fn status_ink(&self) -> [u8; 3] {
        match self.phase {
            Phase::Playing { .. } | Phase::Polaroids { .. } => footer_ink(false),
            _ => footer_ink(self.light()),
        }
    }

    pub fn powering_off(&self) -> bool {
        self.powering_off
    }

    pub fn ready_to_power_off(&self) -> bool {
        self.powering_off && self.now() >= self.act_at
    }

    pub fn ready_to_repair(&self) -> bool {
        self.repairing && self.now() >= self.act_at
    }

    pub fn repair_offered(&self) -> bool {
        self.repair_possible() && self.charged_to_repair()
    }

    fn repair_possible(&self) -> bool {
        self.card_read_only
            && !self.repair_prompt
            && !self.shutting_down()
            && matches!(self.phase, Phase::Shelf | Phase::Playing { .. })
    }

    fn charged_to_repair(&self) -> bool {
        self.battery.is_none_or(|b| {
            b.percent >= REPAIR_BATTERY_MIN || matches!(b.charge, Charge::Charging | Charge::Full)
        })
    }

    pub fn repair_prompt(&self) -> bool {
        self.repair_prompt
    }

    pub fn repair(&mut self) {
        self.settle_saves();
        if let Some(power) = &mut self.power {
            power.repair_card();
        }
    }

    pub fn ready_to_restart(&self) -> bool {
        self.restarting && self.now() >= self.act_at
    }

    pub fn shutting_down(&self) -> bool {
        self.powering_off || self.restarting || self.repairing
    }

    pub fn restarting(&self) -> bool {
        self.restarting
    }

    pub fn restart(&mut self) {
        self.settle_saves();
        if let Some(power) = &mut self.power {
            power.restart();
        }
    }

    pub fn core_picker(&self) -> Option<Core> {
        self.core_picker.map(|p| p.seat())
    }

    pub fn core_picker_chip(&self) -> Option<Chip> {
        self.core_picker.map(|p| p.chip(self.now()))
    }

    pub fn game_menu_open(&self) -> bool {
        self.game_menu.is_some()
    }

    pub fn game_menu(&self) -> Option<GameMenu> {
        self.game_menu
    }

    pub fn take_link_transport(&mut self) -> Option<(u16, Box<dyn LinkChannel>)> {
        self.link_transport.take()
    }

    pub fn take_link_reload(&mut self) -> Option<(String, &'static str)> {
        self.link_reload.take()
    }

    pub fn link_player(&self) -> Option<u8> {
        self.link_player
    }

    pub fn take_colour_correction(&mut self) -> Option<bool> {
        self.colour_pending.take()
    }

    pub fn gb_colors(&self) -> GbColors {
        self.state.gb_colors()
    }

    pub fn card_read(
        &self,
        what: &str,
        read: std::io::Result<Option<Vec<u8>>>,
    ) -> Option<Option<Vec<u8>>> {
        match read {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                eprintln!("slot: {what} read: {e}");
                self.card_writes.failed(what, &e);
                None
            }
        }
    }

    pub fn set_resume_unread(&mut self, unread: bool) {
        self.resume_unread = unread;
    }

    pub fn set_palette_live(&mut self, live: bool) {
        self.palette_live = live;
    }

    pub fn palette_live(&self) -> bool {
        self.palette_live
    }

    pub fn take_gb_colors(&mut self) -> Option<GbColors> {
        self.palette_pending.take()
    }

    pub fn core(&self) -> Core {
        self.core
    }

    pub fn link_reload_done(&mut self) {
        let Some(reload) = self.reload.take() else {
            return;
        };
        if reload.fallback {
            self.link_choices.insert(reload.stem, reload.from);
            self.close_game_menu();
            return self.refuse();
        }
        if reload.cancelled {
            return self.close_game_menu();
        }
        let since = match self.game_menu {
            Some(GameMenu::Working { since, .. }) => since,
            _ => self.now(),
        };
        self.start_link_from(
            LinkStarter::spawn(reload.role.role(), link_port()),
            reload.role.client_id(),
            since,
        );
    }

    pub fn link_reload_failed(&mut self) {
        let Some(mut reload) = self.reload.take() else {
            return;
        };
        if !reload.fallback {
            self.link_reload = Some((reload.stem.clone(), reload.from_serial));
            reload.fallback = true;
            self.reload = Some(reload);
            return;
        }
        self.link_choices.insert(reload.stem, reload.from);
        self.refuse_seated();
    }

    pub fn selected_stem(&self) -> Option<&str> {
        self.shelf()
            .carts
            .get(self.shelf().index)
            .map(|c| c.stem.as_str())
    }

    pub fn battery(&self) -> Option<Battery> {
        self.battery
    }

    pub fn poweroff(&mut self) {
        self.settle_saves();
        if let Some(power) = &mut self.power {
            power.poweroff();
        }
    }

    pub fn apply(&mut self, action: Action) {
        self.track_held_dir(action);
        self.pressed_at = self.now();
        if self.slept_idle && matches!(self.phase, Phase::Doze { .. }) && action != Action::LidClose
        {
            self.slept_idle = false;
            return self.wake();
        }
        match action {
            Action::LidClose => {
                eprintln!("slot: doze: the lid closed");
                return self.doze();
            }
            Action::LidOpen => return self.wake(),
            Action::PowerPress => {
                if self.link_active() {
                    self.end_link();
                    return;
                }
                self.woke_on_press = matches!(self.phase, Phase::Doze { .. });
                if self.woke_on_press {
                    self.wake();
                }
                return self.flush_resume();
            }
            Action::PowerTap => return self.power_press(),
            Action::PowerHold => return self.hold_power(),
            Action::PowerOff => return,
            _ => {}
        }
        if let Phase::SetClock {
            picker, from_menu, ..
        } = &mut self.phase
        {
            match action {
                Action::GbaDown(Btn::Left) | Action::ShelfLeft => picker.left(),
                Action::GbaDown(Btn::Right) | Action::ShelfRight => picker.right(),
                Action::GbaDown(Btn::Up) => picker.up(),
                Action::GbaDown(Btn::Down) => picker.down(),
                Action::GbaDown(Btn::A) | Action::Insert => self.confirm_clock(),
                Action::GbaDown(Btn::B) if *from_menu => {
                    self.phase = Phase::QuickMenu {
                        row: QuickRow::DateTime,
                    }
                }
                _ => {}
            }
            return;
        }
        if self.repair_prompt {
            match action {
                Action::GbaDown(Btn::A) => self.begin_repair(),
                Action::GbaDown(Btn::B) => self.repair_prompt = false,
                _ => {}
            }
            return;
        }
        if let Some(row) = self.reset_prompt {
            match action {
                Action::GbaDown(Btn::A) => self.reset_page(row),
                Action::GbaDown(Btn::B) => self.reset_prompt = None,
                _ => {}
            }
            return;
        }
        if self.delete_prompt {
            match action {
                Action::GbaDown(Btn::X) => {
                    self.delete_prompt = false;
                    self.delete_selected();
                }
                Action::GbaDown(Btn::B) => self.delete_prompt = false,
                _ => {}
            }
            return;
        }
        if let Some(game) = self.game_delete.clone() {
            match action {
                Action::GbaDown(Btn::X) => {
                    self.forget_game_delete();
                    self.delete_game(&game);
                }
                Action::GbaDown(Btn::B) => self.forget_game_delete(),
                _ => {}
            }
            return;
        }
        if action == Action::RepairCard {
            self.repair_prompt = self.repair_offered();
            return;
        }
        if self.adjust(action) {
            return;
        }
        if action == Action::GameSwitcher {
            return self.game_switcher();
        }
        match action {
            Action::GbaUp(Btn::Left) => self.shelf_mut().release_left(),
            Action::GbaUp(Btn::Right) => self.shelf_mut().release_right(),
            _ => {}
        }
        if self.game_menu.is_some() {
            return self.game_menu_input(action);
        }
        if let (Some(row), Phase::Playing { .. }) = (self.in_game_menu, &self.phase) {
            if action != Action::Eject {
                return self.in_game_menu_input(row, action);
            }
        }
        let now = self.now();
        match self.phase {
            Phase::Shelf => match action {
                _ if self.folder_moving().is_some() => {}
                _ if self.games.is_some() => self.games_input(action),
                _ if self.folder_menu.is_some() => self.folder_menu_input(action),
                _ if self.report_open() => self.report_input(action),
                _ if self.extras_menu_shown() => self.extras_menu_input(action),
                Action::GbaDown(Btn::Start) if self.can_open_extras_menu() => {
                    self.open_extras_menu()
                }
                Action::GbaDown(Btn::Start) if self.selected_folder().is_some() => {
                    self.open_folder_menu()
                }
                Action::GbaDown(Btn::Start) if self.core_picker.is_none() => {
                    self.open_core_picker()
                }
                _ if self.core_picker.is_some() => self.core_picker_input(action),
                Action::PreviousShelf => self.switch_shelf(-1),
                Action::NextShelf => self.switch_shelf(1),
                Action::Favorite => self.toggle_favorite(),
                Action::GbaDown(Btn::Up) => self.jump_letter(-1),
                Action::GbaDown(Btn::Down) => self.jump_letter(1),
                _ if self.on_extras() && self.extra_motion.is_some() => {}
                Action::ShelfLeft | Action::GbaDown(Btn::Left) if self.on_extras() => {
                    self.extra_pos -= 1
                }
                Action::ShelfRight | Action::GbaDown(Btn::Right) if self.on_extras() => {
                    self.extra_pos += 1
                }
                Action::GbaDown(Btn::A) | Action::Insert if self.on_extras() => self.open_extra(),
                Action::ShelfLeft | Action::GbaDown(Btn::Left) => self.shelf_mut().hold_left(now),
                Action::ShelfRight | Action::GbaDown(Btn::Right) => {
                    self.shelf_mut().hold_right(now)
                }
                Action::QuickMenu => self.open_quick_menu(),
                Action::GbaDown(Btn::A) => self.play_held = Some(now),
                Action::GbaUp(Btn::A) => {
                    if self.play_held.take().is_some() && !self.open_folder() {
                        self.insert(!self.state.eject_save);
                    }
                }
                Action::Insert => {
                    if !self.open_folder() {
                        self.insert(!self.state.eject_save)
                    }
                }
                Action::GbaDown(Btn::B) => self.close_folder(),
                _ => {}
            },
            Phase::Inserting { .. } if action == Action::Eject => self.eject(),
            Phase::Playing { .. } => match action {
                Action::GbaDown(Btn::L1)
                    if self.slot_owns_the_shoulders() && !self.bound_to_game(Btn::L1) =>
                {
                    self.set_picture(VideoMode::Stretch)
                }
                Action::GbaDown(Btn::R1)
                    if self.slot_owns_the_shoulders() && !self.bound_to_game(Btn::R1) =>
                {
                    self.set_picture(VideoMode::Actual)
                }
                Action::Eject => self.eject(),
                Action::GameMenu => self.game_menu_shortcut(),
                Action::QuickMenu => self.tap_in_game_menu(Some(QuickRow::PLAY[0])),
                Action::Polaroids => self.double_tap_states(),
                Action::SaveState => self.save_state(),
                Action::LoadState => self.load_newest(),
                Action::RewindStart if !self.may_rewind() => self.refuse(),
                Action::FfStart if !self.may_fast_forward() => self.refuse(),
                _ => {}
            },
            Phase::Polaroids { .. } => match action {
                Action::ShelfLeft | Action::GbaDown(Btn::Left) => self.flick(Polaroids::left),
                Action::ShelfRight | Action::GbaDown(Btn::Right) => self.flick(Polaroids::right),
                Action::GbaDown(Btn::A) => self.load_selected(),
                Action::GbaDown(Btn::B) | Action::Polaroids => self.close_polaroids(),
                Action::GbaDown(Btn::X) => self.undo(self.now()),
                Action::GbaDown(Btn::Y) => {
                    self.delete_prompt =
                        self.polaroids.as_ref().and_then(|p| p.selected()).is_some()
                }
                _ => {}
            },
            Phase::About if action == Action::GbaDown(Btn::B) || action == Action::QuickMenu => {
                self.phase = Phase::QuickMenu {
                    row: QuickRow::About,
                }
            }
            Phase::About if action == Action::GbaDown(Btn::X) => self.begin_restart(),
            Phase::QuickMenu { row } => self.quick_menu_input(row, action),
            _ => {}
        }
    }

    fn open_quick_menu(&mut self) {
        self.phase = Phase::QuickMenu {
            row: QuickRow::MAIN[0],
        };
    }

    fn quick_menu_input(&mut self, row: QuickRow, action: Action) {
        if row.binds() {
            match action {
                Action::GbaDown(Btn::A) => return self.start_capture(row),
                Action::GbaDown(Btn::X) => return self.set_binding(row, None),
                Action::GbaDown(Btn::Y) if !self.page_is_default(row) => {
                    self.reset_prompt = Some(row);
                    return;
                }
                Action::GbaDown(Btn::Left | Btn::Right) => return,
                _ => {}
            }
        }
        let row = match action {
            Action::GbaDown(Btn::Up) => row.up(&self.hidden_rows()),
            Action::GbaDown(Btn::Down) => row.down(&self.hidden_rows()),
            Action::GbaDown(Btn::Left) => return self.change_setting(row, false),
            Action::GbaDown(Btn::Right) => return self.change_setting(row, true),
            Action::GbaDown(Btn::A) => return self.open_quick_row(row),
            Action::GbaDown(Btn::B) | Action::QuickMenu => {
                self.phase = match row.parent() {
                    Some(row) => Phase::QuickMenu { row },
                    None => Phase::Shelf,
                };
                return;
            }
            _ => return,
        };
        self.phase = Phase::QuickMenu { row };
    }

    fn in_game_menu_input(&mut self, row: QuickRow, action: Action) {
        if row == QuickRow::CheatList {
            return self.cheat_list_input(action);
        }
        let hidden = self.in_game_hidden();
        let hidden = &hidden[..];
        if row.binds() {
            match action {
                Action::GbaDown(Btn::A) => return self.start_capture(row),
                Action::GbaDown(Btn::X) => return self.set_binding(row, None),
                Action::GbaDown(Btn::Y) if !self.page_is_default(row) => {
                    self.reset_prompt = Some(row);
                    return;
                }
                Action::GbaDown(Btn::Left | Btn::Right) => return,
                _ => {}
            }
        }
        self.in_game_menu = match action {
            Action::GbaDown(Btn::Up) => Some(row.up(hidden)),
            Action::GbaDown(Btn::Down) => Some(row.down(hidden)),
            Action::GbaDown(Btn::Left) => return self.change_setting(row, false),
            Action::GbaDown(Btn::Right) => return self.change_setting(row, true),
            Action::GbaDown(Btn::A) => return self.open_in_game_row(row),
            Action::GbaDown(Btn::B) => row.parent_in_game(),
            Action::QuickMenu => return self.tap_in_game_menu(None),
            Action::Polaroids => return self.double_tap_states(),
            _ => return,
        };
    }

    fn open_in_game_row(&mut self, row: QuickRow) {
        match row {
            QuickRow::Resume => self.in_game_menu = None,
            QuickRow::States => self.open_states(),
            QuickRow::Cheats => self.open_cheats(),
            QuickRow::ExitGame => self.eject(),
            _ => {
                if let Some(child) = row.child() {
                    self.in_game_menu = Some(child);
                }
            }
        }
    }

    fn tap_in_game_menu(&mut self, to: Option<QuickRow>) {
        self.menu_tap = Some((self.now(), self.in_game_menu));
        self.in_game_menu = to;
    }

    fn in_game_menu_shown(&self) -> Option<QuickRow> {
        match self.menu_tap {
            Some((at, None)) if self.now().saturating_sub(at) < MENU_REVEAL_MS => None,
            _ => self.in_game_menu,
        }
    }

    fn double_tap_states(&mut self) {
        if let Some((at, before)) = self.menu_tap.take() {
            if self.now().saturating_sub(at) <= MENU_DOUBLE_TAP_MS {
                self.in_game_menu = before;
            }
        }
        match self.in_game_menu {
            Some(_) => self.open_states(),
            None => self.open_polaroids(),
        }
    }

    fn open_states(&mut self) {
        self.in_game_menu = None;
        self.open_polaroids();
        match self.phase {
            Phase::Polaroids { .. } => self.states_from_menu = true,
            _ => self.in_game_menu = Some(QuickRow::States),
        }
    }

    fn open_quick_row(&mut self, row: QuickRow) {
        match row {
            QuickRow::DateTime => {
                self.phase = clock_screen(self.utc_secs(), self.state.utc_offset_min, true);
            }
            QuickRow::About => self.phase = Phase::About,
            QuickRow::Screen
            | QuickRow::ScreenGeneral
            | QuickRow::Game
            | QuickRow::ScreenGba
            | QuickRow::ScreenGbc
            | QuickRow::ScreenGb
            | QuickRow::Shortcuts
            | QuickRow::General
            | QuickRow::Controls
            | QuickRow::ControlsShelf
            | QuickRow::ControlsInGame => {
                if let Some(row) = row.child() {
                    let row = row.shown(&self.hidden_rows())[0];
                    self.phase = Phase::QuickMenu { row };
                }
            }
            QuickRow::Brightness
            | QuickRow::BlueLight
            | QuickRow::PlayBrightness
            | QuickRow::PlayBlueLight
            | QuickRow::EjectSave
            | QuickRow::Turbo
            | QuickRow::Rewind
            | QuickRow::FastForward
            | QuickRow::FastForwardSound
            | QuickRow::GbaColourCorrection
            | QuickRow::GbColourCorrection
            | QuickRow::GbcColourCorrection
            | QuickRow::GbaShader
            | QuickRow::GbShader
            | QuickRow::GbcShader
            | QuickRow::GbPalette
            | QuickRow::Rumble
            | QuickRow::LeftStick
            | QuickRow::GbaCore
            | QuickRow::GbCore
            | QuickRow::NesCore
            | QuickRow::SnesCore
            | QuickRow::Theme
            | QuickRow::Recents
            | QuickRow::ShowTitles
            | QuickRow::Wallpaper
            | QuickRow::WallpaperTint
            | QuickRow::Resume
            | QuickRow::States
            | QuickRow::Settings
            | QuickRow::ExitGame
            | QuickRow::PlayScreen
            | QuickRow::PlayGameplay
            | QuickRow::Shader
            | QuickRow::ColourCorrection
            | QuickRow::Palette
            | QuickRow::GbBezel
            | QuickRow::GbcBezel
            | QuickRow::GbaBezel
            | QuickRow::Bezel
            | QuickRow::ShelfPrev
            | QuickRow::ShelfNext
            | QuickRow::ShelfFavorite
            | QuickRow::PlayControls
            | QuickRow::GameUp
            | QuickRow::GameDown
            | QuickRow::GameLeft
            | QuickRow::GameRight
            | QuickRow::GameA
            | QuickRow::GameB
            | QuickRow::GameX
            | QuickRow::GameY
            | QuickRow::GameSelect
            | QuickRow::GameStart
            | QuickRow::GameL
            | QuickRow::GameR
            | QuickRow::GameTurboA
            | QuickRow::GameTurboB
            | QuickRow::ScSave
            | QuickRow::ScLoad
            | QuickRow::ScBrightUp
            | QuickRow::ScBrightDown
            | QuickRow::ScBlueDown
            | QuickRow::ScBlueUp
            | QuickRow::ScColour
            | QuickRow::ScPaletteBack
            | QuickRow::ScPaletteNext
            | QuickRow::ScShaderNext
            | QuickRow::ScShaderBack
            | QuickRow::ScFastForward
            | QuickRow::ScRewind
            | QuickRow::LidOff
            | QuickRow::AutoSleep
            | QuickRow::DimScreen
            | QuickRow::Picture
            | QuickRow::SwitcherGames
            | QuickRow::Cheats
            | QuickRow::CheatList => {}
        }
    }

    fn change_setting(&mut self, row: QuickRow, right: bool) {
        let row = self.playing_row(row);
        if let QuickRow::Brightness | QuickRow::BlueLight = row {
            let (kind, now, max) = match row {
                QuickRow::Brightness => {
                    (HudKind::Brightness, self.state.brightness, BRIGHTNESS_MAX)
                }
                _ => (HudKind::BlueLight, self.state.blue_light, BLUE_LIGHT_MAX),
            };
            let floor = match kind {
                HudKind::Brightness => 1,
                _ => 0,
            };
            let to = match right {
                true => up(now, 1, max).max(floor),
                false => now.saturating_sub(1).max(floor).min(now),
            };
            self.set_level(kind, to);
            return;
        }
        if let QuickRow::Palette | QuickRow::GbPalette = row {
            let to = match right {
                true => self.state.gb_colors().next(),
                false => self.state.gb_colors().prev(),
            };
            self.state.set_gb_colors(to);
            self.palette_pending = Some(to);
            self.persist();
            return;
        }
        if let QuickRow::GbaBezel | QuickRow::GbBezel | QuickRow::GbcBezel = row {
            let platform = match row {
                QuickRow::GbaBezel => Platform::Gba,
                QuickRow::GbBezel => Platform::Gb,
                _ => Platform::Gbc,
            };
            self.cycle_bezel(platform, right);
            self.persist();
            return;
        }
        if row == QuickRow::Wallpaper {
            self.cycle_wallpaper(right);
            self.persist();
            return;
        }
        if row == QuickRow::Recents {
            self.state.show_recents = !self.state.show_recents;
            self.restock_recents();
            self.persist();
            return;
        }
        if row == QuickRow::ShowTitles {
            self.state.show_titles = !self.state.show_titles;
            self.persist();
            return;
        }
        if let QuickRow::GbaCore | QuickRow::GbCore | QuickRow::NesCore | QuickRow::SnesCore = row {
            return self.flip_default_core(row);
        }
        if row == QuickRow::SwitcherGames {
            let n = SWITCHER_GAMES.len();
            let at = SWITCHER_GAMES
                .iter()
                .position(|c| *c == self.state.switcher_games)
                .unwrap_or(n - 1);
            self.state.switcher_games = SWITCHER_GAMES[if right {
                (at + 1) % n
            } else {
                (at + n - 1) % n
            }];
            self.persist();
            return;
        }
        if row == QuickRow::Picture {
            // Two choices, so either way flips it.
            return self.set_picture(match self.video_mode {
                VideoMode::Stretch => VideoMode::Actual,
                VideoMode::Actual => VideoMode::Stretch,
            });
        }
        if row == QuickRow::DimScreen {
            let n = DIM_SCREEN_S.len();
            let at = DIM_SCREEN_S
                .iter()
                .position(|s| *s == self.state.dim_screen_s)
                .unwrap_or(0);
            self.state.dim_screen_s = DIM_SCREEN_S[if right {
                (at + 1) % n
            } else {
                (at + n - 1) % n
            }];
            self.persist();
            return;
        }
        if row == QuickRow::AutoSleep {
            let n = AUTO_SLEEP_MIN.len();
            let at = AUTO_SLEEP_MIN
                .iter()
                .position(|m| *m == self.state.auto_sleep_min)
                .unwrap_or(0);
            self.state.auto_sleep_min = AUTO_SLEEP_MIN[if right {
                (at + 1) % n
            } else {
                (at + n - 1) % n
            }];
            self.persist();
            return;
        }
        if row == QuickRow::LidOff {
            let n = LID_OFF_MIN.len();
            let at = LID_OFF_MIN
                .iter()
                .position(|m| *m == self.lid_off_min())
                .unwrap_or(1);
            self.state.lid_off_min = Some(
                LID_OFF_MIN[if right {
                    (at + 1) % n
                } else {
                    (at + n - 1) % n
                }],
            );
            self.persist();
            return;
        }
        let s = &mut self.state;
        match row {
            QuickRow::FastForward => {
                let to = ff_next(s.ff_speed, right);
                if to == s.ff_speed {
                    return;
                }
                s.ff_speed = to;
            }
            QuickRow::FastForwardSound => s.ff_sound = !s.ff_sound,
            QuickRow::GbaColourCorrection => {
                s.colour_correction_gba = !s.colour_correction_gba;
                self.colour_pending = Some(self.colour_correction());
            }
            QuickRow::GbColourCorrection => {
                s.colour_correction_gb = !s.colour_correction_gb;
                self.colour_pending = Some(self.colour_correction());
            }
            QuickRow::GbcColourCorrection => {
                s.colour_correction_gbc = !s.colour_correction_gbc;
                self.colour_pending = Some(self.colour_correction());
            }
            QuickRow::GbaShader => {
                let to = s.shader_gba.step(&Shader::GBA, right);
                if to == s.shader_gba {
                    return;
                }
                s.shader_gba = to;
            }
            QuickRow::GbShader => {
                let to = s.shader_gb.step(&Shader::GB, right);
                if to == s.shader_gb {
                    return;
                }
                s.shader_gb = to;
            }
            QuickRow::GbcShader => {
                let to = s.shader_gbc.step(&Shader::GB, right);
                if to == s.shader_gbc {
                    return;
                }
                s.shader_gbc = to;
            }
            QuickRow::Rumble => s.rumble = !s.rumble,
            QuickRow::LeftStick => s.left_stick_dpad = !s.left_stick_dpad,
            QuickRow::EjectSave => s.eject_save = !s.eject_save,
            QuickRow::Turbo => s.turbo = !s.turbo,
            QuickRow::Rewind => s.rewind = !s.rewind,
            QuickRow::Theme => s.appearance = s.appearance.toggle(),
            QuickRow::WallpaperTint => s.wallpaper_tint = s.wallpaper_tint.step(right),
            QuickRow::ScreenGeneral
            | QuickRow::Brightness
            | QuickRow::BlueLight
            | QuickRow::PlayBrightness
            | QuickRow::PlayBlueLight
            | QuickRow::Wallpaper
            | QuickRow::Recents
            | QuickRow::ShowTitles
            | QuickRow::DateTime
            | QuickRow::About
            | QuickRow::Screen
            | QuickRow::Game
            | QuickRow::Resume
            | QuickRow::States
            | QuickRow::Settings
            | QuickRow::ExitGame
            | QuickRow::PlayScreen
            | QuickRow::PlayGameplay
            | QuickRow::Shader
            | QuickRow::ColourCorrection
            | QuickRow::Palette
            | QuickRow::GbPalette
            | QuickRow::ScreenGba
            | QuickRow::ScreenGbc
            | QuickRow::ScreenGb
            | QuickRow::GbBezel
            | QuickRow::GbcBezel
            | QuickRow::GbaBezel
            | QuickRow::Bezel
            | QuickRow::General
            | QuickRow::ShelfPrev
            | QuickRow::ShelfNext
            | QuickRow::ShelfFavorite
            | QuickRow::PlayControls
            | QuickRow::Shortcuts
            | QuickRow::GameUp
            | QuickRow::GameDown
            | QuickRow::GameLeft
            | QuickRow::GameRight
            | QuickRow::GameA
            | QuickRow::GameB
            | QuickRow::GameX
            | QuickRow::GameY
            | QuickRow::GameSelect
            | QuickRow::GameStart
            | QuickRow::GameL
            | QuickRow::GameR
            | QuickRow::GameTurboA
            | QuickRow::GameTurboB
            | QuickRow::ScSave
            | QuickRow::ScLoad
            | QuickRow::ScBrightUp
            | QuickRow::ScBrightDown
            | QuickRow::ScBlueDown
            | QuickRow::ScBlueUp
            | QuickRow::ScColour
            | QuickRow::ScPaletteBack
            | QuickRow::ScPaletteNext
            | QuickRow::ScShaderNext
            | QuickRow::ScShaderBack
            | QuickRow::ScFastForward
            | QuickRow::ScRewind
            | QuickRow::Controls
            | QuickRow::ControlsShelf
            | QuickRow::ControlsInGame
            | QuickRow::LidOff
            | QuickRow::GbaCore
            | QuickRow::GbCore
            | QuickRow::NesCore
            | QuickRow::SnesCore
            | QuickRow::AutoSleep
            | QuickRow::DimScreen
            | QuickRow::Picture
            | QuickRow::SwitcherGames
            | QuickRow::Cheats
            | QuickRow::CheatList => return,
        }
        self.persist();
    }

    pub fn apply_at(&mut self, action: Action, now: Millis) {
        self.clock = now as f64;
        self.apply(action);
    }

    fn step_shader(&mut self, right: bool) {
        if !matches!(self.phase, Phase::Inserting { .. } | Phase::Playing { .. }) {
            return;
        }
        let (shader, choices) = match self.playing_platform() {
            Platform::Gba => (&mut self.state.shader_gba, &Shader::GBA[..]),
            Platform::Gb => (&mut self.state.shader_gb, &Shader::GB[..]),
            Platform::Gbc => (&mut self.state.shader_gbc, &Shader::GB[..]),
            // The TV consoles have no LCD shaders to step through.
            Platform::Nes | Platform::Snes => return,
        };
        *shader = shader.step(choices, right);
        let said = match *shader {
            Shader::Off => Toast::ShaderOff,
            Shader::Lcd3x => Toast::ShaderLcd3x,
            Shader::Grid => Toast::ShaderGrid,
            Shader::Dot => Toast::ShaderDot,
            Shader::Simpletex => Toast::ShaderSimpletex,
        };
        self.persist();
        self.hud.toast(said, self.now());
    }

    fn adjust(&mut self, action: Action) -> bool {
        if action == Action::MuteToggle {
            self.mute_toggle();
            return true;
        }
        if self.in_game_menu.is_some()
            && matches!(
                action,
                Action::ShaderNext
                    | Action::ShaderPrev
                    | Action::PaletteNext
                    | Action::PalettePrev
                    | Action::ColourCorrectionToggle
            )
        {
            return true;
        }
        if let Action::ShaderNext | Action::ShaderPrev = action {
            self.step_shader(action == Action::ShaderNext);
            return true;
        }
        if let Action::PaletteNext | Action::PalettePrev = action {
            if self.palette_live && matches!(self.phase, Phase::Playing { .. }) {
                let to = match action {
                    Action::PalettePrev => self.state.gb_colors().prev(),
                    _ => self.state.gb_colors().next(),
                };
                self.state.set_gb_colors(to);
                self.palette_pending = Some(to);
                self.persist();
                self.hud.toast(Toast::Palette(to), self.now());
            }
            return true;
        }
        if action == Action::ColourCorrectionToggle {
            let row = match self.playing_platform() {
                Platform::Gba => QuickRow::GbaColourCorrection,
                Platform::Gb => QuickRow::GbColourCorrection,
                Platform::Gbc => QuickRow::GbcColourCorrection,
                Platform::Nes | Platform::Snes => return true,
            };
            self.change_setting(row, true);
            let said = match self.colour_correction() {
                true => Toast::ColourOn,
                false => Toast::ColourOff,
            };
            self.hud.toast(said, self.now());
            return true;
        }
        let s = &self.state;
        let (kind, value) = match action {
            Action::BrightnessUp => (HudKind::Brightness, up(s.brightness, 1, BRIGHTNESS_MAX)),
            Action::BrightnessDown => (HudKind::Brightness, s.brightness.saturating_sub(1)),
            Action::BlueLightUp => (HudKind::BlueLight, up(s.blue_light, 1, BLUE_LIGHT_MAX)),
            Action::BlueLightDown => (HudKind::BlueLight, s.blue_light.saturating_sub(1)),
            Action::VolumeUp => (HudKind::Volume, up(self.level(), VOLUME_STEP, VOLUME_MAX)),
            Action::VolumeDown => (HudKind::Volume, self.level().saturating_sub(VOLUME_STEP)),
            _ => return false,
        };
        if kind == HudKind::Volume {
            self.remember_volume();
        }
        let unmuted = kind == HudKind::Volume && std::mem::take(self.muted_mut());
        let moved = self.set_level(kind, value);
        let (shown, now) = (self.hud_value(kind, value), self.now());
        self.hud.show(kind, shown, self.muted(), now);
        if unmuted && !moved {
            self.persist_soon();
        }
        true
    }

    fn set_level(&mut self, kind: HudKind, value: u8) -> bool {
        let level = match kind {
            HudKind::Brightness => &mut self.state.brightness,
            HudKind::BlueLight => &mut self.state.blue_light,
            HudKind::Volume if self.headphones => &mut self.state.volume_hp,
            HudKind::Volume => &mut self.state.volume,
            HudKind::Rewind => return false,
        };
        let moved = *level != value;
        *level = value;
        if let (HudKind::Brightness, Some(power)) = (kind, &mut self.power) {
            power.set_backlight(value);
        }
        if moved {
            self.persist_soon();
        }
        moved
    }

    fn remember_volume(&mut self) {
        if self.vol_before.len() == 2 {
            self.vol_before.remove(0);
        }
        self.vol_before
            .push((self.level(), self.muted(), self.now()));
    }

    fn mute_toggle(&mut self) {
        let now = self.now();
        if let Some((volume, muted, _)) = self
            .vol_before
            .iter()
            .find(|(_, _, at)| now.saturating_sub(*at) <= MUTE_CHORD_MS)
            .copied()
        {
            *self.level_mut() = volume;
            *self.muted_mut() = muted;
        }
        self.vol_before.clear();
        let muted = !self.muted();
        *self.muted_mut() = muted;
        self.hud
            .show(HudKind::Volume, self.output_volume(), muted, now);
        self.persist_soon();
    }

    fn hud_value(&self, kind: HudKind, value: u8) -> u8 {
        match kind {
            HudKind::Volume => self.output_volume(),
            _ => value,
        }
    }

    pub fn show_rewind(&mut self, fill: u8) {
        let now = self.now();
        self.hud.show(HudKind::Rewind, fill, false, now);
    }

    pub fn hide_rewind(&mut self) {
        self.hud.release_rewind();
    }

    pub fn set_ff(&mut self, ff: FfState) {
        self.hud.set_ff(ff);
    }

    pub fn ff_badge(&self) -> Option<Icon> {
        self.hud.badge()
    }

    pub fn blue_light(&self) -> u8 {
        self.state.blue_light
    }

    pub fn brightness(&self) -> u8 {
        self.state.brightness
    }

    /// The share of full brightness the picture is drawn at: under 1 only for a step darker
    /// than the backlight reaches.
    pub fn picture_dim(&self) -> f32 {
        self.power.as_ref().map_or(1.0, |p| p.picture_dim())
    }

    pub fn volume(&self) -> u8 {
        self.level()
    }

    fn level(&self) -> u8 {
        if self.headphones {
            self.state.volume_hp
        } else {
            self.state.volume
        }
    }

    fn level_mut(&mut self) -> &mut u8 {
        if self.headphones {
            &mut self.state.volume_hp
        } else {
            &mut self.state.volume
        }
    }

    pub fn muted(&self) -> bool {
        if self.headphones {
            self.state.muted_hp
        } else {
            self.state.muted
        }
    }

    fn muted_mut(&mut self) -> &mut bool {
        if self.headphones {
            &mut self.state.muted_hp
        } else {
            &mut self.state.muted
        }
    }

    pub fn output_volume(&self) -> u8 {
        if self.muted() {
            0
        } else {
            self.level()
        }
    }

    pub fn hud_icon(&self) -> Icon {
        self.hud.glyph()
    }

    pub fn now(&self) -> Millis {
        self.clock as Millis
    }

    fn flick(&mut self, step: fn(&mut Polaroids)) {
        if let Some(p) = &mut self.polaroids {
            step(p);
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.clock += dt as f64 * 1000.0;
        let (target, omega) = (self.extra_pos as f32, EXTRA_OMEGA);
        let accel = -2.0 * omega * self.extra_vel - omega * omega * (self.extra_scroll - target);
        self.extra_vel += accel * dt;
        self.extra_scroll += self.extra_vel * dt;
        self.finish_extra_motion();
        self.finish_report();
        self.track_play(dt);
        if let Some(FolderMenu {
            closing: Some(at), ..
        }) = self.folder_menu
        {
            if self.now().saturating_sub(at) as f32 >= FOLDER_MENU_CLOSE_MS {
                self.folder_menu = None;
            }
        }
        if self.name_pending && self.shelf_platform.face.is_some() {
            self.name_pending = false;
            self.shelf_named = Some(self.now());
        }
        self.timers();
        self.poll_link();
        let now = self.now();
        let ready = self.core_faces_ready();
        if let Some(picker) = &mut self.core_picker {
            if picker.waiting() && (ready || picker.waited(now) >= FACES_WAIT_MS) {
                picker.start(now);
            }
        }
        if self.core_picker.is_some_and(|p| p.finished(now)) {
            self.core_picker = None;
        }
        if !self.on_shelf() {
            self.shelf_mut().release_hold();
        }
        let mut touched = false;
        let at = self.shelf_at;
        let next = match &mut self.phase {
            Phase::Shelf => {
                let shelf = &mut self.shelves[at].1;
                shelf.tick(now);
                shelf.update(dt);
                None
            }
            Phase::Inserting {
                cart,
                t,
                core_ready,
                resumed,
                ..
            } => {
                let was = *t;
                *t += dt;
                let at = SEATED_AT - Sfx::Insert.lead();
                touched = !*resumed && was < at && *t >= at;
                (*t >= INSERT_S && *core_ready).then(|| Phase::Playing {
                    cart: std::mem::take(cart),
                })
            }
            Phase::Ejecting { t, .. } => {
                if self.screen <= 0.0 {
                    let was = *t;
                    *t += dt;
                    touched = was < 0.0 && *t >= 0.0;
                }
                (*t >= EJECT_S).then_some(Phase::Shelf)
            }
            _ => None,
        };
        if touched {
            self.sfx = Some(match self.phase {
                Phase::Ejecting { .. } => Sfx::Eject,
                _ => Sfx::Insert,
            });
        }
        if let Some(phase) = next {
            self.phase = phase;
            self.refused_from = None;
            let seated = match &self.phase {
                Phase::Playing { cart } => Some(cart.clone()),
                _ => None,
            };
            if seated.is_none() {
                self.link_loaded = None;
            }
            self.record_cart(seated);
            if std::mem::take(&mut self.games_after_eject) && self.on_shelf() {
                self.open_games();
            }
            if std::mem::take(&mut self.states_after_insert) && self.seated().is_some() {
                self.open_polaroids();
            }
        }
        self.step_screen(dt);
        self.step_footer(dt);
    }

    fn step_screen(&mut self, dt: f32) {
        self.straight_in &= matches!(self.phase, Phase::Inserting { .. });
        self.blank_s = match self.blank_panel() {
            Some(_) => self.blank_s + dt,
            None => 0.0,
        };
        let lit = matches!(self.phase, Phase::Playing { .. } | Phase::Polaroids { .. })
            || self.powering_up()
            || self.straight_in;
        let step = if lit {
            dt / POWER_ON_S
        } else {
            -dt / POWER_OFF_S
        };
        self.screen = (self.screen + step).clamp(0.0, 1.0);
        let docked = !self.straight_in
            && matches!(
                self.phase,
                Phase::Shelf | Phase::Inserting { .. } | Phase::Ejecting { .. }
            );
        let sliding = matches!(
            self.phase,
            Phase::Inserting { .. }
                | Phase::Playing { .. }
                | Phase::Polaroids { .. }
                | Phase::Ejecting { .. }
        );
        let reach = if sliding { dt / POWER_ON_S } else { 1.0 };
        self.lift = match docked {
            true => (self.lift - reach).max(0.0),
            false => (self.lift + reach).min(1.0),
        };
    }

    fn step_footer(&mut self, dt: f32) {
        let shown = match &self.phase {
            Phase::Playing { .. } => self.in_game_menu_shown(),
            // The carousel already holds this strip, so the menu does not slide it.
            Phase::QuickMenu { row } => {
                self.footer = 1.0;
                self.footer_row = Some(*row);
                return;
            }
            Phase::Ejecting { .. } => {
                self.footer = match self.screen > 0.0 {
                    true => (self.footer + dt / POWER_OFF_S).min(1.0),
                    false => 1.0,
                };
                self.footer_row = None;
                return;
            }
            _ => {
                self.footer = 0.0;
                self.footer_row = None;
                return;
            }
        };
        if let Some(row) = shown {
            self.footer_row = Some(row);
        }
        let target = if shown.is_some() { 1.0 } else { 0.0 };
        let step = dt / FOOTER_SLIDE_S;
        self.footer = if target > self.footer {
            (self.footer + step).min(target)
        } else {
            (self.footer - step).max(target)
        };
        if self.footer == 0.0 {
            self.footer_row = None;
        }
    }

    fn menu_footer_offset(&self) -> f32 {
        (1.0 - ease(self.footer)) * MOUTH_H
    }

    /// The carousel strip shares this canvas, so the picture scales in the space above it.
    /// A 4:3 game frame is a different canvas; its close keeps the full-frame scale.
    fn footer_on_carousel_canvas(&self) -> bool {
        !self.framed_panel() || !self.shows_game()
    }

    pub fn screen_footer(&self) -> f32 {
        if matches!(self.phase, Phase::Ejecting { .. })
            && self.screen > 0.0
            && self.footer_on_carousel_canvas()
        {
            MOUTH_H * ease(self.footer)
        } else {
            0.0
        }
    }

    pub fn set_panel(&mut self, panel: (u32, u32)) {
        self.panel = panel;
    }

    pub fn wallpaper_size(&self) -> (u32, u32) {
        (Canvas::of(self.panel).w as u32, OUT_H)
    }

    pub fn game_bezel_size(&self) -> (u32, u32) {
        match slot_gfx::framed(self.panel) {
            true => self.panel,
            false => (OUT_W, OUT_H),
        }
    }

    pub fn shows_game(&self) -> bool {
        if matches!(self.phase, Phase::Playing { .. }) && self.in_game_menu_shown().is_some() {
            return false;
        }
        if self.powering_up() {
            return true;
        }
        match &self.phase {
            Phase::Playing { .. } | Phase::Polaroids { .. } => true,
            Phase::Ejecting { .. } => self.overlay_slide() > 0.0,
            Phase::Doze { cart } => cart.is_some(),
            _ => false,
        }
    }

    pub fn canvas(&self) -> Canvas {
        Canvas::of(self.panel)
    }

    pub fn set_sp_strip(&mut self, tex: Option<TexId>) {
        self.sp_strip = tex;
    }

    fn draw_sp_strip(&self, out: &mut Vec<Draw>) {
        let slide = self.overlay_slide();
        let shown = self.shows_game() && self.sp_frame() && self.framed_panel() && slide > 0.0;
        let Some(tex) = self.sp_strip.filter(|_| shown) else {
            return;
        };
        let canvas = self.canvas();
        let drop =
            slot_gfx::lifted_rect(self.panel, slide).1 as f32 * canvas.h / self.panel.1 as f32;
        out.push(Draw::Tex {
            x: 0.0,
            y: drop,
            w: canvas.w,
            h: canvas.h,
            tex,
            alpha: 1.0,
        });
    }

    pub fn overlay_slide(&self) -> f32 {
        match &self.phase {
            Phase::Ejecting { t, .. } => ((-t - (EJECT_HOLD_S - POWER_ON_S)) / POWER_ON_S)
                .clamp(0.0, 1.0)
                .min(self.eject_overlay),
            Phase::Playing { .. } | Phase::Polaroids { .. } | Phase::Doze { cart: Some(_) } => {
                self.lift
            }
            _ => 0.0,
        }
    }

    fn framed_panel(&self) -> bool {
        slot_gfx::framed(self.panel)
    }

    fn draw_powering(&self, out: &mut Vec<Draw>) {
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: self.canvas().w,
            h: OUT_H as f32,
            colour: [0.0, 0.0, 0.0, 1.0],
        });
        if self.panel_lit() && self.screen > 0.0 {
            out.push(Draw::Game);
        }
    }

    fn powering_up(&self) -> bool {
        matches!(self.phase, Phase::Inserting { t, resumed: false, .. } if t >= INSERT_S)
    }

    pub fn blank_panel(&self) -> Option<[u8; 3]> {
        let waiting = self.powering_up()
            || (matches!(self.phase, Phase::Playing { .. }) && self.blank_s > 0.0);
        if self.game_ready || !waiting {
            return None;
        }
        let lit = match self.playing_platform() {
            Platform::Gb => [0x9b, 0xa5, 0x82],
            _ => [0xe6, 0xe4, 0xda],
        };
        let k = (self.blank_s / BLANK_WARM_S).min(1.0);
        let level = BLANK_DIM + (1.0 - BLANK_DIM) * (1.0 - (1.0 - k) * (1.0 - k));
        Some(lit.map(|c| (c as f32 * level) as u8))
    }

    fn panel_lit(&self) -> bool {
        self.game_ready || self.blank_panel().is_some()
    }

    pub fn screen_power(&self) -> f32 {
        self.screen
    }

    pub fn set_game_ready(&mut self, ready: bool) {
        self.game_ready = ready;
    }

    pub fn game_ready(&self) -> bool {
        self.game_ready
    }

    pub fn game_visible(&self) -> bool {
        self.panel_lit() && self.screen > 0.0
    }

    pub fn tick_ms(&mut self, now: Millis) {
        self.clock = self.clock.max(now as f64);
        self.timers();
    }

    fn timers(&mut self) {
        self.remember_settled_shelf();
        self.play_hold();
        self.repeat_held_dir();
        let offer = self.undo_label().filter(|_| self.games.is_none());
        if let Some(p) = &mut self.polaroids {
            p.set_undo(offer);
        }
        if self.doze_expired() {
            self.on_doze_timeout();
        }
        self.sleep_when_settled();
        if self.idle_expired() {
            eprintln!("slot: doze: Auto Sleep, no input for a while");
            self.doze();
            self.slept_idle = true;
        }
        if self.now() >= self.autosave_at {
            self.autosave();
        }
        if self.now() >= self.card_check_at {
            self.card_check_at = self.now() + CARD_CHECK_MS;
            self.card_read_only = self.power.as_ref().is_some_and(|p| p.card_read_only());
        }
        let failing = self.card_writes.failing() || self.card_read_only;
        if failing != self.hud.alarmed() {
            eprintln!(
                "slot: card: alarm {} (read-only {})",
                if failing { "on" } else { "off" },
                self.card_read_only
            );
            self.log_card(failing);
        }
        self.hud.set_alarm(failing, self.now());
        self.hud
            .set_alarm_toast(match (self.repair_possible(), self.charged_to_repair()) {
                (true, true) => Toast::CardRepair,
                (true, false) => Toast::CardRepairCharge,
                (false, _) => Toast::CardUnwritable,
            });
        if self.now() >= self.battery_at {
            self.battery_at = self.now() + BATTERY_POLL_MS;
            self.battery = self.power.as_ref().and_then(|p| p.battery());
            if let Some(b) = self.battery {
                self.on_battery(b);
            }
        }
        if self.now() >= self.headphones_at {
            self.headphones_at = self.now() + HEADPHONES_POLL_MS;
            let on = self.power.as_ref().is_some_and(|p| p.headphones());
            if on != self.headphones {
                self.headphones = on;
                if let Some(power) = &mut self.power {
                    power.route_audio(on);
                }
                self.hud.set_headphones(on);
                let now = self.now();
                self.hud
                    .show(HudKind::Volume, self.output_volume(), self.muted(), now);
            }
        }
        if self.now() >= self.charge_at {
            self.charge_at = self.now() + CHARGE_POLL_MS;
            if let (Some(power), Some(b)) = (self.power.as_ref(), self.battery.as_mut()) {
                b.charge = power.charge();
            }
            let state = self.led_state();
            self.set_led(state);
        }
        if let Some(GameMenu::Linked {
            since,
            opened: false,
            ..
        }) = self.game_menu
        {
            if self.now().saturating_sub(since) >= LINKED_HOLD_MS {
                self.game_menu = None;
            }
        }
        if let Some(GameMenu::Unplug { since, .. }) = self.game_menu {
            if self.now().saturating_sub(since) >= UNPLUG_HOLD_MS {
                self.game_menu = None;
            }
        }
        if let Some(at) = self.link.as_ref().and_then(|s| s.lost_at) {
            if self.now().saturating_sub(at) >= LINK_LOST_MS {
                self.end_link();
            }
        }
        // Last, after Auto Sleep and the headphones poll: a doze lights the screen back up for
        // the lid, and a new output's volume follows its route in the same frame.
        self.sync_power_saving();
    }

    pub fn led_state(&self) -> LedState {
        let Some(b) = self.battery else {
            return LedState::Running;
        };
        match b.charge {
            Charge::Charging => LedState::Charging,
            Charge::Full => LedState::Charged,
            _ if b.percent <= BATTERY_LOW => LedState::Low,
            _ => LedState::Running,
        }
    }

    fn set_led(&mut self, state: LedState) {
        if self.shutting_down() && state != LedState::Off {
            return;
        }
        if self.last_led == Some(state) {
            return;
        }
        self.last_led = Some(state);
        if let Some(power) = self.power.as_mut() {
            power.set_led(state);
        }
    }

    fn restock_recents(&mut self) {
        let carts: Vec<Cart> = match self.state.show_recents {
            true => self
                .state
                .recents
                .iter()
                .filter_map(|r| {
                    self.carts()
                        .find(|c| c.platform == r.platform && c.stem == r.stem)
                })
                .cloned()
                .collect(),
            false => Vec::new(),
        };
        self.restock(RECENTS_AT, carts);
        self.mark_stars();
    }

    fn restock_favorites(&mut self) {
        let mut carts: Vec<Cart> = self
            .carts()
            .filter(|c| self.state.is_favorite(c.platform, &c.stem))
            .cloned()
            .collect();
        carts.sort_by(|a, b| {
            (slot_store::sort_key(&a.stem), a.platform as u8)
                .cmp(&(slot_store::sort_key(&b.stem), b.platform as u8))
        });
        let index = self.shelves[FAVORITES_AT].1.index;
        self.restock(FAVORITES_AT, carts);
        let shelf = &mut self.shelves[FAVORITES_AT].1;
        if self.shelf_at == FAVORITES_AT && !shelf.carts.is_empty() {
            shelf.select(index.min(shelf.carts.len() - 1));
        }
        self.mark_stars();
    }

    fn restock(&mut self, at: usize, carts: Vec<Cart>) {
        let stocked = &self.shelves[at].1.carts;
        if stocked.len() == carts.len()
            && stocked
                .iter()
                .zip(&carts)
                .all(|(a, b)| a.platform == b.platform && a.stem == b.stem)
        {
            return;
        }
        let old = self.shelves[at].1.restock(carts);
        self.orphaned.extend(old);
        // Faces still being built belong to the carts that stood here before.
        self.shelf_epoch += 1;
        let empty = self.shelves[at].1.carts.is_empty();
        if self.shelf_at == at && empty {
            self.shelf_at = self.next_shelf(1).unwrap_or(0);
        }
    }

    fn toggle_favorite(&mut self) {
        if self.selected_folder().is_some() {
            return;
        }
        let Some(cart) = self.shelf().carts.get(self.shelf().index) else {
            return;
        };
        let (platform, stem) = (cart.platform, cart.stem.clone());
        let added = self.state.toggle_favorite(platform, &stem);
        let toast = match added {
            true => Toast::FavoriteAdded,
            false => Toast::FavoriteRemoved,
        };
        self.hud.toast(toast, self.now());
        let shelf_at = self.shelf_at;
        self.restock_favorites();
        // Taking away the last favourite empties its shelf: rather than land on whichever shelf
        // comes next, stay on that game where it lives.
        if shelf_at == FAVORITES_AT && self.shelves[FAVORITES_AT].1.carts.is_empty() {
            let home = (0..self.shelves.len())
                .filter(|at| self.shelves[*at].0.platform().is_some())
                .find_map(|at| {
                    let shelf = &self.shelves[at].1;
                    (0..shelf.carts.len())
                        .find(|i| {
                            shelf.folder_at(*i).is_none()
                                && shelf.carts[*i].platform == platform
                                && shelf.carts[*i].stem == stem
                        })
                        .map(|i| (at, i))
                });
            if let Some((at, i)) = home {
                self.shelf_at = at;
                self.shelves[at].1.select(i);
            }
        }
        if self.shelf_at != shelf_at {
            self.shelf_platform = slot_ui::Printed::default();
            self.slot_letter = None;
            self.shelf_named = None;
            self.name_pending = true;
        }
        self.persist();
    }

    fn record_cart(&mut self, cart: Option<String>) {
        let platform = self.seated_cart().map(|c| c.platform);
        if let (Some(stem), Some(platform)) = (&cart, platform) {
            let first = self.state.recents.first();
            if first.is_none_or(|r| r.platform != platform || r.stem != *stem) {
                let stem = stem.clone();
                self.state.played(platform, &stem);
                self.restock_recents();
                self.persist();
            }
        }
        if self.state.cart == cart && self.state.cart_platform == platform {
            return;
        }
        self.state.cart = cart;
        self.state.cart_platform = platform;
        self.persist();
    }

    fn persist(&self) {
        let Some(root) = &self.root else {
            return;
        };
        self.state_writer.flush();
        let written = write_slot_state(root, &self.state);
        if let Err(e) = &written {
            eprintln!("slot: slot.state: {e}");
        }
        self.card_writes.wrote("slot.state", &written);
    }

    fn persist_soon(&self) {
        if let Some(root) = &self.root {
            self.state_writer.save(root, &self.state);
        }
    }

    pub fn flush_state(&self) {
        self.state_writer.flush();
    }

    pub fn on_core_ready(&mut self) {
        let Phase::Inserting {
            cart, core_ready, ..
        } = &mut self.phase
        else {
            return;
        };
        *core_ready = true;
        let cart = cart.clone();
        self.retire_refused_resume(&cart);
    }

    fn retire_refused_resume(&mut self, stem: &str) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        if snapshot.resume_trusted() || self.resume_unread {
            return;
        }
        if !self.named_core {
            return;
        }
        let Some(root) = &self.root else {
            return;
        };
        if let Some(h) = self.pending_save.take() {
            let _ = h.join();
        }
        let ring = StateRing::new(root, self.platform, self.core, stem);
        match ring.retire_resume(&format_stamp(self.wall_secs())) {
            Ok(Some(to)) => eprintln!(
                "slot: resume: {} refused this state, moved it to {}",
                self.core.as_str(),
                to.display()
            ),
            Ok(None) => {}
            Err(e) => eprintln!("slot: resume: could not move the refused state aside: {e}"),
        }
    }

    pub fn on_core_failed(&mut self) {
        let caught = self.seat();
        let Phase::Inserting { cart, .. } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        self.refuse_out(cart, caught);
    }

    fn refuse_out(&mut self, cart: String, caught: f32) {
        let t = (1.0 - caught) * EJECT_S;
        self.phase = Phase::Ejecting { cart, t };
        self.refused_from = Some(t);
    }

    pub fn refuse(&mut self) {
        self.refusal = Some(Refusal::started(self.now()));
    }

    pub fn refusal_active(&self, now: Millis) -> bool {
        self.refusal.is_some_and(|r| r.active(now))
    }

    pub fn seat(&self) -> f32 {
        match &self.phase {
            Phase::Shelf => 0.0,
            Phase::Inserting { t, resumed, .. } => {
                if *resumed {
                    1.0
                } else {
                    (t / SEATED_AT).clamp(0.0, 1.0)
                }
            }
            Phase::Ejecting { t, .. } => 1.0 - (t / EJECT_S).clamp(0.0, 1.0),
            _ => 1.0,
        }
    }

    pub fn draw(&self, out: &mut Vec<Draw>) {
        if self.shutting_down() {
            out.push(Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: self.canvas().w,
                h: OUT_H as f32,
                colour: [0.0, 0.0, 0.0, 1.0],
            });
            let which = if self.repairing {
                PowerChoice::Repair
            } else if self.restarting {
                PowerChoice::Restart
            } else {
                PowerChoice::PowerOff
            };
            if let Some((tex, w, h)) = self.shutdown_faces.get(which.index()).copied() {
                out.push(Draw::Tex {
                    x: self.canvas().centred(w as f32).floor(),
                    y: ((OUT_H - h) / 2) as f32,
                    w: w as f32,
                    h: h as f32,
                    tex,
                    alpha: 1.0,
                });
            }
            return;
        }
        if self.repair_prompt {
            return self.draw_prompt(&self.repair_faces, out);
        }
        if let Some(row) = self.reset_prompt {
            return self.draw_prompt(&self.reset_prompt_faces(row), out);
        }
        if self.delete_prompt {
            return self.draw_prompt(&self.delete_faces, out);
        }
        if self.game_delete.is_some() && !self.game_delete_faces.is_empty() {
            return self.draw_prompt(&self.game_delete_faces, out);
        }
        match &self.phase {
            Phase::SetClock {
                picker, from_menu, ..
            } => {
                let (line, hint) = match self.clock_faces {
                    Some((line, hint)) => (Some(line), Some(hint)),
                    None => (None, None),
                };
                let back = self.clock_back.filter(|_| *from_menu);
                picker.draw(self.canvas(), line, hint, back, out);
                return;
            }
            Phase::QuickMenu { row } => QuickMenu {
                row: *row,
                hidden: &self.hidden_rows(),
                bound: Some(&self.bound_faces()),
                description: self.description(*row),
                values: QuickRow::ALL.map(|r| self.quick_value(r)),
                clock: self.quick_clock_faces,
                wallpaper: self.quick_wallpaper_faces,
                faces: self.quick_menu_faces.as_ref(),
                ground: self.menu_ground(),
                palette: self.quick_palette_faces,
                gba_bezel: self.quick_bezel_faces[bezel_slot(Platform::Gba)],
                gb_bezel: self.quick_bezel_faces[bezel_slot(Platform::Gb)],
                gbc_bezel: self.quick_bezel_faces[bezel_slot(Platform::Gbc)],
                bezel: None,
                over_game: false,
                footer_legends: true,
                footer_top: OUT_H as f32,
                canvas: self.canvas(),
            }
            .draw(out),
            Phase::Shelf if self.games.is_some() => {
                self.draw_background(out);
                if let Some(p) = &self.polaroids {
                    p.draw_print(self.canvas(), out);
                    self.draw_photo_chrome(p, out);
                }
            }
            Phase::Shelf => {
                self.draw_background(out);
                match (self.core_picker_shown(), self.selected_stem()) {
                    (Some(picker), Some(stem)) => {
                        let open = ease(picker.openness(self.now()));
                        let dim = 1.0 + (CORE_PICKER_DIM - 1.0) * open;
                        self.shelf().draw_row(
                            self.canvas(),
                            Some(stem),
                            0.0,
                            CORE_PICKER_RECEDE * open,
                            dim,
                            out,
                        );
                        draw_empty_slot(self.canvas(), self.slot_faces, out);
                    }
                    _ if self.folder_menu.is_some() => {
                        self.draw_folder_menu(out);
                        draw_empty_slot(self.canvas(), self.slot_faces, out);
                    }
                    _ if self.extras_menu_shown() => self.draw_extras_menu(out),
                    _ if self.on_extras() => {
                        self.draw_extras(out);
                        self.draw_report(out);
                        draw_empty_slot(self.canvas(), self.slot_faces, out);
                        draw_slot_name(
                            self.canvas(),
                            self.shelf_platform,
                            self.slot_name_alpha(),
                            out,
                        );
                    }
                    _ if self.folder_moving().is_some() => {
                        self.draw_folder_motion(out);
                        draw_empty_slot(self.canvas(), self.slot_faces, out);
                    }
                    _ => {
                        self.shelf()
                            .draw(self.canvas(), self.shelf_shake(), self.slot_faces, out);
                        draw_slot_name(
                            self.canvas(),
                            self.shelf_platform,
                            self.slot_name_alpha(),
                            out,
                        );
                        slot_ui::draw_slot_title(
                            self.canvas(),
                            self.shelf_title,
                            self.title_alpha(),
                            out,
                        );
                        self.draw_folder_title(self.shelf_at, 1.0, out);
                    }
                }
                draw_footer(
                    self.canvas(),
                    self.battery,
                    self.battery_percent,
                    self.bolt,
                    self.shelf_clock,
                    footer_ink(self.light()),
                    out,
                );
            }
            Phase::About => {
                slot_ui::draw_menu_ground(self.canvas(), self.menu_ground(), out);
                draw_sticker(self.canvas(), self.sticker_face, out);
                self.draw_menu_footer(out);
                return;
            }
            Phase::Inserting { .. } if self.powering_up() && self.framed_panel() => {
                self.draw_powering(out)
            }
            Phase::Inserting { cart, .. } if self.powering_up() => {
                self.draw_background(out);
                self.chrome(cart, 0.0, out);
            }
            Phase::Inserting { .. } if self.straight_in => self.draw_powering(out),
            Phase::Inserting { cart, resumed, .. } => {
                if !resumed {
                    self.draw_background(out);
                    self.shelf()
                        .draw_row(self.canvas(), Some(cart), 0.0, self.seat(), 1.0, out);
                }
                self.chrome(cart, 0.0, out);
            }
            Phase::Ejecting { .. }
                if self.framed_panel() && (self.shows_game() || self.screen > 0.0) =>
            {
                self.draw_powering(out)
            }
            Phase::Playing { .. } if self.framed_panel() && self.screen < 1.0 => {
                self.draw_powering(out)
            }
            Phase::Ejecting { cart, .. } => {
                self.draw_background(out);
                self.shelf()
                    .draw_row(self.canvas(), Some(cart), 0.0, self.seat(), 1.0, out);
                self.chrome(cart, 0.0, out);
            }
            Phase::Playing { cart } if self.screen < 1.0 => {
                self.draw_background(out);
                self.chrome(cart, 0.0, out);
            }
            Phase::Playing { .. } => self.push_game(out),
            Phase::Polaroids { .. } => {
                self.push_game(out);
                if let Some(p) = &self.polaroids {
                    p.draw_photo(self.canvas(), out);
                    self.draw_sp_strip(out);
                    self.draw_photo_chrome(p, out);
                }
            }
            Phase::Doze { .. } => {
                out.push(Draw::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: self.canvas().w,
                    h: OUT_H as f32,
                    colour: [0.0, 0.0, 0.0, 1.0],
                });
                self.draw_sp_strip(out);
                return;
            }
        }
        if !matches!(self.phase, Phase::Polaroids { .. }) {
            self.draw_sp_strip(out);
        }
        if let (Some(row), Phase::Playing { .. }, true) =
            (self.in_game_menu_shown(), &self.phase, self.screen >= 1.0)
        {
            let footer_top = OUT_H as f32 - MOUTH_H + self.menu_footer_offset();
            if row == QuickRow::CheatList {
                self.draw_cheat_list(footer_top, out);
            } else {
                let hidden = self.in_game_hidden();
                let bound = self.bound_faces();
                QuickMenu {
                    row,
                    bound: Some(&bound),
                    description: self.description(row),
                    hidden: &hidden,
                    values: QuickRow::ALL.map(|r| self.quick_value(r)),
                    clock: None,
                    wallpaper: None,
                    faces: self.quick_menu_faces.as_ref(),
                    ground: None,
                    palette: self.quick_palette_faces,
                    gba_bezel: None,
                    gb_bezel: None,
                    gbc_bezel: None,
                    bezel: self.quick_bezel_faces[bezel_slot(self.playing_platform())],
                    over_game: true,
                    footer_legends: false,
                    footer_top,
                    canvas: self.canvas(),
                }
                .draw(out);
            }
            if row.page() != QuickRow::PLAY_SCREEN {
                slot_ui::draw_corners(
                    self.canvas(),
                    self.battery,
                    self.battery_percent,
                    self.hud_bolt,
                    self.shelf_clock,
                    out,
                );
            }
        }
        self.draw_menu_footer(out);
        self.draw_close_footer(out);
        if let Some(picker) = self.core_picker_shown() {
            self.draw_core_picker(&picker, out);
        }
        if let Some(menu) = self.game_menu {
            self.draw_game_menu(menu, out);
        }
        self.hud.draw(self.canvas(), self.now(), out);
    }

    fn draw_photo_chrome(&self, p: &Polaroids, out: &mut Vec<Draw>) {
        p.draw_chrome(
            self.canvas(),
            self.battery,
            self.battery_percent,
            self.hud_bolt,
            self.shelf_clock,
            out,
        );
    }

    pub fn screen_shake(&self) -> f32 {
        self.shake_at(self.now())
    }

    pub fn shake_at(&self, now: Millis) -> f32 {
        self.shake_when(matches!(self.phase, Phase::Playing { .. }), now)
    }

    pub fn shelf_shake(&self) -> f32 {
        self.shake_when(self.on_shelf() && self.core_picker.is_none(), self.now())
    }

    fn shake_when(&self, mine: bool, now: Millis) -> f32 {
        if !mine {
            return 0.0;
        }
        self.refusal.map_or(0.0, |r| r.offset(now))
    }

    fn push_game(&self, out: &mut Vec<Draw>) {
        if self.game_visible() {
            out.push(Draw::Game);
        }
    }

    fn draw_menu_footer(&self, out: &mut Vec<Draw>) {
        let carousel = matches!(self.phase, Phase::QuickMenu { .. } | Phase::About);
        let dy = if carousel {
            0.0
        } else {
            self.menu_footer_offset()
        };
        let start = out.len();
        let row = match &self.phase {
            Phase::Playing { cart } if self.footer > 0.0 => {
                let row = self.in_game_menu_shown().or(self.footer_row);
                match row.is_some_and(QuickRow::binds) {
                    true => slot_ui::draw_plugged_slot(self.canvas(), self.plugged_face, out),
                    false => self.draw_seated_mouth(cart, out),
                }
                row
            }
            // The carousel menu is not a game, so the mouth stays empty, in the
            // same place the shelf already had it.
            Phase::QuickMenu { row } if row.binds() => {
                slot_ui::draw_plugged_slot(self.canvas(), self.plugged_face, out);
                Some(*row)
            }
            Phase::QuickMenu { row } => {
                draw_empty_slot(self.canvas(), self.slot_faces, out);
                Some(*row)
            }
            Phase::About => {
                draw_empty_slot(self.canvas(), self.slot_faces, out);
                self.draw_about_legend(out);
                return;
            }
            _ => return,
        };
        shift_down(&mut out[start..], dy);
        if let Some(row) = row {
            self.draw_menu_legends(row, LINK_LEGEND_Y + dy, out);
        }
    }

    fn draw_close_footer(&self, out: &mut Vec<Draw>) {
        let Phase::Ejecting { cart, .. } = &self.phase else {
            return;
        };
        if !self.footer_on_carousel_canvas() {
            return;
        }
        let start = out.len();
        // The framed power-off paints black and the picture, and never the slot. The other
        // close already draws the seated cart through the chrome.
        if self.framed_panel() && self.screen > 0.0 {
            self.draw_seated_mouth(cart, out);
        }
        draw_footer(
            self.canvas(),
            self.battery,
            self.battery_percent,
            self.bolt,
            self.shelf_clock,
            footer_ink(self.light()),
            out,
        );
        shift_down(&mut out[start..], self.menu_footer_offset());
    }

    fn draw_seated_mouth(&self, stem: &str, out: &mut Vec<Draw>) {
        let Some((cart, face)) = self.shelf().find(stem) else {
            return;
        };
        let (rest, scale) = self.shelf().selected_at(self.canvas());
        SlotChrome {
            cart,
            face,
            rest,
            scale,
            seat: 1.0,
            alert: None,
            dim: 0.0,
            screen: 0.0,
            game: false,
            over: true,
            behind: self.shelf().behind(stem),
            slot: self.slot_faces,
            canvas: self.canvas(),
        }
        .draw(out);
    }

    fn draw_prompt(&self, faces: &[(TexId, u32, u32)], out: &mut Vec<Draw>) {
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: self.canvas().w,
            h: OUT_H as f32,
            colour: [0.0, 0.0, 0.0, 1.0],
        });
        let [title, detail, no, yes] = match faces[..] {
            [a, b, c, d] => [a, b, c, d],
            _ => return,
        };
        let top = ((OUT_H - title.2 - REPAIR_GAP - detail.2) / 2) as f32;
        for ((tex, w, h), y) in [(title, top), (detail, top + (title.2 + REPAIR_GAP) as f32)] {
            out.push(Draw::Tex {
                x: self.canvas().centred(w as f32).floor(),
                y,
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
        }
        self.draw_side_legend((no.0, no.1), false, LINK_LEGEND_Y, out);
        self.draw_side_legend((yes.0, yes.1), true, LINK_LEGEND_Y, out);
    }

    fn draw_about_legend(&self, out: &mut Vec<Draw>) {
        let Some(faces) = &self.quick_menu_faces else {
            return;
        };
        let [back, _, _, _, restart, ..] = match self.light() {
            true => faces.legend_light,
            false => faces.legend,
        };
        self.draw_side_legend(back, false, LINK_LEGEND_Y, out);
        self.draw_side_legend(restart, true, LINK_LEGEND_Y, out);
    }

    fn draw_menu_legends(&self, row: QuickRow, y: f32, out: &mut Vec<Draw>) {
        let Some(faces) = &self.quick_menu_faces else {
            return;
        };
        let [back, change, open, resume, _, set, clear, reset] = match self.light() {
            true => faces.legend_light,
            false => faces.legend,
        };
        if row.binds() {
            let seen = |w: u32| w.saturating_sub(HINT_EDGE) as f32;
            let legends = match self.page_is_default(row) {
                true => vec![back, clear, set],
                false => vec![back, reset, clear, set],
            };
            let room = self.canvas().w - 2.0 * LINK_LEGEND_EDGE;
            let gap = (room - legends.iter().map(|l| seen(l.1)).sum::<f32>())
                / (legends.len() - 1) as f32;
            let mut x = LINK_LEGEND_EDGE;
            for (tex, w) in legends {
                out.push(Draw::Tex {
                    x: x.round(),
                    y,
                    w: w as f32,
                    h: MOUTH_H,
                    tex,
                    alpha: 1.0,
                });
                x += seen(w) + gap;
            }
            return;
        }
        let left = match row.page() == QuickRow::PLAY {
            true => resume,
            false => back,
        };
        self.draw_side_legend(left, false, y, out);
        let right = if row.opens() { open } else { change };
        self.draw_side_legend(right, true, y, out);
    }

    fn draw_side_legend(&self, face: (TexId, u32), right: bool, y: f32, out: &mut Vec<Draw>) {
        let seen = face.1.saturating_sub(HINT_EDGE) as f32;
        let x = if right {
            (self.canvas().w - LINK_LEGEND_EDGE - seen).round()
        } else {
            LINK_LEGEND_EDGE
        };
        out.push(Draw::Tex {
            x,
            y,
            w: face.1 as f32,
            h: MOUTH_H,
            tex: face.0,
            alpha: 1.0,
        });
    }

    fn chrome(&self, stem: &str, dim: f32, out: &mut Vec<Draw>) {
        let Some((cart, face)) = self.shelf().find(stem) else {
            return;
        };
        let alpha = self.alert_alpha();
        let (rest, scale) = self.shelf().selected_at(self.canvas());
        SlotChrome {
            cart,
            face,
            rest,
            scale,
            seat: self.seat(),
            alert: self.alert_face.filter(|_| alpha > 0.0).map(|t| (t, alpha)),
            dim,
            screen: self.screen,
            game: self.panel_lit(),
            over: !self.powering_up()
                && !matches!(
                    &self.phase,
                    Phase::Inserting { resumed: true, .. }
                        | Phase::Playing { .. }
                        | Phase::Ejecting { .. }
                ),
            behind: self.shelf().behind(stem),
            slot: self.slot_faces,
            canvas: self.canvas(),
        }
        .draw(out);
    }

    pub fn alert_visible(&self) -> bool {
        self.alert_alpha() > 0.0
    }

    pub fn alert_alpha(&self) -> f32 {
        let (Some(from), Phase::Ejecting { t, .. }) = (self.refused_from, &self.phase) else {
            return 0.0;
        };
        let span = EJECT_S - from;
        if span <= 0.0 {
            return 0.0;
        }
        let u = ((t - from) / span).clamp(0.0, 1.0);
        ((ALERT_GONE - u) / (ALERT_GONE - ALERT_HOLD)).clamp(0.0, 1.0)
    }

    pub fn set_alert_face(&mut self, face: TexId) {
        self.alert_face = Some(face);
    }

    pub fn set_repair_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.repair_faces = faces;
    }

    pub fn set_delete_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.delete_faces = faces;
    }

    pub fn delete_prompt(&self) -> bool {
        self.delete_prompt
    }

    pub fn set_reset_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.reset_faces = faces;
    }

    pub fn set_shutdown_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.shutdown_faces = faces;
    }

    pub fn set_core_interior_face(&mut self, interior: TexId) {
        self.core_interior_face = Some(interior);
    }

    pub fn set_core_board_faces(&mut self, board: TexId, lid: TexId) {
        self.core_board_face = Some(board);
        self.core_lid_face = Some(lid);
        self.core_faces_stem = self.selected_stem().map(str::to_string);
    }

    pub fn set_core_part_faces(
        &mut self,
        sockets: Vec<TexId>,
        chips: Vec<TexId>,
        blank: TexId,
        shadow: TexId,
    ) {
        self.core_socket_faces = sockets;
        self.core_chip_faces = chips;
        self.core_blank_chip_face = Some(blank);
        self.core_chip_shadow_face = Some(shadow);
    }

    pub fn set_core_legend_faces(&mut self, faces: Vec<(TexId, u32)>) {
        self.core_legend_faces = faces;
    }

    pub fn set_link_menu_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.link_menu_faces = faces;
    }

    pub fn set_link_linked_face(&mut self, face: (TexId, u32, u32)) {
        self.link_linked_face = Some(face);
    }

    pub fn set_link_step_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.link_step_faces = faces;
    }

    pub fn set_link_fail_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.link_fail_faces = faces;
    }

    pub fn set_link_legend_faces(&mut self, faces: Vec<(TexId, u32)>) {
        self.link_legend_faces = faces;
    }

    pub fn set_link_sprites(&mut self, sprites: LinkSprites) {
        self.link_sprites = Some(sprites);
    }

    pub fn set_link_shot(&mut self, tex: Option<TexId>) {
        self.link_shot = tex;
    }

    pub fn set_link_port(&mut self, tex: TexId) {
        if let Some(sprites) = &mut self.link_sprites {
            sprites.port.tex = tex;
        }
    }

    pub fn link_sprites_ready(&self) -> bool {
        self.link_sprites.is_some()
    }

    fn core_picker_shown(&self) -> Option<CorePicker> {
        self.core_picker.filter(|p| !p.waiting())
    }

    fn draw_core_picker(&self, picker: &CorePicker, out: &mut Vec<Draw>) {
        let now = self.now();
        let progress = picker.openness(now);
        let lift = lift_of(progress);
        let (rest, scale) = self.shelf().selected_at(self.canvas());
        let platform = self
            .shelf()
            .carts
            .get(self.shelf().index)
            .map_or(Platform::Gba, |c| c.platform);
        let open = open_layout(platform);
        let shelf = shelf_cart_of(platform, rest, scale);
        let board = open.board_from(self.canvas(), shelf, progress);
        let zoom = open.zoom(board);
        let ready = self.core_faces_ready();

        if ready {
            if let Some(tex) = self.core_interior_face {
                out.push(Draw::Tex {
                    x: board.x,
                    y: board.y,
                    w: board.w,
                    h: board.h,
                    tex,
                    alpha: 1.0,
                });
            }
            if let Some(tex) = self.core_board_face {
                out.push(Draw::Tex {
                    x: board.x,
                    y: board.y,
                    w: board.w,
                    h: board.h,
                    tex,
                    alpha: 1.0,
                });
            }
            let sockets = picker
                .pair()
                .map(|c| self.core_socket_faces.get(c.index()).copied());
            for (i, tex) in sockets.into_iter().enumerate() {
                let Some(tex) = tex else {
                    continue;
                };
                let (x, y) = open.on_board(board, open.socket_u[i], open.socket_v);
                out.push(Draw::Tex {
                    x: x.round(),
                    y: y.round(),
                    w: SOCKET_W as f32 * zoom,
                    h: SOCKET_H as f32 * zoom,
                    tex,
                    alpha: 1.0,
                });
            }

            let chip = picker.chip(now);
            let u = open.chip_u[0] + (open.chip_u[1] - open.chip_u[0]) * chip.across;
            if chip.lift > 0.0 {
                if let Some(tex) = self.core_chip_shadow_face {
                    let (cx, cy) =
                        open.on_board(board, u + open.shadow_at.0, open.chip_v + open.shadow_at.1);
                    let (w, h) = (SHADOW_W as f32 * zoom, SHADOW_H as f32 * zoom);
                    out.push(Draw::Tex {
                        x: cx - w / 2.0,
                        y: cy - h / 2.0,
                        w,
                        h,
                        tex,
                        alpha: 0.6 * chip.lift * lift,
                    });
                }
            }
            let face = match chip.seated {
                Some(core) => self.core_chip_faces.get(core.index()).copied(),
                None => self.core_blank_chip_face,
            };
            if let Some(tex) = face {
                let (x, y) = open.on_board(board, u, open.chip_v - open.hop * chip.lift);
                let body = Placed {
                    x: x + chip.shake,
                    y,
                    w: CHIP_W as f32 * zoom,
                    h: CHIP_H as f32 * zoom,
                };
                let at = grown(body, TURN_PAD as f32 * zoom);
                out.push(Draw::Turned {
                    x: at.x.round(),
                    y: at.y.round(),
                    w: at.w,
                    h: at.h,
                    tex,
                    alpha: 1.0,
                    turn: chip.tip,
                });
            }
        }

        if let Some(tex) = self.core_chip_shadow_face {
            let (lid, _) = open.lid_from(self.canvas(), shelf, progress);
            let k = lid.w / open.lid.w;
            let (shadow_w, shadow_h, drop) = open.lid_shadow;
            let (w, h) = (shadow_w * k, shadow_h * k);
            out.push(Draw::Tex {
                x: lid.x + (lid.w - w) / 2.0,
                y: lid.y + lid.h + drop * k - h / 2.0,
                w,
                h,
                tex,
                alpha: LID_SHADOW_ALPHA * lift,
            });
        }

        if ready {
            if let Some(tex) = self.core_lid_face {
                let (lid, turn) = open.lid_from(self.canvas(), shelf, progress);
                let at = grown(lid, TURN_PAD as f32 * lid.w / cart_box(platform).0 as f32);
                out.push(Draw::Turned {
                    x: at.x,
                    y: at.y,
                    w: at.w,
                    h: at.h,
                    tex,
                    alpha: 1.0,
                    turn,
                });
            }
        } else if let Some((_, Some(tex))) = self
            .selected_stem()
            .and_then(|stem| self.shelf().find(stem))
        {
            let (lid, turn) = open.lid_from(self.canvas(), shelf, progress);
            out.push(Draw::Turned {
                x: lid.x,
                y: lid.y,
                w: lid.w,
                h: lid.h,
                tex,
                alpha: 1.0,
                turn,
            });
        }

        self.draw_played_plate(platform, board, lift, out);

        // Cancel, Swap, Choose and Delete in a centred row.
        self.draw_legend_row(&self.core_legend_faces, &[true; 4], lift, out);
    }

    fn draw_game_menu(&self, menu: GameMenu, out: &mut Vec<Draw>) {
        let canvas = self.canvas();
        if let Some(tex) = self.link_shot {
            out.push(Draw::Tex {
                x: 0.0,
                y: 0.0,
                w: canvas.w,
                h: OUT_H as f32,
                tex,
                alpha: 1.0,
            });
        }
        out.push(Draw::Rect {
            x: 0.0,
            y: 0.0,
            w: canvas.w,
            h: OUT_H as f32,
            colour: LINK_SCRIM,
        });
        if let Some(sprites) = &self.link_sprites {
            crate::link_screen::draw_link_art(
                self.canvas(),
                menu,
                self.link_hardware,
                self.now(),
                sprites,
                out,
            );
        }
        let line = match menu {
            GameMenu::Pick(role) => self.link_menu_faces.get(role.index()).copied(),
            GameMenu::Working { step, .. } => self
                .link_step_faces
                .get(step.shown(self.radio.warmed()).index())
                .copied(),
            GameMenu::Linked { .. } => self.link_linked_face,
            GameMenu::Failed { fail, .. } => fail
                .shown()
                .and_then(|i| self.link_fail_faces.get(i).copied()),
            GameMenu::Unplug { .. } => None,
        };
        if let Some((tex, w, h)) = line {
            out.push(Draw::Tex {
                x: canvas.centred(w as f32).round(),
                y: LINK_TEXT_Y,
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
        }
        let switchable = self.seated().is_some_and(|stem| self.link_switchable(stem));
        // Cancel and Mode stay on the left of the footer, Swap and Link on the right, so the
        // socket in the middle of the strip stays clear.
        let (left, right): (&[LinkLegend], &[LinkLegend]) = match menu {
            GameMenu::Pick(_) if switchable => (&[LinkLegend::Mode], &[LinkLegend::Link]),
            GameMenu::Pick(_) => (&[], &[LinkLegend::Link]),
            GameMenu::Working { .. } => (&[LinkLegend::Cancel], &[]),
            GameMenu::Linked { opened: true, .. } => (&[LinkLegend::Back], &[LinkLegend::EndLink]),
            GameMenu::Linked { .. } => (&[], &[]),
            GameMenu::Failed { .. } => (&[], &[LinkLegend::Ok]),
            GameMenu::Unplug { .. } => (&[], &[]),
        };
        let (left, right) = (self.link_faces(left), self.link_faces(right));
        let room = self.canvas().w - 2.0 * LINK_LEGEND_EDGE;
        if link_keys_w(&left) + link_keys_w(&right) + LINK_LEGEND_GAP <= room {
            self.draw_link_keys(&left, false, out);
            self.draw_link_keys(&right, true, out);
            return;
        }
        let all: Vec<(TexId, u32)> = left.into_iter().chain(right).collect();
        let seen = |w: u32| w.saturating_sub(HINT_EDGE) as f32;
        let gap = (room - all.iter().map(|(_, w)| seen(*w)).sum::<f32>())
            / all.len().saturating_sub(1).max(1) as f32;
        let mut x = LINK_LEGEND_EDGE;
        for (tex, w) in all {
            out.push(Draw::Tex {
                x: x.round(),
                y: LINK_LEGEND_Y,
                w: w as f32,
                h: MOUTH_H,
                tex,
                alpha: 1.0,
            });
            x += seen(w) + gap;
        }
    }

    fn link_faces(&self, keys: &[LinkLegend]) -> Vec<(TexId, u32)> {
        keys.iter()
            .filter_map(|k| self.link_legend_faces.get(k.index()).copied())
            .collect()
    }

    fn draw_link_keys(&self, faces: &[(TexId, u32)], right: bool, out: &mut Vec<Draw>) {
        if faces.is_empty() {
            return;
        }
        let seen = |w: u32| w.saturating_sub(HINT_EDGE) as f32;
        let total = link_keys_w(faces);
        let mut x = if right {
            (self.canvas().w - LINK_LEGEND_EDGE - total).round()
        } else {
            LINK_LEGEND_EDGE
        };
        for &(tex, w) in faces {
            out.push(Draw::Tex {
                x,
                y: LINK_LEGEND_Y,
                w: w as f32,
                h: MOUTH_H,
                tex,
                alpha: 1.0,
            });
            x += seen(w) + LINK_LEGEND_GAP;
        }
    }

    fn auto_link(&self, stem: &str) -> Option<(&Cart, LinkKind)> {
        let cart = self.shelf().carts.iter().find(|c| c.stem == stem)?;
        let auto = link_kind(&cart.code, &cart.title, slot_store::header_clean(&cart.rom));
        Some((cart, auto))
    }

    fn on_shelf(&self) -> bool {
        matches!(self.phase, Phase::Shelf)
    }

    pub fn browsed_rom(&self) -> Option<&std::path::Path> {
        if !self.on_shelf() {
            return None;
        }
        let shelf = self.shelf();
        shelf.carts.get(shelf.index).map(|c| c.rom.as_path())
    }

    fn insert(&mut self, clean: bool) {
        if !self.on_shelf() || self.selected_folder().is_some() {
            return;
        }
        if self.in_folder() {
            let (at, stem) = (self.shelf_at, self.selected_stem().map(str::to_string));
            self.change_prefs(at, |p| p.last = stem);
        }
        let at = self.shelf().index;
        let Some(cart) = self.shelf_mut().carts.get_mut(at).map(|c| {
            c.read_header();
            c.stem.clone()
        }) else {
            return;
        };
        eprintln!(
            "slot: launch: insert {cart} at {:.1} ms",
            crate::boot_time::ms()
        );
        self.play_held = None;
        self.refusal = None;
        self.refused_from = None;
        self.phase = Phase::Inserting {
            cart,
            t: 0.0,
            core_ready: false,
            resumed: false,
            clean,
        };
    }

    pub fn starting_clean(&self) -> bool {
        matches!(self.phase, Phase::Inserting { clean: true, .. })
    }

    fn play_hold(&mut self) {
        let Some(at) = self.play_held else {
            return;
        };
        if !self.on_shelf() {
            self.play_held = None;
            return;
        }
        if self.now().saturating_sub(at) >= PLAY_HOLD_MS {
            self.insert(true);
        }
    }

    fn eject(&mut self) {
        if self.single_cart() {
            return self.refuse();
        }
        self.eject_overlay = match self.shows_game() {
            true => self.overlay_slide(),
            false => 0.0,
        };
        self.in_game_menu = None;
        let cart = match &mut self.phase {
            Phase::Playing { cart } | Phase::Inserting { cart, .. } => std::mem::take(cart),
            _ => return,
        };
        self.end_link();
        self.close_game_menu();
        self.states_after_insert = false;
        self.flush_eject(&cart);
        self.pending = None;
        self.refusal = None;
        self.refused_from = None;
        self.phase = Phase::Ejecting {
            cart,
            t: -EJECT_HOLD_S,
        };
    }

    fn flush_eject(&mut self, stem: &str) {
        self.settle_saves();
        let (Some(root), Some(snapshot)) = (&self.root, &self.snapshot) else {
            return;
        };
        let Some(state) = snapshot.state() else {
            eprintln!("slot: eject: the core gave up no state");
            return;
        };
        let (state, sav) = trusted_write(snapshot.as_ref(), state, "eject");
        let state = state.filter(|_| self.state.eject_save);
        self.state_writer.flush();
        match persist::eject(
            root,
            self.platform,
            self.core,
            stem,
            state.as_deref(),
            sav.as_deref(),
            &self.state,
        ) {
            Ok(()) => {
                self.state.cart = None;
                self.state.cart_platform = None;
                self.card_writes.ok("eject");
                if let (Some(_), Some(thumb)) = (&state, snapshot.thumb()) {
                    let ring = StateRing::new(root, self.platform, self.core, stem);
                    if let Err(e) = ring.write_resume_thumb(&thumb) {
                        eprintln!("slot: eject: resume thumb: {e}");
                    }
                }
            }
            Err(e) => {
                eprintln!("slot: eject: {e}");
                self.card_writes.failed("eject", &e);
            }
        }
    }

    fn doze(&mut self) {
        if self.link_active() {
            self.end_link();
        }
        self.close_game_menu();
        self.core_picker = None;
        self.close_games();
        self.games_after_eject = false;
        self.states_after_insert = false;
        if matches!(self.phase, Phase::Doze { .. }) {
            return;
        }
        self.state_writer.flush();
        self.flush_resume();
        let cart = match &mut self.phase {
            Phase::Playing { cart } | Phase::Polaroids { cart } => Some(std::mem::take(cart)),
            _ => None,
        };
        if std::mem::take(&mut self.states_from_menu) {
            self.in_game_menu = Some(QuickRow::States);
        }
        self.polaroids = None;
        self.phase = Phase::Doze { cart };
        self.dozed_at = self.now();
        self.dozed_wall = self.power.as_ref().map(|p| p.now());
        self.radio.ask(RadioJob::Cool);
        if let Some(power) = &mut self.power {
            power.on_close();
        }
    }

    fn wake(&mut self) {
        let Phase::Doze { cart } = &mut self.phase else {
            return;
        };
        self.phase = match cart.take() {
            Some(cart) => Phase::Playing { cart },
            None => Phase::Shelf,
        };
        if let Some(power) = &mut self.power {
            power.on_open();
        }
    }

    pub fn on_doze_timeout(&mut self) {
        if !matches!(self.phase, Phase::Doze { .. }) {
            return;
        }
        eprintln!("slot: power off: asleep for the Lid Power Off time");
        self.begin_power_off();
    }

    fn power_press(&mut self) {
        if std::mem::take(&mut self.woke_on_press) {
            return;
        }
        match self.phase {
            Phase::Doze { .. } => self.wake(),
            _ => {
                eprintln!("slot: doze: the power key was tapped");
                self.doze()
            }
        }
    }

    fn hold_power(&mut self) {
        self.flush_resume();
        self.begin_power_off();
    }

    fn begin_repair(&mut self) {
        self.repair_prompt = false;
        if self.shutting_down() {
            return;
        }
        self.repairing = true;
        self.act_at = self.now() + SHUTDOWN_SHOW_MS;
        self.set_led(LedState::Off);
    }

    fn begin_restart(&mut self) {
        if self.shutting_down() {
            return;
        }
        self.end_play_session(true);
        self.remember_shelf();
        self.restarting = true;
        self.act_at = self.now() + SHUTDOWN_SHOW_MS;
        self.set_led(LedState::Off);
    }

    fn open_core_picker(&mut self) {
        if self.selected_folder().is_some() {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let Some(cart) = self.shelf().carts.get(self.shelf().index) else {
            return;
        };
        // The picker offers the platform's two cores, so it opens only when the alternative is
        // installed (gpSP ships with Slot, so the GBA always has it).
        let alternative = Core::pair(cart.platform)[1];
        if cart.platform != Platform::Gba && !crate::core::installed(&root, alternative) {
            return;
        }
        let seat = slot_store::core_for_cart(&root, cart);
        let now = self.now();
        let mut picker = CorePicker::open(seat, Core::pair(cart.platform), now);
        if self.core_faces_ready() {
            picker.start(now);
        }
        self.core_picker = Some(picker);
        self.ask_played_plate();
        self.shelf_mut().release_hold();
        self.play_held = None;
    }

    fn core_faces_ready(&self) -> bool {
        self.core_faces_stem
            .as_deref()
            .is_some_and(|stem| self.selected_stem() == Some(stem))
    }

    fn core_picker_input(&mut self, action: Action) {
        let press = match action {
            Action::GbaDown(Btn::Left) | Action::ShelfLeft => Press::Left,
            Action::GbaDown(Btn::Right) | Action::ShelfRight => Press::Right,
            Action::GbaDown(Btn::A) => Press::Keep,
            Action::GbaDown(Btn::B) => Press::Back,
            Action::GbaDown(Btn::Y) => {
                self.game_delete = self.selected_game();
                return;
            }
            _ => return,
        };
        let now = self.now();
        let Some(picker) = &mut self.core_picker else {
            return;
        };
        let outcome = picker.press(press, now);
        if let Outcome::Write(core) = outcome {
            self.write_core(core);
        }
    }

    fn open_game_menu(&mut self) {
        // Link play is behind the `link` feature (on by default for PC runs and tests, off in the
        // Flip build, `--no-default-features --features device`): without it the Select+Menu
        // chord does nothing at all, and the radio is never warmed.
        if !cfg!(feature = "link") {
            return;
        }
        if self.game_menu.is_some() {
            return;
        }
        if self.link_active() {
            return self.refuse();
        }
        if self.core == Core::Mgba {
            let wireless = self
                .seated()
                .is_some_and(|stem| self.link_mode(stem).0 == LinkKind::Wireless);
            if wireless {
                self.hud.toast(Toast::NeedsGpsp, self.now());
                return;
            }
            self.link_hardware = LinkKind::Cable;
            self.radio.ask(RadioJob::Warm);
            self.game_menu = Some(GameMenu::Pick(self.last_role));
            return;
        }
        if self.platform != Platform::Gba {
            self.hud.toast(Toast::NoLink, self.now());
            return;
        }
        let carried = self
            .seated()
            .and_then(|stem| self.auto_link(stem))
            .is_some_and(|(cart, _)| link_carried(&cart.code, &cart.title));
        if !carried {
            self.hud.toast(Toast::NoLink, self.now());
            return;
        }
        if self.core != Core::Gpsp {
            self.hud.toast(Toast::NeedsGpsp, self.now());
            return;
        }
        let hardware = self
            .seated()
            .map_or(LinkKind::Cable, |stem| self.link_mode(stem).0);
        self.link_hardware = hardware;
        self.radio.ask(RadioJob::Warm);
        self.game_menu = Some(GameMenu::Pick(self.last_role));
    }

    fn game_menu_shortcut(&mut self) {
        if !cfg!(feature = "link") {
            return;
        }
        let Some(client_id) = self.link_client_id() else {
            return self.open_game_menu();
        };
        let now = self.now();
        self.game_menu = Some(GameMenu::Linked {
            role: LinkRow::from_client_id(client_id),
            worked: now,
            since: now,
            opened: true,
        });
    }

    fn end_link_from_menu(&mut self) {
        let role = self
            .link_client_id()
            .map_or(self.last_role, LinkRow::from_client_id);
        self.end_link();
        self.hud.toast(Toast::LinkEnded, self.now());
        self.unplug(role);
    }

    fn unplug(&mut self, role: LinkRow) {
        self.game_menu = Some(GameMenu::Unplug {
            role,
            since: self.now(),
        });
    }

    pub fn set_radio_jobs(&mut self, jobs: Box<dyn RadioJobs>) {
        self.radio = jobs;
    }

    fn game_menu_input(&mut self, action: Action) {
        let Some(menu) = self.game_menu else {
            return;
        };
        match menu {
            GameMenu::Pick(role) => match action {
                Action::GbaDown(Btn::Left) | Action::GbaDown(Btn::Right) => {
                    self.last_role = role.other();
                    self.game_menu = Some(GameMenu::Pick(role.other()));
                }
                Action::GbaDown(Btn::Select) => self.switch_hardware(),
                Action::GbaDown(Btn::A) => self.pick_link(role),
                Action::GbaDown(Btn::B) | Action::GameMenu => self.close_game_menu(),
                _ => {}
            },
            GameMenu::Working { .. } => {
                if action == Action::GbaDown(Btn::B) {
                    if let Some(starting) = &mut self.starting {
                        starting.starter.cancel();
                    }
                    if let Some(reload) = &mut self.reload {
                        reload.cancelled = true;
                    }
                }
            }
            GameMenu::Linked { opened: true, .. } => match action {
                Action::GbaDown(Btn::A) => self.end_link_from_menu(),
                Action::GbaDown(Btn::B) | Action::GameMenu => self.close_game_menu(),
                _ => {}
            },
            GameMenu::Linked { .. } => {}
            GameMenu::Failed { .. } => {
                if matches!(
                    action,
                    Action::GbaDown(Btn::A) | Action::GbaDown(Btn::B) | Action::GameMenu
                ) {
                    self.close_game_menu();
                }
            }
            GameMenu::Unplug { .. } => {}
        }
    }

    pub fn start_link(&mut self, starter: LinkStarter, client_id: u16) {
        self.start_link_from(starter, client_id, self.now());
    }

    fn start_link_from(&mut self, starter: LinkStarter, client_id: u16, since: Millis) {
        if let Some(mut old) = self.starting.replace(LinkStarting { starter, client_id }) {
            old.starter.cancel();
        }
        self.game_menu = Some(GameMenu::Working {
            role: LinkRow::from_client_id(client_id),
            step: LinkStep::Radio,
            since,
        });
    }

    fn switch_hardware(&mut self) {
        let Some(stem) = self.seated().map(str::to_string) else {
            return;
        };
        if !self.link_switchable(&stem) {
            return self.refuse();
        }
        let other = self.link_hardware.other();
        self.link_hardware = other;
        self.link_choices.insert(stem, other);
    }

    fn link_switchable(&self, stem: &str) -> bool {
        self.core == Core::Gpsp
            && self
                .auto_link(stem)
                .is_some_and(|(cart, _)| link_dual(&cart.code, &cart.title))
    }

    fn pick_link(&mut self, role: LinkRow) {
        let Some(stem) = self.seated().map(str::to_string) else {
            return;
        };
        let loaded = self.link_loaded.unwrap_or("auto");
        if self.core == Core::Mgba {
            let player = role.client_id() as u8;
            if self.link_player == Some(player) {
                return self.start_link(
                    LinkStarter::spawn(role.role(), link_port()),
                    role.client_id(),
                );
            }
            if self.snapshot.as_ref().is_some_and(|s| !s.resume_trusted()) {
                return self.refuse();
            }
            self.link_player = Some(player);
            self.link_reload = Some((stem.clone(), loaded));
            self.reload = Some(Reload {
                stem,
                role,
                cancelled: false,
                from: self.link_hardware,
                from_serial: loaded,
                fallback: false,
            });
            self.game_menu = Some(GameMenu::Working {
                role,
                step: LinkStep::Radio,
                since: self.now(),
            });
            return;
        }
        let (_, serial) = self.link_mode(&stem);
        if serial == loaded {
            return self.start_link(
                LinkStarter::spawn(role.role(), link_port()),
                role.client_id(),
            );
        }
        if self.snapshot.as_ref().is_some_and(|s| !s.resume_trusted()) {
            return self.refuse();
        }
        self.link_reload = Some((stem.clone(), serial));
        self.reload = Some(Reload {
            stem,
            role,
            cancelled: false,
            from: self.link_hardware.other(),
            from_serial: loaded,
            fallback: false,
        });
        self.game_menu = Some(GameMenu::Working {
            role,
            step: LinkStep::Radio,
            since: self.now(),
        });
    }

    fn refuse_seated(&mut self) {
        self.close_game_menu();
        self.pending = None;
        let caught = self.seat();
        let cart = match &mut self.phase {
            Phase::Playing { cart } => Some(std::mem::take(cart)),
            Phase::Doze { cart } => {
                *cart = None;
                None
            }
            _ => None,
        };
        if let Some(cart) = cart {
            self.refuse_out(cart, caught);
        }
    }

    fn close_game_menu(&mut self) {
        self.game_menu = None;
        if let Some(mut starting) = self.starting.take() {
            starting.starter.cancel();
        }
        if !self.link_active() {
            self.radio.ask(RadioJob::Cool);
        }
        let uncollected =
            self.link_reload.is_some() && self.reload.as_ref().is_some_and(|r| !r.fallback);
        if uncollected {
            self.link_reload = None;
            self.reload = None;
        } else if let Some(reload) = &mut self.reload {
            reload.cancelled = true;
        }
    }

    fn working_role(&self, client_id: u16) -> (LinkRow, Millis) {
        match self.game_menu {
            Some(GameMenu::Working { role, since, .. }) => (role, since),
            _ => (LinkRow::from_client_id(client_id), self.now()),
        }
    }

    fn poll_link(&mut self) {
        let Some(mut starting) = self.starting.take() else {
            return;
        };
        match starting.starter.poll() {
            None => self.starting = Some(starting),
            Some(LinkProgress::At(step)) => {
                if let Some(GameMenu::Working { role, since, .. }) = self.game_menu {
                    self.game_menu = Some(GameMenu::Working { role, step, since });
                }
                self.starting = Some(starting);
            }
            Some(LinkProgress::Ready(link)) => {
                let (role, worked) = self.working_role(starting.client_id);
                self.game_menu = Some(GameMenu::Linked {
                    role,
                    worked,
                    since: self.now(),
                    opened: false,
                });
                self.begin_link(starting.client_id);
                self.link_transport = Some((starting.client_id, Box::new(link)));
            }
            Some(LinkProgress::Failed(LinkFail::Cancelled)) => self.game_menu = None,
            Some(LinkProgress::Failed(fail)) => {
                let (role, worked) = self.working_role(starting.client_id);
                self.game_menu = Some(GameMenu::Failed {
                    role,
                    fail,
                    worked,
                    since: self.now(),
                });
            }
        }
    }

    fn write_core(&self, core: Core) {
        let (Some(root), Some(cart)) = (
            self.root.clone(),
            self.shelf().carts.get(self.shelf().index),
        ) else {
            return;
        };
        if let Err(e) = slot_store::write_selected_core(&root, &cart.stem, core) {
            eprintln!("slot: core: could not write selected_core.txt: {e}");
        }
    }

    /// Counts play while a game runs with no menu over it, and writes it to the card a minute at
    /// a time and when the game stops, so a session survives a crash but for its last minute.
    fn track_play(&mut self, dt: f32) {
        let game = match &self.phase {
            Phase::Playing { .. } if self.in_game_menu.is_none() && self.game_menu.is_none() => {
                self.seated_cart().map(|c| Recent {
                    platform: c.platform,
                    stem: c.stem.clone(),
                })
            }
            _ => None,
        };
        match (&mut self.play_session, game) {
            (Some((_, playing, unlogged)), Some(game)) if *playing == game => {
                *unlogged += dt;
                if *unlogged >= PLAY_LOG_SECS {
                    self.log_play(false);
                }
            }
            (_, game) => {
                self.end_play_session(false);
                let now = self.power.as_ref().map_or_else(system_secs, |p| p.now());
                self.play_session = game.map(|g| (now, g, 0.0));
            }
        }
    }

    /// Writes the play gathered so far; `now` waits for it, for a power off that is coming.
    fn log_play(&mut self, now: bool) {
        let (Some(root), Some((start, game, unlogged))) = (&self.root, &mut self.play_session)
        else {
            return;
        };
        let secs = std::mem::take(unlogged).round() as u32;
        self.extras_stale = true;
        let (root, start, game) = (root.clone(), *start, game.clone());
        let write = move || {
            if let Err(e) = slot_store::log_play(&root, start, secs, &game) {
                eprintln!("slot: activity: {e}");
            }
        };
        match now {
            true => write(),
            false => drop(std::thread::spawn(write)),
        }
    }

    fn end_play_session(&mut self, now: bool) {
        self.log_play(now);
        self.play_session = None;
    }

    fn begin_power_off(&mut self) {
        self.end_play_session(true);
        if self.powering_off {
            return;
        }
        if self.link_active() {
            self.end_link();
        }
        self.close_game_menu();
        self.remember_shelf();
        self.powering_off = true;
        self.act_at = self.now() + SHUTDOWN_SHOW_MS;
        self.set_led(LedState::Off);
    }

    pub fn on_battery(&mut self, b: Battery) {
        if b.percent > BATTERY_CRITICAL || self.powering_off {
            return;
        }
        if matches!(b.charge, Charge::Charging | Charge::Full) {
            return;
        }
        self.flush_resume();
        self.begin_power_off();
    }

    /// Auto Sleep: idle long enough on the shelf or in a game, with nothing else on screen, and
    /// not on a computer's cable, where a long copy shows as no input at all. Asked last, so the
    /// USB state is read only once the time is up.
    fn idle_expired(&self) -> bool {
        let min = self.state.auto_sleep_min;
        min > 0
            && !self.link_active()
            && matches!(self.phase, Phase::Shelf | Phase::Playing { .. })
            && self.now().saturating_sub(self.pressed_at) >= min as Millis * 60_000
            && !self.power.as_ref().is_some_and(|p| p.on_a_computer())
    }

    /// The screen is off for a shut lid or Auto Sleep.
    pub fn dozing(&self) -> bool {
        matches!(self.phase, Phase::Doze { .. })
    }

    /// A few seconds after the lid shuts, sleeps for real until it opens, the power key is
    /// pressed, or the lid's power-off time is up, when it powers off as before.
    fn sleep_when_settled(&mut self) {
        let settled = self.now().saturating_sub(self.dozed_at) >= SUSPEND_AFTER_MS;
        if !self.dozing() || self.slept_idle || self.no_suspend || !settled || self.link_active() {
            return;
        }
        let timeout = match (self.state.lid_off_min, &self.power) {
            (Some(min), _) => min as i64 * 60,
            (None, Some(power)) => power.timeout().as_secs() as i64,
            (None, None) => return,
        };
        self.settle_saves();
        let Some(power) = &mut self.power else {
            return;
        };
        let since = self.dozed_wall.unwrap_or_else(|| power.now());
        let left = (timeout - (power.now() - since)).max(1);
        if !power.suspend(Some(std::time::Duration::from_secs(left as u64))) {
            self.no_suspend = true;
            return;
        }
        if power.now() - since >= timeout {
            eprintln!("slot: power off: slept for the Lid Power Off time");
            return self.begin_power_off();
        }
        let headphones = power.headphones();
        power.route_audio(headphones);
        // Woken by the lid or the power key, whose press follows as input; if it was neither, the
        // next settle sleeps again with what is left of the time.
        self.dozed_at = self.now();
    }

    fn doze_expired(&self) -> bool {
        if self.link_active() {
            return false;
        }
        let (Phase::Doze { .. }, Some(power)) = (&self.phase, &self.power) else {
            return false;
        };
        let timeout = match self.state.lid_off_min {
            Some(min) => min as Millis * 60_000,
            None => power.timeout().as_millis() as Millis,
        };
        self.now().saturating_sub(self.dozed_at) >= timeout
    }

    pub fn settle_saves(&mut self) {
        self.state_writer.flush();
        if let Some(h) = self.pending_save.take() {
            let _ = h.join();
        }
    }

    fn autosave(&mut self) {
        self.autosave_at = self.now() + AUTOSAVE_MS;
        self.settle_saves();
        let (Some(root), Some(snapshot), Some(cart)) = (&self.root, &self.snapshot, self.seated())
        else {
            return;
        };
        let Some(state) = snapshot.state() else {
            eprintln!("slot: autosave: the core gave up no state");
            return;
        };
        let (state, sav) = trusted_write(snapshot.as_ref(), state, "autosave");
        let (root, platform, core, cart) =
            (root.clone(), self.platform, self.core, cart.to_owned());
        let failing = self.card_writes.clone();
        let write = move || {
            let written = persist::flush(
                &root,
                platform,
                core,
                &cart,
                state.as_deref(),
                sav.as_deref(),
            );
            if let Err(e) = &written {
                eprintln!("slot: autosave: {e}");
            }
            failing.wrote("autosave", &written);
        };
        match std::thread::Builder::new()
            .name("slot-autosave".into())
            .spawn(write)
        {
            Ok(h) => self.pending_save = Some(h),
            Err(e) => eprintln!("slot: autosave: no writer thread: {e}"),
        }
    }

    pub fn flush_resume(&mut self) {
        self.settle_saves();
        self.autosave_at = self.now() + AUTOSAVE_MS;
        let (Some(root), Some(snapshot), Some(cart)) = (&self.root, &self.snapshot, self.seated())
        else {
            return;
        };
        let Some(state) = snapshot.state() else {
            eprintln!("slot: flush: the core gave up no state");
            return;
        };
        let (state, sav) = trusted_write(snapshot.as_ref(), state, "flush");
        let written = persist::flush(
            root,
            self.platform,
            self.core,
            cart,
            state.as_deref(),
            sav.as_deref(),
        );
        if let Err(e) = &written {
            eprintln!("slot: flush: {e}");
        }
        self.card_writes.wrote("flush", &written);
    }

    fn ring(&self) -> Option<StateRing> {
        let (Some(root), Some(cart)) = (&self.root, self.seated()) else {
            return None;
        };
        Some(StateRing::new(root, self.platform, self.core, cart))
    }

    fn seated(&self) -> Option<&str> {
        match &self.phase {
            Phase::Playing { cart } | Phase::Polaroids { cart } => Some(cart),
            _ => None,
        }
    }

    fn entries(&self) -> Vec<StateEntry> {
        self.ring().and_then(|r| r.list().ok()).unwrap_or_default()
    }

    fn open_polaroids(&mut self) {
        if self.link_active() {
            return self.refuse();
        }
        let entries = self.entries();
        if entries.is_empty() {
            return self.refuse();
        }
        let Phase::Playing { cart } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        let mut p = Polaroids::new(entries);
        p.set_undo(self.undo_label());
        self.polaroids = Some(p);
        self.phase = Phase::Polaroids { cart };
        self.push_hint_faces();
    }

    fn close_polaroids(&mut self) {
        let Phase::Polaroids { cart } = &mut self.phase else {
            return;
        };
        let cart = std::mem::take(cart);
        self.polaroids = None;
        self.delete_prompt = false;
        self.phase = Phase::Playing { cart };
        if std::mem::take(&mut self.states_from_menu) {
            self.in_game_menu = Some(QuickRow::States);
        }
    }

    fn load_selected(&mut self) {
        let state = self
            .polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.state.clone());
        let refused = state.map(|state| self.load_file(&state)) == Some(false);
        if !refused {
            self.states_from_menu = false;
            self.close_polaroids();
        }
    }

    fn delete_selected(&mut self) {
        let stamp = self
            .polaroids
            .as_ref()
            .and_then(|p| p.selected())
            .map(|e| e.stamp.clone());
        let (Some(stamp), Some(ring)) = (stamp, self.ring()) else {
            return;
        };
        if let Err(e) = ring.remove(&stamp) {
            eprintln!("slot: delete: {e}");
            return;
        }
        if self.undo_targets(&stamp) {
            self.pending = None;
        }
        let Some(p) = &mut self.polaroids else {
            return;
        };
        p.remove_selected();
        if p.is_empty() {
            self.close_polaroids();
        }
    }

    fn undo_targets(&self, stamp: &str) -> bool {
        match self.pending.as_ref() {
            Some((PendingUndo::Save { stamp: pending, .. }, _)) => pending == stamp,
            _ => false,
        }
    }

    fn load_newest(&mut self) {
        let Some(newest) = self.entries().first().map(|e| e.state.clone()) else {
            return self.refuse();
        };
        self.load_file(&newest);
    }

    fn load_file(&mut self, state: &Path) -> bool {
        if !self.may_load_state() {
            self.refuse();
            return false;
        }
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        let bytes = match std::fs::read(state) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("slot: load: {e}");
                return false;
            }
        };
        let prior = snapshot.state();
        snapshot.load(bytes);
        self.hud.toast(Toast::StateLoaded, self.now());
        if let Some(prior) = prior {
            self.pending = Some((PendingUndo::Load { prior }, self.now()));
        }
        true
    }

    fn save_state(&mut self) {
        let (Some(ring), Some(snapshot)) = (self.ring(), &self.snapshot) else {
            return;
        };
        if !snapshot.resume_trusted() {
            eprintln!("slot: save: the core refused the resume it was given, not pushing a state");
            return self.refuse();
        }
        let Some(state) = snapshot.state() else {
            eprintln!("slot: save: the core gave up no state");
            return;
        };
        let thumb = snapshot.thumb().unwrap_or_default();
        let stamp = free_stamp(&ring, self.wall_secs());
        let evicted = doomed(&ring);
        let pushed = ring.push(&state, &thumb, &stamp);
        self.card_writes.wrote("save state", &pushed);
        if let Err(e) = pushed {
            eprintln!("slot: save: {e}");
            return;
        }
        self.hud.toast(Toast::StateSaved, self.now());
        self.pending = Some((PendingUndo::Save { stamp, evicted }, self.now()));
    }

    pub fn undo_available(&self, now: Millis) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|(_, at)| now.saturating_sub(*at) <= UNDO_GRACE_MS)
    }

    pub fn undo_label(&self) -> Option<&'static str> {
        if !self.undo_available(self.now()) {
            return None;
        }
        match self.pending.as_ref()?.0 {
            PendingUndo::Save { .. } => Some("undo save"),
            PendingUndo::Load { .. } => Some("undo load"),
        }
    }

    pub fn set_legend_faces(&mut self, faces: Vec<TexId>) {
        self.legend_faces = faces;
        self.push_hint_faces();
    }

    pub fn set_undo_face(&mut self, face: Option<TexId>) {
        self.undo_face = face;
        self.push_hint_faces();
    }

    pub fn set_games_legend_faces(&mut self, faces: Vec<TexId>) {
        self.games_legend_faces = faces;
        self.push_hint_faces();
    }

    fn push_hint_faces(&mut self) {
        let mut faces = match self.games {
            Some(_) => self.games_legend_faces.clone(),
            None => self.legend_faces.clone(),
        };
        faces.extend(self.undo_face.filter(|_| self.games.is_none()));
        if let Some(p) = &mut self.polaroids {
            p.set_hint_faces(faces);
        }
    }

    pub fn set_icon_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_icons(faces);
    }

    pub fn set_percent_faces(&mut self, faces: Vec<(TexId, u32, u32)>) {
        self.hud.set_percent_faces(faces);
    }

    pub fn set_toast_faces(&mut self, faces: Vec<TexId>) {
        self.hud.set_toasts(faces);
    }

    pub fn toast(&self) -> Option<Toast> {
        self.hud.said(self.now())
    }

    pub fn card_alarm(&self) -> bool {
        self.hud.alarmed()
    }

    pub fn alarm_toast(&self) -> Option<Toast> {
        self.hud.alarm_toast()
    }

    pub fn undo(&mut self, now: Millis) {
        if !self.undo_available(now) {
            self.pending = None;
            return;
        }
        if matches!(&self.pending, Some((PendingUndo::Load { .. }, _))) && !self.may_load_state() {
            return self.refuse();
        }
        let Some((what, _)) = self.pending.take() else {
            return;
        };
        match what {
            PendingUndo::Save { stamp, evicted } => self.undo_save(&stamp, evicted),
            PendingUndo::Load { prior } => {
                if let Some(snapshot) = &self.snapshot {
                    snapshot.load(prior);
                }
            }
        }
        self.close_polaroids();
    }

    fn undo_save(&self, stamp: &str, evicted: Option<(String, Vec<u8>, Vec<u8>)>) {
        let Some(ring) = self.ring() else {
            return;
        };
        if let Err(e) = ring.remove(stamp) {
            eprintln!("slot: undo: {e}");
            return;
        }
        let Some((stamp, state, thumb)) = evicted else {
            return;
        };
        if let Err(e) = ring.push(&state, &thumb, &stamp) {
            eprintln!("slot: undo: {e}");
        }
    }

    pub fn polaroid_entries(&self) -> &[StateEntry] {
        match &self.polaroids {
            Some(p) => &p.entries,
            None => &[],
        }
    }

    pub fn set_polaroid_faces(
        &mut self,
        faces: Vec<TexId>,
        game_boy: Vec<bool>,
        sizes: Vec<(u32, u32)>,
    ) {
        if let Some(p) = &mut self.polaroids {
            p.set_faces(faces);
            p.set_game_boy_faces(game_boy);
            p.set_face_sizes(sizes);
        }
    }

    pub fn polaroid_title(&self, now: &str) -> String {
        let Some(p) = &self.polaroids else {
            return String::new();
        };
        let Some(game) = self.games.as_ref().and_then(|g| g.get(p.index)) else {
            return p.title(now);
        };
        let title = self
            .carts()
            .find(|c| c.platform == game.platform && c.stem == game.stem)
            .map_or_else(|| game.stem.clone(), slot_ui::label_text);
        match p.selected().filter(|e| !e.stamp.is_empty()) {
            Some(e) => format!("{title} - {}", Polaroids::relative_time(&e.stamp, now)),
            None => title,
        }
    }

    pub fn polaroids_open(&self) -> bool {
        self.polaroids.is_some()
    }

    pub fn games_open(&self) -> bool {
        self.games.is_some()
    }

    fn game_switcher(&mut self) {
        match self.phase {
            Phase::Shelf if self.games.is_some() => self.close_games(),
            Phase::Shelf => self.open_games(),
            Phase::Playing { .. } | Phase::Polaroids { .. } if self.link_active() => self.refuse(),
            Phase::Playing { .. } | Phase::Polaroids { .. } => {
                self.close_polaroids();
                self.eject();
                self.games_after_eject = matches!(self.phase, Phase::Ejecting { .. });
            }
            _ => {}
        }
    }

    fn open_games(&mut self) {
        if self.shutting_down() || self.link_active() || self.core_picker.is_some() {
            return self.refuse();
        }
        let Some(root) = self.root.clone() else {
            return self.refuse();
        };
        let (games, entries): (Vec<Recent>, Vec<StateEntry>) = self
            .state
            .recents
            .iter()
            .filter_map(|r| {
                let cart = self
                    .carts()
                    .find(|c| c.platform == r.platform && c.stem == r.stem)?;
                Some((r.clone(), game_entry(&root, cart)))
            })
            .take(GAMES_MAX.min(self.state.switcher_games as usize))
            .unzip();
        if games.is_empty() {
            return self.refuse();
        }
        self.play_held = None;
        self.shelf_mut().release_hold();
        self.polaroids = Some(Polaroids::games(entries));
        self.games = Some(games);
        self.push_hint_faces();
    }

    fn close_games(&mut self) {
        if self.games.take().is_some() {
            self.polaroids = None;
        }
    }

    fn games_input(&mut self, action: Action) {
        match action {
            Action::ShelfLeft | Action::GbaDown(Btn::Left) => self.flick(Polaroids::left),
            Action::ShelfRight | Action::GbaDown(Btn::Right) => self.flick(Polaroids::right),
            Action::GbaDown(Btn::A) | Action::Insert => self.resume_game(false),
            Action::GbaDown(Btn::X) => self.resume_game(true),
            Action::GbaDown(Btn::Y) => self.forget_game(),
            Action::GbaDown(Btn::B) | Action::QuickMenu => self.close_games(),
            _ => {}
        }
    }

    /// Takes the chosen game off the switcher and the recent games; the game, its saves and its
    /// states stay on the card.
    fn forget_game(&mut self) {
        let index = self.polaroids.as_ref().map_or(0, |p| p.index);
        let Some(game) = self
            .games
            .as_mut()
            .filter(|g| index < g.len())
            .map(|g| g.remove(index))
        else {
            return;
        };
        eprintln!(
            "slot: switcher: removed {} from the recent games",
            game.stem
        );
        self.state.recents.retain(|g| *g != game);
        self.persist();
        self.restock_recents();
        if let Some(p) = &mut self.polaroids {
            p.remove_selected();
        }
        if self.games.as_ref().is_some_and(|g| g.is_empty()) {
            self.close_games();
        }
    }

    fn resume_game(&mut self, states: bool) {
        let index = self.polaroids.as_ref().map_or(0, |p| p.index);
        let Some(game) = self.games.as_ref().and_then(|g| g.get(index)).cloned() else {
            return;
        };
        let Some((at, i)) = self.find_seat(&game.stem, Some(game.platform)) else {
            return self.refuse();
        };
        if states && !self.game_has_states(&game) {
            return self.refuse();
        }
        self.close_games();
        self.go_to_shelf(at);
        self.shelf_mut().select(i);
        self.insert(!self.state.eject_save);
        self.states_after_insert = states && matches!(self.phase, Phase::Inserting { .. });
    }

    fn game_has_states(&self, game: &Recent) -> bool {
        self.root.as_ref().is_some_and(|root| {
            let core = match self
                .carts()
                .find(|c| c.platform == game.platform && c.stem == game.stem)
            {
                Some(cart) => slot_store::core_for_cart(root, cart),
                None => slot_store::core_for_platform(root, &game.stem, game.platform),
            };
            StateRing::new(root, game.platform, core, &game.stem)
                .list()
                .is_ok_and(|l| !l.is_empty())
        })
    }

    pub fn set_polaroid_title_face(&mut self, face: TexId) {
        if let Some(p) = &mut self.polaroids {
            p.set_title_face(Some(face));
        }
    }
}

fn game_entry(root: &Path, cart: &Cart) -> StateEntry {
    let core = slot_store::core_for_cart(root, cart);
    let ring = StateRing::new(root, cart.platform, core, &cart.stem);
    if let Some(thumb) = ring.resume_thumb() {
        return StateEntry {
            stamp: String::new(),
            state: PathBuf::new(),
            thumb,
        };
    }
    let newest = ring.list().ok().and_then(|l| l.into_iter().next());
    newest.unwrap_or_else(|| StateEntry {
        stamp: String::new(),
        state: PathBuf::new(),
        thumb: cart.label.clone().unwrap_or_default(),
    })
}

fn trusted_write(
    snapshot: &dyn Snapshot,
    state: Vec<u8>,
    verb: &str,
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let state = if snapshot.resume_trusted() {
        Some(state)
    } else {
        eprintln!(
            "slot: {verb}: the core refused the resume it was given, not overwriting the saved one"
        );
        None
    };
    let sav = snapshot.save_ram();
    let sav = if snapshot.save_ram_trusted() {
        sav
    } else {
        if sav.is_some() {
            eprintln!(
                "slot: {verb}: the core refused the save ram it was given, not overwriting the saved one"
            );
        }
        None
    };
    (state, sav)
}

fn up(level: u8, step: u8, max: u8) -> u8 {
    level.saturating_add(step).min(max)
}

fn shader_value(shader: Shader) -> QuickValue {
    match shader {
        Shader::Off => QuickValue::Off,
        Shader::Lcd3x => QuickValue::Lcd3x,
        Shader::Grid => QuickValue::Grid,
        Shader::Dot => QuickValue::Dot,
        Shader::Simpletex => QuickValue::Simpletex,
    }
}

fn tint_alpha(tint: WallpaperTint) -> f32 {
    match tint {
        WallpaperTint::Off => 0.0,
        WallpaperTint::Low => 0.25,
        WallpaperTint::Medium => 0.45,
        WallpaperTint::High => 0.65,
    }
}

fn ff_next(from: u8, right: bool) -> u8 {
    let n = FF_SPEEDS.len();
    let at = FF_SPEEDS.iter().position(|&v| v == from).unwrap_or(0);
    FF_SPEEDS[if right {
        (at + 1) % n
    } else {
        (at + n - 1) % n
    }]
}

fn clock_screen(utc: i64, offset_min: i16, from_menu: bool) -> Phase {
    Phase::SetClock {
        picker: ClockPicker::local(utc, offset_min),
        seed: utc - utc.rem_euclid(60),
        from_menu,
    }
}

fn system_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn doomed(ring: &StateRing) -> Option<(String, Vec<u8>, Vec<u8>)> {
    let entries = ring.list().ok()?;
    let oldest = entries.get(RING_MAX - 1)?;
    let (state, thumb) = ring.read(&oldest.stamp).ok()?;
    Some((oldest.stamp.clone(), state, thumb))
}

fn free_stamp(ring: &StateRing, now: i64) -> String {
    let taken: Vec<String> = ring
        .list()
        .map(|l| l.into_iter().map(|e| e.stamp).collect())
        .unwrap_or_default();
    let newest = taken.iter().filter_map(|s| parse_stamp(s)).max();
    let mut secs = newest.map_or(now, |n| now.max(n + 1));
    let mut stamp = format_stamp(secs);
    while taken.contains(&stamp) {
        secs += 1;
        stamp = format_stamp(secs);
    }
    stamp
}

fn bezel_slot(platform: Platform) -> usize {
    Platform::ALL
        .iter()
        .position(|p| *p == platform)
        .unwrap_or(0)
}

fn lerp_placed(a: Placed, b: Placed, t: f32) -> Placed {
    Placed {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
        w: a.w + (b.w - a.w) * t,
        h: a.h + (b.h - a.h) * t,
    }
}

/// Where the loop stands, in rows below the first shown: gliding from where it was towards `to`.
fn glide_at((from, at): (f32, Millis), to: f32, now: Millis) -> f32 {
    let t = ease((now.saturating_sub(at) as f32 / FOLDER_LOOP_GLIDE_MS).min(1.0));
    from + (to - from) * t
}

/// A sheet of the folder menu to draw: the settings, or the name entry.
pub enum FolderSheet {
    Menu {
        paper: Platform,
        name: String,
        rows: Vec<slot_ui::MenuRow>,
        first: usize,
    },
    Entry {
        paper: Platform,
        name: String,
        lower: bool,
    },
}

const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

/// The week the receipt prints.
pub struct ExtrasWeek {
    pub range: String,
    pub days: [u32; 7],
    pub labels: [String; 7],
    pub total: String,
}

/// Fast out, slow to rest, like the design's cubic-bezier(.2, .7, .2, 1).
fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}
