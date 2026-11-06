//! Cheats come as libretro `.cht` files the player copies to `Cheats/<system>/<game>.cht`, named
//! like the rom. Which of them are on is the player's own choice, kept per game in
//! `Config/cheats/<system>/<game>.txt` as the numbers of the cheats that are on, one a line, so
//! the `.cht` itself is never written and can be swapped for a newer copy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::platform::Platform;

pub const CHEATS_DIR: &str = "Cheats";
pub const CHEAT_CHOICES_DIR: &str = "Config/cheats";

/// One cheat from a `.cht`. `index` is its number in the file, which the core is handed and
/// the choices file names, so a cheat the file leaves without a code does not shift the rest.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cheat {
    pub index: u32,
    pub desc: String,
    pub code: String,
    pub enabled: bool,
}

pub fn cheat_file(root: &Path, platform: Platform, stem: &str) -> PathBuf {
    root.join(CHEATS_DIR)
        .join(platform.dir_name())
        .join(format!("{stem}.cht"))
}

fn choices_file(root: &Path, platform: Platform, stem: &str) -> PathBuf {
    root.join(CHEAT_CHOICES_DIR)
        .join(platform.dir_name())
        .join(format!("{stem}.txt"))
}

fn unquote(value: &str) -> &str {
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
}

/// Reads a libretro `.cht`: `cheats = N`, then `cheat{i}_desc`, `cheat{i}_code` and
/// `cheat{i}_enable` for each. A cheat with no code is RetroArch's own memory-search kind,
/// which a core cannot apply, so it is left out.
pub fn parse_cheats(text: &str) -> Vec<Cheat> {
    let mut count: Option<u32> = None;
    let mut found: BTreeMap<u32, Cheat> = BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), unquote(value));
        if key == "cheats" {
            count = value.parse().ok();
            continue;
        }
        let Some((index, field)) = key
            .strip_prefix("cheat")
            .and_then(|rest| rest.split_once('_'))
            .and_then(|(n, field)| Some((n.parse::<u32>().ok()?, field)))
        else {
            continue;
        };
        let cheat = found.entry(index).or_insert_with(|| Cheat {
            index,
            desc: String::new(),
            code: String::new(),
            enabled: false,
        });
        match field {
            "desc" => cheat.desc = value.to_string(),
            "code" => cheat.code = value.to_string(),
            "enable" => cheat.enabled = value.eq_ignore_ascii_case("true") || value == "1",
            _ => {}
        }
    }
    found
        .into_values()
        .filter(|c| count.is_none_or(|n| c.index < n))
        .filter(|c| !c.code.trim().is_empty())
        .map(|mut c| {
            if c.desc.trim().is_empty() {
                c.desc = format!("Cheat {}", c.index + 1);
            }
            c
        })
        .collect()
}

/// The game's cheats, on as the player last left them, or as the `.cht` says before then.
/// No `.cht` means no cheats.
pub fn read_cheats(root: &Path, platform: Platform, stem: &str) -> Vec<Cheat> {
    let Ok(text) = std::fs::read_to_string(cheat_file(root, platform, stem)) else {
        return Vec::new();
    };
    let mut cheats = parse_cheats(&text);
    if let Ok(chosen) = std::fs::read_to_string(choices_file(root, platform, stem)) {
        let on: Vec<u32> = chosen
            .lines()
            .filter_map(|l| l.trim().parse().ok())
            .collect();
        for cheat in &mut cheats {
            cheat.enabled = on.contains(&cheat.index);
        }
    }
    cheats
}

/// Keeps which cheats are on, for the next time the game loads.
pub fn write_cheat_choices(
    root: &Path,
    platform: Platform,
    stem: &str,
    cheats: &[Cheat],
) -> std::io::Result<()> {
    let text: String = cheats
        .iter()
        .filter(|c| c.enabled)
        .map(|c| format!("{}\n", c.index))
        .collect();
    let path = choices_file(root, platform, stem);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::atomic::atomic_write(&path, text.as_bytes())
}
