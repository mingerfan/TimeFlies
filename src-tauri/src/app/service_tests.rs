use super::*;

fn database() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE tasks (
            id TEXT PRIMARY KEY, parent_id TEXT, title TEXT, status TEXT,
            created_at INTEGER, archived_at INTEGER
        );
        CREATE TABLE time_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT, task_id TEXT,
            event_type TEXT, ts INTEGER, payload TEXT
        );
        CREATE TABLE tags (id TEXT, name TEXT);
        CREATE TABLE task_tags (task_id TEXT, tag_id TEXT);
        CREATE TABLE rest_suggestions (
            id INTEGER, trigger_type TEXT, task_id TEXT, focus_seconds INTEGER,
            switch_count_30m INTEGER, deviation_ratio REAL, suggested_minutes INTEGER,
            reasons TEXT, status TEXT, created_at INTEGER
        );
        CREATE TABLE notifications (
            id INTEGER, kind TEXT, level TEXT, status TEXT, title TEXT, message TEXT,
            detail TEXT, created_at INTEGER, rest_suggestion_id INTEGER
        );",
    )
    .unwrap();
    conn
}

fn task(conn: &Connection, id: &str, parent: Option<&str>) {
    conn.execute(
        "INSERT INTO tasks VALUES (?1, ?2, ?1, 'stopped', 0, NULL)",
        params![id, parent],
    )
    .unwrap();
}

fn event(conn: &Connection, task_id: &str, event_type: &str, ts: i64, delta: Option<i64>) {
    let payload = delta.map(|delta| json!({"delta_seconds": delta}).to_string());
    conn.execute(
        "INSERT INTO time_events (task_id, event_type, ts, payload) VALUES (?1, ?2, ?3, ?4)",
        params![task_id, event_type, ts, payload],
    )
    .unwrap();
}

fn seconds(conn: &Connection, task_id: &str, start: Option<i64>, end: i64) -> i64 {
    replay_exclusive_seconds(conn, start, end)
        .unwrap()
        .get(task_id)
        .copied()
        .unwrap_or(0)
}

#[test]
fn cross_day_deduction_does_not_swallow_new_focus_or_other_tasks() {
    let conn = database();
    let midnight = local_day_start_ts(1_700_000_000);
    event(&conn, "a", EVENT_START, midnight - 7200, None);
    event(&conn, "a", EVENT_STOP, midnight, None);
    event(&conn, "a", EVENT_START, midnight, None);
    event(&conn, "a", EVENT_ADJUST, midnight + 600, Some(-3600));
    assert_eq!(seconds(&conn, "a", Some(midnight), midnight + 600), 0);
    assert_eq!(seconds(&conn, "a", None, midnight + 600), 4200);
    assert_eq!(seconds(&conn, "a", Some(midnight), midnight + 660), 60);
    event(&conn, "a", EVENT_STOP, midnight + 660, None);
    event(&conn, "b", EVENT_START, midnight + 660, None);
    event(&conn, "b", EVENT_STOP, midnight + 960, None);
    assert_eq!(seconds(&conn, "a", None, midnight + 960), 4260);
    assert_eq!(seconds(&conn, "b", Some(midnight), midnight + 960), 300);
    let (intervals, _) = collect_focus_history(&conn, None, midnight + 960).unwrap();
    assert!(intervals.iter().any(|interval| {
        interval.task_id == "a"
            && interval.start_ts == midnight - 7200
            && interval.end_ts == midnight - 3000
    }));
}

#[test]
fn cross_midnight_session_deducts_latest_seconds_first() {
    let conn = database();
    let midnight = local_day_start_ts(1_700_000_000);
    event(&conn, "a", EVENT_START, midnight - 600, None);
    event(&conn, "a", EVENT_ADJUST, midnight + 600, Some(-900));
    event(&conn, "a", EVENT_STOP, midnight + 600, None);
    assert_eq!(seconds(&conn, "a", None, midnight + 600), 300);
    assert_eq!(seconds(&conn, "a", Some(midnight), midnight + 600), 0);
    let (intervals, _) = collect_focus_history(&conn, None, midnight + 600).unwrap();
    assert_eq!(intervals.len(), 1);
    assert_eq!(intervals[0].end_ts, midnight - 300);
}

#[test]
fn running_adjustments_reduce_lifetime_balance_and_reject_repeated_overdraft() {
    let mut conn = database();
    task(&conn, "a", None);
    let now = now_ts();
    event(&conn, "a", EVENT_START, now - 7200, None);
    event(&conn, "a", EVENT_STOP, now - 3600, None);
    event(&conn, "a", EVENT_START, now - 600, None);
    event(&conn, "a", EVENT_ADJUST, now, Some(-1800));
    assert_eq!(task_total_focus_seconds(&conn, "a", now).unwrap(), 2400);
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM time_events", [], |r| r.get(0))
        .unwrap();
    assert!(adjust_task_focus(&mut conn, "a".into(), -2460).is_err());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM time_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        count
    );
    event(&conn, "a", EVENT_ADJUST, now, Some(-1200));
    assert_eq!(task_total_focus_seconds(&conn, "a", now).unwrap(), 1200);
}

#[test]
fn adjustments_before_and_after_pause_at_same_second_have_equal_totals() {
    for adjust_before_pause in [false, true] {
        let conn = database();
        event(&conn, "a", EVENT_START, 100, None);
        event(&conn, "a", EVENT_PAUSE, 3700, None);
        event(&conn, "a", EVENT_RESUME, 4000, None);
        if adjust_before_pause {
            event(&conn, "a", EVENT_ADJUST, 4600, Some(-1800));
        }
        event(&conn, "a", EVENT_PAUSE, 4600, None);
        if !adjust_before_pause {
            event(&conn, "a", EVENT_ADJUST, 4600, Some(-1800));
        }
        assert_eq!(task_total_focus_seconds(&conn, "a", 4600).unwrap(), 2400);
    }
}

#[test]
fn positive_adjustments_are_credited_on_their_day_and_can_be_deducted() {
    let conn = database();
    let midnight = local_day_start_ts(1_700_000_000);
    event(&conn, "a", EVENT_ADJUST, midnight - 1, Some(3600));
    event(&conn, "a", EVENT_ADJUST, midnight, Some(1800));
    event(&conn, "a", EVENT_ADJUST, midnight, Some(-2400));
    assert_eq!(seconds(&conn, "a", Some(midnight), midnight), 0);
    assert_eq!(seconds(&conn, "a", None, midnight), 3000);
    event(&conn, "a", EVENT_START, midnight, None);
    event(&conn, "a", EVENT_STOP, midnight + 60, None);
    assert_eq!(seconds(&conn, "a", Some(midnight), midnight + 60), 60);
}

#[test]
fn deductions_consume_new_running_time_before_earlier_manual_credit() {
    let conn = database();
    event(&conn, "a", EVENT_ADJUST, 100, Some(1000));
    event(&conn, "a", EVENT_START, 200, None);
    event(&conn, "a", EVENT_ADJUST, 400, Some(-250));
    event(&conn, "a", EVENT_STOP, 460, None);
    assert_eq!(seconds(&conn, "a", None, 460), 1010);
    assert_eq!(seconds(&conn, "a", Some(300), 460), 60);
    let (intervals, adjustments) = collect_focus_history(&conn, None, 460).unwrap();
    assert_eq!(intervals.len(), 1);
    assert_eq!(intervals[0].start_ts, 400);
    assert_eq!(adjustments[0].delta_seconds, 950);
}

#[test]
fn legacy_overdraft_is_not_a_debt_against_future_time() {
    let conn = database();
    event(&conn, "a", EVENT_START, 100, None);
    event(&conn, "a", EVENT_ADJUST, 200, Some(-1000));
    event(&conn, "a", EVENT_PAUSE, 260, None);
    assert_eq!(seconds(&conn, "a", None, 260), 60);
    event(&conn, "a", EVENT_ADJUST, 260, Some(i64::MIN));
    event(&conn, "a", EVENT_RESUME, 260, None);
    event(&conn, "a", EVENT_STOP, 320, None);
    assert_eq!(seconds(&conn, "a", None, 320), 60);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM time_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        6
    );
}

#[test]
fn replay_excludes_future_events_and_respects_same_second_event_order() {
    let conn = database();
    event(&conn, "a", EVENT_START, 100, None);
    event(&conn, "a", EVENT_STOP, 200, None);
    event(&conn, "a", EVENT_ADJUST, 200, Some(-200));
    event(&conn, "a", EVENT_ADJUST, 200, Some(50));
    event(&conn, "a", EVENT_START, 201, None);
    event(&conn, "a", EVENT_ADJUST, 300, Some(-100));
    event(&conn, "b", EVENT_START, 1000, None);
    assert_eq!(seconds(&conn, "a", None, 250), 99);
    assert_eq!(seconds(&conn, "a", None, 300), 49);
    assert_eq!(seconds(&conn, "b", None, 250), 0);
}

#[test]
fn summary_and_overview_share_adjusted_history_and_parent_totals() {
    let conn = database();
    task(&conn, "parent", None);
    task(&conn, "a", Some("parent"));
    task(&conn, "b", None);
    let first_day = shift_local_day_start(local_day_start_ts(now_ts()), -2);
    let second_day = shift_local_day_start(first_day, 1);
    event(&conn, "a", EVENT_START, first_day + 3600, None);
    event(&conn, "a", EVENT_STOP, first_day + 10800, None);
    event(&conn, "a", EVENT_START, second_day, None);
    event(&conn, "a", EVENT_ADJUST, second_day + 600, Some(-3600));
    event(&conn, "a", EVENT_STOP, second_day + 660, None);
    event(&conn, "b", EVENT_START, second_day + 660, None);
    event(&conn, "b", EVENT_STOP, second_day + 960, None);
    let overview = get_overview(&conn, Some("all".into())).unwrap();
    let summary = get_focus_summary(&conn, Some("all".into())).unwrap();
    let total = overview
        .tasks
        .iter()
        .map(|task| task.exclusive_seconds)
        .sum::<i64>();
    assert_eq!(total, 4560);
    assert_eq!(
        summary
            .days
            .iter()
            .map(|day| day.total_focus_seconds)
            .sum::<i64>(),
        total
    );
    assert_eq!(
        overview
            .tasks
            .iter()
            .find(|task| task.id == "parent")
            .unwrap()
            .inclusive_seconds,
        4260
    );
    let day = summary
        .days
        .iter()
        .find(|day| day.day_start_ts == second_day)
        .unwrap();
    assert_eq!(day.total_focus_seconds, 360);
    assert_eq!(
        day.timeline_segments
            .iter()
            .map(|segment| segment.duration_seconds)
            .sum::<i64>(),
        360
    );
    assert_eq!(seconds(&conn, "a", Some(second_day), now_ts()), 60);
}

#[test]
fn adjustment_only_task_is_in_all_time_summary() {
    let conn = database();
    task(&conn, "a", None);
    let ts = shift_local_day_start(local_day_start_ts(now_ts()), -1);
    event(&conn, "a", EVENT_ADJUST, ts, Some(1800));
    let summary = get_focus_summary(&conn, Some("all".into())).unwrap();
    let day = summary
        .days
        .iter()
        .find(|day| day.day_start_ts == ts)
        .unwrap();
    assert_eq!(day.total_focus_seconds, 1800);
    assert!(day.timeline_segments.is_empty());
}

#[test]
#[ignore = "requires TIMEFLIES_TEST_SNAPSHOT pointing to an offline backup"]
fn offline_snapshot_overview_and_summary_are_consistent() {
    let path = std::env::var("TIMEFLIES_TEST_SNAPSHOT").unwrap();
    let conn =
        Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let overview = get_overview(&conn, Some("today".into())).unwrap();
    let summary = get_focus_summary(&conn, Some("today".into())).unwrap();
    assert!(overview
        .tasks
        .iter()
        .all(|task| task.exclusive_seconds >= 0));
    let overview_total = overview
        .tasks
        .iter()
        .map(|task| task.exclusive_seconds)
        .sum::<i64>();
    let summary_total = summary
        .days
        .iter()
        .map(|day| day.total_focus_seconds)
        .sum::<i64>();
    // Both APIs read the clock independently. Reconcile using the summary's
    // exact as-of time, so an open session crossing a second cannot make this flaky.
    let projected = replay_exclusive_seconds(
        &conn,
        Some(local_day_start_ts(summary.generated_at)),
        summary.generated_at,
    )
    .unwrap();
    assert_eq!(projected.values().sum::<i64>(), summary_total);
    assert!(overview_total > 0);
    assert!(summary_total > 0);
    println!("snapshot today focus: overview={overview_total}, summary={summary_total} seconds");
}
