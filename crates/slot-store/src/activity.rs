//! Play time, as `Config/activity.txt` keeps it: a line per stretch of play written as it goes,
//! `<session start, unix seconds> <seconds played> <GBA|GB|GBC>/<game>`. A session longer than a
//! minute writes several lines with the same start; reading sums them.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use crate::platform::Platform;
use crate::Recent;

pub const ACTIVITY_FILE: &str = "Config/activity.txt";

const DAY: i64 = 86_400;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Session {
    pub start: i64,
    pub secs: u32,
    pub game: Recent,
}

fn platform_of(dir: &str) -> Option<Platform> {
    Platform::ALL.into_iter().find(|p| p.dir_name() == dir)
}

fn parse(line: &str) -> Option<Session> {
    let mut parts = line.splitn(3, ' ');
    let start = parts.next()?.parse().ok()?;
    let secs = parts.next()?.parse().ok()?;
    let (dir, stem) = parts.next()?.split_once('/')?;
    Some(Session {
        start,
        secs,
        game: Recent {
            platform: platform_of(dir)?,
            stem: stem.to_string(),
        },
    })
}

/// Every session on the card, oldest first, its lines summed. Lines that do not read are skipped.
pub fn read_activity(root: &Path) -> Vec<Session> {
    let text = std::fs::read_to_string(root.join(ACTIVITY_FILE)).unwrap_or_default();
    let mut sessions: Vec<Session> = Vec::new();
    let mut at: HashMap<(i64, Platform, String), usize> = HashMap::new();
    for s in text.lines().filter_map(parse) {
        let key = (s.start, s.game.platform, s.game.stem.clone());
        match at.get(&key) {
            Some(i) => sessions[*i].secs += s.secs,
            None => {
                at.insert(key, sessions.len());
                sessions.push(s);
            }
        }
    }
    sessions.sort_by_key(|s| s.start);
    sessions
}

/// Adds `secs` of play to the session that began at `start`.
pub fn log_play(root: &Path, start: i64, secs: u32, game: &Recent) -> std::io::Result<()> {
    if secs == 0 || game.stem.contains('\n') {
        return Ok(());
    }
    // Writers take turns, and each line goes out in one write, so two never interleave.
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let path = root.join(ACTIVITY_FILE);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let line = format!(
        "{start} {secs} {}/{}\n",
        game.platform.dir_name(),
        game.stem
    );
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())?;
    file.sync_data()
}

/// What the activity report and a game's START plate show, worked out for local day `today`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Activity {
    /// Seconds played on each day of the week holding `today`, Monday first.
    pub week: [u32; 7],
    pub this_week: u32,
    pub this_month: u32,
    pub all_time: u32,
    /// Games by time played, most first.
    pub most_played: Vec<(Recent, u32)>,
    /// Games by when they were last played, latest first, with that day.
    pub last_played: Vec<(Recent, i64)>,
    pub by_system: Vec<(Platform, u32)>,
    pub sessions: usize,
    pub longest: u32,
    /// Days in a row up to today (or yesterday) with any play.
    pub streak: u32,
}

/// The local day number (days since 1970-01-01 in local time) of a unix time.
pub fn local_day(unix: i64, utc_offset_min: i32) -> i64 {
    (unix + utc_offset_min as i64 * 60).div_euclid(DAY)
}

/// 0 for Monday: 1970-01-01 was a Thursday.
pub fn weekday(day: i64) -> usize {
    (day + 3).rem_euclid(7) as usize
}

pub fn activity(sessions: &[Session], today: i64, utc_offset_min: i32) -> Activity {
    let monday = today - weekday(today) as i64;
    let (year, month, _) = civil(today);
    let mut a = Activity::default();
    let mut per_game: HashMap<(Platform, String), (u32, i64)> = HashMap::new();
    let mut per_system: HashMap<Platform, u32> = HashMap::new();
    let mut days: Vec<i64> = Vec::new();
    for s in sessions {
        let day = local_day(s.start, utc_offset_min);
        if (monday..monday + 7).contains(&day) {
            a.week[(day - monday) as usize] += s.secs;
            a.this_week += s.secs;
        }
        let (y, m, _) = civil(day);
        if (y, m) == (year, month) {
            a.this_month += s.secs;
        }
        a.all_time += s.secs;
        a.sessions += 1;
        a.longest = a.longest.max(s.secs);
        *per_system.entry(s.game.platform).or_default() += s.secs;
        let game = per_game
            .entry((s.game.platform, s.game.stem.clone()))
            .or_default();
        game.0 += s.secs;
        game.1 = game.1.max(day);
        days.push(day);
    }
    let mut games: Vec<(Recent, u32, i64)> = per_game
        .into_iter()
        .map(|((platform, stem), (secs, last))| (Recent { platform, stem }, secs, last))
        .collect();
    games.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.stem.cmp(&b.0.stem)));
    a.most_played = games
        .iter()
        .map(|(g, secs, _)| (g.clone(), *secs))
        .collect();
    games.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.stem.cmp(&b.0.stem)));
    a.last_played = games.iter().map(|(g, _, day)| (g.clone(), *day)).collect();
    a.by_system = Platform::ALL
        .into_iter()
        .map(|p| (p, per_system.get(&p).copied().unwrap_or(0)))
        .collect();
    days.sort_unstable();
    days.dedup();
    let mut day = if days.binary_search(&today).is_ok() {
        today
    } else {
        today - 1
    };
    while days.binary_search(&day).is_ok() {
        a.streak += 1;
        day -= 1;
    }
    a
}

/// One game's time played and the last day it was played on.
pub fn played(sessions: &[Session], game: &Recent, utc_offset_min: i32) -> Option<(u32, i64)> {
    let mine = sessions.iter().filter(|s| s.game == *game);
    mine.fold(None, |acc, s| {
        let day = local_day(s.start, utc_offset_min);
        let (secs, last) = acc.unwrap_or((0, day));
        Some((secs + s.secs, last.max(day)))
    })
}

/// `45 M`, `2 H 10 M`, `64 H 10 M`: a space between number and unit, as the design writes it.
pub fn play_time_text(secs: u32) -> String {
    let minutes = secs / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} M"),
        (h, m) => format!("{h} H {m:02} M"),
    }
}

/// Year, month and day of a day number (Howard Hinnant's civil-from-days).
pub fn civil(day: i64) -> (i64, u32, u32) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}
