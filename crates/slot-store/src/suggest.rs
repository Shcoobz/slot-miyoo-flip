//! Get lucky and Never played draw from one shared deck: `Config/suggested.txt` lists the games
//! either has offered, and a game comes up again only once every other game in the pool has.
//! Each picks among all systems or one, as `Config/extras.txt` says (`lucky.system = gba`).

use std::path::Path;

use crate::platform::Platform;
use crate::Recent;

pub const SUGGESTED_FILE: &str = "Config/suggested.txt";
pub const EXTRAS_FILE: &str = "Config/extras.txt";

fn platform_of(dir: &str) -> Option<Platform> {
    Platform::ALL.into_iter().find(|p| p.dir_name() == dir)
}

pub fn read_suggested(root: &Path) -> Vec<Recent> {
    let text = std::fs::read_to_string(root.join(SUGGESTED_FILE)).unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let (dir, stem) = line.split_once('/')?;
            Some(Recent {
                platform: platform_of(dir)?,
                stem: stem.to_string(),
            })
        })
        .collect()
}

pub fn write_suggested(root: &Path, suggested: &[Recent]) -> std::io::Result<()> {
    let text: String = suggested
        .iter()
        .map(|r| format!("{}/{}\n", r.platform.dir_name(), r.stem))
        .collect();
    let path = root.join(SUGGESTED_FILE);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::atomic::atomic_write(&path, text.as_bytes())
}

/// Picks a game from `pool` that the deck has not offered yet, `roll` deciding which, and notes
/// it. When the pool is used up its games go back in, all but the one offered last.
pub fn pick(pool: &[Recent], suggested: &mut Vec<Recent>, roll: u64) -> Option<Recent> {
    if pool.is_empty() {
        return None;
    }
    let fresh = |suggested: &[Recent]| -> Vec<Recent> {
        pool.iter()
            .filter(|g| !suggested.contains(g))
            .cloned()
            .collect()
    };
    let mut open = fresh(suggested);
    if open.is_empty() {
        let last = suggested.last().cloned();
        suggested.retain(|g| !pool.contains(g));
        open = fresh(suggested);
        if open.len() > 1 {
            open.retain(|g| Some(g) != last.as_ref());
        }
    }
    let game = open[(roll % open.len() as u64) as usize].clone();
    suggested.push(game.clone());
    Some(game)
}

/// Which systems an Extras pick draws from: all, or one.
pub fn extras_system(root: &Path, item: &str) -> Option<Platform> {
    let value = crate::ini::value(root, EXTRAS_FILE, &format!("{item}.system"))?;
    Platform::ALL
        .into_iter()
        .find(|p| p.dir_name().eq_ignore_ascii_case(&value))
}

pub fn write_extras_system(
    root: &Path,
    item: &str,
    system: Option<Platform>,
) -> std::io::Result<()> {
    crate::ini::put(
        root,
        EXTRAS_FILE,
        &format!("{item}.system"),
        Some(system.map_or("all", |p| p.dir_name())),
    )
}
