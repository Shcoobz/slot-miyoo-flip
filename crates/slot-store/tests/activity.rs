use slot_store::{
    activity, civil, local_day, log_play, play_time_text, played, read_activity, weekday, Platform,
    Recent,
};

fn game(stem: &str) -> Recent {
    Recent {
        platform: Platform::Gba,
        stem: stem.into(),
    }
}

/// 2026-10-22 12:00 UTC, a Thursday.
const THURSDAY_NOON: i64 = 1_792_670_400;

#[test]
fn days_and_weekdays_come_out_of_unix_time() {
    assert_eq!(civil(0), (1970, 1, 1));
    let today = local_day(THURSDAY_NOON, 0);
    assert_eq!(civil(today), (2026, 10, 22));
    assert_eq!(weekday(today), 3, "Thursday is the fourth day from Monday");
    assert_eq!(
        local_day(THURSDAY_NOON + 11 * 3600 + 1800, 60),
        today + 1,
        "half past midnight in UTC+1 is already the next day"
    );
}

#[test]
fn a_session_written_in_pieces_reads_back_as_one() {
    let d = tempfile::tempdir().unwrap();
    log_play(d.path(), THURSDAY_NOON, 60, &game("Golden Sun")).unwrap();
    log_play(d.path(), THURSDAY_NOON, 45, &game("Golden Sun")).unwrap();
    log_play(d.path(), THURSDAY_NOON + 7200, 30, &game("Golden Sun")).unwrap();
    log_play(d.path(), THURSDAY_NOON, 0, &game("Never Logged")).unwrap();
    let sessions = read_activity(d.path());
    let secs: Vec<u32> = sessions.iter().map(|s| s.secs).collect();
    assert_eq!(secs, [105, 30]);
    assert_eq!(
        played(&sessions, &game("Golden Sun"), 0).map(|p| p.0),
        Some(135)
    );
    assert_eq!(played(&sessions, &game("Never Logged"), 0), None);
}

#[test]
fn the_report_adds_up_the_week_the_month_and_the_habits() {
    let d = tempfile::tempdir().unwrap();
    let day = 86_400;
    log_play(d.path(), THURSDAY_NOON - 3 * day, 2700, &game("Emerald")).unwrap();
    log_play(d.path(), THURSDAY_NOON - day, 600, &game("Tetris")).unwrap();
    log_play(d.path(), THURSDAY_NOON, 7800, &game("Emerald")).unwrap();
    log_play(d.path(), THURSDAY_NOON - 40 * day, 3600, &game("Zelda")).unwrap();
    let today = local_day(THURSDAY_NOON, 0);
    let a = activity(&read_activity(d.path()), today, 0);
    assert_eq!(a.week, [2700, 0, 600, 7800, 0, 0, 0]);
    assert_eq!(a.this_week, 11_100);
    assert_eq!(a.this_month, 11_100, "September's session is not October's");
    assert_eq!(a.all_time, 14_700);
    assert_eq!(a.most_played[0], (game("Emerald"), 10_500));
    assert_eq!(a.last_played[0].0, game("Emerald"));
    assert_eq!(a.sessions, 4);
    assert_eq!(a.longest, 7800);
    assert_eq!(a.streak, 2, "Wednesday and Thursday in a row");
}

#[test]
fn play_time_reads_as_the_design_writes_it() {
    assert_eq!(play_time_text(45 * 60), "45 M");
    assert_eq!(play_time_text(130 * 60), "2 H 10 M");
    assert_eq!(play_time_text(64 * 3600 + 600), "64 H 10 M");
    assert_eq!(play_time_text(12 * 3600), "12 H 00 M");
}

#[test]
fn writers_at_the_same_time_never_tear_a_line() {
    let d = tempfile::tempdir().unwrap();
    let writers: Vec<_> = (0..8)
        .map(|n| {
            let root = d.path().to_path_buf();
            std::thread::spawn(move || {
                for _ in 0..25 {
                    log_play(&root, THURSDAY_NOON + n, 1, &game("Pokemon Emerald")).unwrap();
                }
            })
        })
        .collect();
    for w in writers {
        w.join().unwrap();
    }
    let sessions = read_activity(d.path());
    assert_eq!(sessions.len(), 8);
    assert!(sessions.iter().all(|s| s.secs == 25), "{sessions:?}");
}
