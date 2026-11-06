use std::path::Path;

use slot_gfx::{TvFit, SRC_H, SRC_W, WHOLE_TEXTURE};
use slot_store::{ini, Platform};

pub const VIDEO_MODE_FILE: &str = "Config/video_mode.txt";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VideoMode {
    #[default]
    Actual,
    Stretch,
}

impl VideoMode {
    pub fn as_str(self) -> &'static str {
        match self {
            VideoMode::Actual => "actual",
            VideoMode::Stretch => "stretch",
        }
    }

    pub fn parse(s: &str) -> Option<VideoMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "actual" => Some(VideoMode::Actual),
            "stretch" => Some(VideoMode::Stretch),
            _ => None,
        }
    }

    /// The mode a game starts in before one is chosen: a TV console's fills the screen at 4:3,
    /// a handheld's shows its own pixels.
    pub fn default_for(platform: Platform) -> VideoMode {
        match platform.on_a_tv() {
            true => VideoMode::Stretch,
            false => VideoMode::Actual,
        }
    }
}

pub fn video_mode_for(root: &Path, stem: &str) -> VideoMode {
    ini::value(root, VIDEO_MODE_FILE, stem)
        .as_deref()
        .and_then(VideoMode::parse)
        .unwrap_or_default()
}

/// A game's line in video_mode.txt: its name, and for a TV console its system before it, so a
/// Game Boy game of the same name keeps a setting of its own.
pub fn video_mode_key(stem: &str, platform: Platform) -> String {
    match platform.on_a_tv() {
        true => format!("{}/{stem}", platform.dir_name()),
        false => stem.to_string(),
    }
}

/// The game's chosen mode, or its platform's default when none was chosen.
pub fn video_mode_for_platform(root: &Path, stem: &str, platform: Platform) -> VideoMode {
    ini::value(root, VIDEO_MODE_FILE, &video_mode_key(stem, platform))
        .as_deref()
        .and_then(VideoMode::parse)
        .unwrap_or_else(|| VideoMode::default_for(platform))
}

pub fn write_video_mode(
    root: &Path,
    stem: &str,
    platform: Platform,
    mode: VideoMode,
) -> std::io::Result<()> {
    ini::write(
        root,
        VIDEO_MODE_FILE,
        &video_mode_key(stem, platform),
        mode.as_str(),
    )
}

/// How a TV console's picture fills the screen: stretched is the 4:3 a television showed (the
/// whole 640x480 Flip panel), actual the sharp whole-number scale (2x: 512x480 NES, 512x448
/// SNES). None for the handhelds, which keep the GBA screen.
pub fn tv_fit(platform: Platform, mode: VideoMode) -> Option<TvFit> {
    if !platform.on_a_tv() {
        return None;
    }
    Some(match mode {
        VideoMode::Stretch => TvFit::Tv,
        VideoMode::Actual => TvFit::Sharp,
    })
}

pub fn source_rect(platform: Platform, mode: VideoMode) -> [f32; 4] {
    let (w, h) = platform.picture();
    if mode == VideoMode::Actual || (w, h) == (SRC_W, SRC_H) || platform.on_a_tv() {
        return WHOLE_TEXTURE;
    }
    let x = SRC_W.saturating_sub(w) / 2;
    [x as f32 / SRC_W as f32, 0.0, w as f32 / SRC_W as f32, 1.0]
}

/// The part of the game texture that is picture, as [x0, y0, x1, y1]: the Game Boy's 160x144
/// in the middle of its 240x160 frame, else all of it. A TV console's frame is all picture
/// whatever size it comes in.
pub fn picture_uv(platform: Platform) -> [f32; 4] {
    if platform.on_a_tv() {
        return [0.0, 0.0, 1.0, 1.0];
    }
    let (w, h) = platform.picture();
    let (sw, sh) = (SRC_W as f32, SRC_H as f32);
    let (x, y) = ((sw - w as f32) / 2.0, (sh - h as f32) / 2.0);
    [x / sw, y / sh, (x + w as f32) / sw, (y + h as f32) / sh]
}
