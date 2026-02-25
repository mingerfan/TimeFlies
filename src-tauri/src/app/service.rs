use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::json;
use uuid::Uuid;

use crate::domain::{OverviewResponse, TaskRecord};
use crate::infra::AppResult;

const STATUS_IDLE: &str = "idle";
const STATUS_RUNNING: &str = "running";
const STATUS_PAUSED: &str = "paused";
const STATUS_STOPPED: &str = "stopped";

const EVENT_START: &str = "start";
const EVENT_PAUSE: &str = "pause";
const EVENT_RESUME: &str = "resume";
const EVENT_STOP: &str = "stop";
const EVENT_TAG_ADD: &str = "tag_add";
const EVENT_TAG_REMOVE: &str = "tag_remove";

#[derive(Debug)]
struct TaskState {
    parent_id: Option<String>,
    status: String,
}

#[derive(Debug)]
struct TaskRow {
    id: String,
    parent_id: Option<String>,
    title: String,
    status: String,
    created_at: i64,
}

pub fn create_task(conn: &mut Connection, title: String, parent_id: Option<String>) -> AppResult<String> {
    let clean_title = sanitize_title(&title)?;
    if let Some(parent) = &parent_id {
        ensure_task_exists(conn, parent)?;
    }

    let task_id = Uuid::new_v4().to_string();
    let created_at = now_ts();

    conn.execute(
        "INSERT INTO tasks (id, parent_id, title, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![task_id, parent_id, clean_title, STATUS_IDLE, created_at],
    )
    .map_err(to_error)?;

    Ok(task_id)
}

pub fn start_task(conn: &mut Connection, task_id: String) -> AppResult<()> {
    let task = get_task_state(conn, &task_id)?;

    if task.status == STATUS_RUNNING {
        return Ok(());
    }

    if task.status == STATUS_PAUSED {
        return Err("task is paused, use resume_task instead".to_string());
    }

    if let Some(active_task_id) = find_running_task(conn)? {
        if active_task_id != task_id {
            return Err(format!(
                "cannot start task because task {active_task_id} is already running"
            ));
        }
    }

    let ts = now_ts();
    let tx = conn.transaction().map_err(to_error)?;
    tx.execute(
        "UPDATE tasks SET status = ?1 WHERE id = ?2",
        params![STATUS_RUNNING, task_id],
    )
    .map_err(to_error)?;
    append_event(&tx, &task_id, EVENT_START, ts, None)?;
    tx.commit().map_err(to_error)?;

    Ok(())
}

pub fn pause_task(conn: &mut Connection, task_id: String) -> AppResult<()> {
    let task = get_task_state(conn, &task_id)?;

    if task.status == STATUS_PAUSED {
        return Ok(());
    }

    if task.status != STATUS_RUNNING {
        return Err("only a running task can be paused".to_string());
    }

    let ts = now_ts();
    let tx = conn.transaction().map_err(to_error)?;
    tx.execute(
        "UPDATE tasks SET status = ?1 WHERE id = ?2",
        params![STATUS_PAUSED, task_id],
    )
    .map_err(to_error)?;
    append_event(&tx, &task_id, EVENT_PAUSE, ts, None)?;
    tx.commit().map_err(to_error)?;

    Ok(())
}

pub fn resume_task(conn: &mut Connection, task_id: String) -> AppResult<()> {
    let task = get_task_state(conn, &task_id)?;

    if task.status == STATUS_RUNNING {
        return Ok(());
    }

    if task.status != STATUS_PAUSED {
        return Err("only a paused task can be resumed".to_string());
    }

    if let Some(active_task_id) = find_running_task(conn)? {
        if active_task_id != task_id {
            return Err(format!(
                "cannot resume task because task {active_task_id} is already running"
            ));
        }
    }

    let ts = now_ts();
    let tx = conn.transaction().map_err(to_error)?;
    tx.execute(
        "UPDATE tasks SET status = ?1 WHERE id = ?2",
        params![STATUS_RUNNING, task_id],
    )
    .map_err(to_error)?;
    append_event(&tx, &task_id, EVENT_RESUME, ts, None)?;
    tx.commit().map_err(to_error)?;

    Ok(())
}

pub fn stop_task(conn: &mut Connection, task_id: String) -> AppResult<()> {
    let task = get_task_state(conn, &task_id)?;

    if task.status == STATUS_STOPPED {
        return Ok(());
    }

    if task.status == STATUS_IDLE {
        return Err("cannot stop an idle task".to_string());
    }

    let ts = now_ts();
    let tx = conn.transaction().map_err(to_error)?;
    tx.execute(
        "UPDATE tasks SET status = ?1 WHERE id = ?2",
        params![STATUS_STOPPED, task_id],
    )
    .map_err(to_error)?;
    append_event(&tx, &task_id, EVENT_STOP, ts, None)?;

    if let Some(parent_id) = task.parent_id {
        maybe_auto_resume_parent(&tx, &parent_id, &task_id, ts)?;
    }

    tx.commit().map_err(to_error)?;
    Ok(())
}

pub fn insert_subtask_and_start(
    conn: &mut Connection,
    parent_task_id: String,
    title: String,
) -> AppResult<String> {
    let clean_title = sanitize_title(&title)?;
    let parent = get_task_state(conn, &parent_task_id)?;

    if parent.status != STATUS_RUNNING {
        return Err("insert_subtask_and_start requires the parent task to be running".to_string());
    }

    if let Some(active_task_id) = find_running_task(conn)? {
        if active_task_id != parent_task_id {
            return Err(format!(
                "cannot insert subtask while task {active_task_id} is running"
            ));
        }
    } else {
        return Err("no running task found for subtask insertion".to_string());
    }

    let child_task_id = Uuid::new_v4().to_string();
    let ts = now_ts();
    let tx = conn.transaction().map_err(to_error)?;

    tx.execute(
        "UPDATE tasks SET status = ?1 WHERE id = ?2",
        params![STATUS_PAUSED, parent_task_id],
    )
    .map_err(to_error)?;
    append_event(
        &tx,
        &parent_task_id,
        EVENT_PAUSE,
        ts,
        Some(json!({
            "reason": "insert_subtask",
            "child_id": child_task_id
        })),
    )?;

    tx.execute(
        "INSERT INTO tasks (id, parent_id, title, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![child_task_id, parent_task_id, clean_title, STATUS_RUNNING, ts],
    )
    .map_err(to_error)?;
    append_event(
        &tx,
        &child_task_id,
        EVENT_START,
        ts,
        Some(json!({
            "reason": "insert_subtask",
            "parent_id": parent_task_id
        })),
    )?;

    tx.commit().map_err(to_error)?;

    Ok(child_task_id)
}

pub fn add_tag_to_task(conn: &mut Connection, task_id: String, tag_name: String) -> AppResult<()> {
    ensure_task_exists(conn, &task_id)?;
    let clean_tag = sanitize_tag(&tag_name)?;
    let ts = now_ts();

    let tx = conn.transaction().map_err(to_error)?;

    let maybe_tag_id: Option<String> = tx
        .query_row(
            "SELECT id FROM tags WHERE lower(name) = lower(?1) LIMIT 1",
            params![clean_tag],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_error)?;

    let tag_id = if let Some(existing_id) = maybe_tag_id {
        existing_id
    } else {
        let created_tag_id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO tags (id, name) VALUES (?1, ?2)",
            params![created_tag_id, clean_tag],
        )
        .map_err(to_error)?;
        created_tag_id
    };

    let inserted = tx
        .execute(
            "INSERT OR IGNORE INTO task_tags (task_id, tag_id, created_at) VALUES (?1, ?2, ?3)",
            params![task_id, tag_id, ts],
        )
        .map_err(to_error)?;

    if inserted > 0 {
        append_event(
            &tx,
            &task_id,
            EVENT_TAG_ADD,
            ts,
            Some(json!({
                "tag": clean_tag
            })),
        )?;
    }

    tx.commit().map_err(to_error)?;
    Ok(())
}

pub fn remove_tag_from_task(conn: &mut Connection, task_id: String, tag_name: String) -> AppResult<()> {
    ensure_task_exists(conn, &task_id)?;
    let clean_tag = sanitize_tag(&tag_name)?;
    let ts = now_ts();

    let tx = conn.transaction().map_err(to_error)?;
    let maybe_tag_id: Option<String> = tx
        .query_row(
            "SELECT id FROM tags WHERE lower(name) = lower(?1) LIMIT 1",
            params![clean_tag],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_error)?;

    if let Some(tag_id) = maybe_tag_id {
        let deleted = tx
            .execute(
                "DELETE FROM task_tags WHERE task_id = ?1 AND tag_id = ?2",
                params![task_id, tag_id],
            )
            .map_err(to_error)?;

        if deleted > 0 {
            append_event(
                &tx,
                &task_id,
                EVENT_TAG_REMOVE,
                ts,
                Some(json!({
                    "tag": clean_tag
                })),
            )?;
        }
    }

    tx.commit().map_err(to_error)?;
    Ok(())
}

pub fn get_overview(conn: &Connection, range: Option<String>) -> AppResult<OverviewResponse> {
    let now = now_ts();
    let (window_start, resolved_range) = resolve_window(range, now)?;

    let tasks = load_tasks(conn)?;
    let tags_by_task = load_tags(conn)?;
    let exclusive_seconds = replay_exclusive_seconds(conn, window_start, now)?;
    let inclusive_seconds = derive_inclusive_seconds(&tasks, &exclusive_seconds);
    let active_task_id = find_running_task(conn)?;

    let records = tasks
        .into_iter()
        .map(|task| TaskRecord {
            id: task.id.clone(),
            parent_id: task.parent_id.clone(),
            title: task.title,
            status: task.status,
            created_at: task.created_at,
            tags: tags_by_task.get(&task.id).cloned().unwrap_or_default(),
            inclusive_seconds: *inclusive_seconds.get(&task.id).unwrap_or(&0),
            exclusive_seconds: *exclusive_seconds.get(&task.id).unwrap_or(&0),
        })
        .collect::<Vec<_>>();

    Ok(OverviewResponse {
        range: resolved_range,
        generated_at: now,
        active_task_id,
        tasks: records,
    })
}

fn ensure_task_exists(conn: &Connection, task_id: &str) -> AppResult<()> {
    get_task_state(conn, task_id).map(|_| ())
}

fn get_task_state(conn: &Connection, task_id: &str) -> AppResult<TaskState> {
    conn.query_row(
        "SELECT parent_id, status FROM tasks WHERE id = ?1 AND archived_at IS NULL LIMIT 1",
        params![task_id],
        |row| {
            Ok(TaskState {
                parent_id: row.get(0)?,
                status: row.get(1)?,
            })
        },
    )
    .optional()
    .map_err(to_error)?
    .ok_or_else(|| format!("task {task_id} not found or archived"))
}

fn find_running_task(conn: &Connection) -> AppResult<Option<String>> {
    conn.query_row(
        "SELECT id FROM tasks WHERE status = ?1 AND archived_at IS NULL LIMIT 1",
        params![STATUS_RUNNING],
        |row| row.get(0),
    )
    .optional()
    .map_err(to_error)
}

fn append_event(
    tx: &Transaction<'_>,
    task_id: &str,
    event_type: &str,
    ts: i64,
    payload: Option<serde_json::Value>,
) -> AppResult<()> {
    let payload_string = payload.map(|value| value.to_string());
    tx.execute(
        "INSERT INTO time_events (task_id, event_type, ts, payload) VALUES (?1, ?2, ?3, ?4)",
        params![task_id, event_type, ts, payload_string],
    )
    .map_err(to_error)?;
    Ok(())
}

fn maybe_auto_resume_parent(
    tx: &Transaction<'_>,
    parent_task_id: &str,
    child_task_id: &str,
    ts: i64,
) -> AppResult<()> {
    let parent_status: Option<String> = tx
        .query_row(
            "SELECT status FROM tasks WHERE id = ?1 AND archived_at IS NULL LIMIT 1",
            params![parent_task_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_error)?;

    if parent_status.as_deref() != Some(STATUS_PAUSED) {
        return Ok(());
    }

    let latest_parent_event: Option<(String, Option<String>)> = tx
        .query_row(
            "SELECT event_type, payload FROM time_events WHERE task_id = ?1 ORDER BY ts DESC, id DESC LIMIT 1",
            params![parent_task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(to_error)?;

    let Some((latest_event_type, latest_payload)) = latest_parent_event else {
        return Ok(());
    };

    if latest_event_type != EVENT_PAUSE {
        return Ok(());
    }

    let child_id_from_payload = latest_payload
        .and_then(|payload| serde_json::from_str::<serde_json::Value>(&payload).ok())
        .and_then(|value| value.get("child_id").and_then(|raw| raw.as_str()).map(str::to_owned));

    if child_id_from_payload.as_deref() != Some(child_task_id) {
        return Ok(());
    }

    let maybe_running_elsewhere: Option<String> = tx
        .query_row(
            "SELECT id FROM tasks WHERE status = ?1 AND archived_at IS NULL LIMIT 1",
            params![STATUS_RUNNING],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_error)?;

    if maybe_running_elsewhere.is_some() {
        return Ok(());
    }

    tx.execute(
        "UPDATE tasks SET status = ?1 WHERE id = ?2",
        params![STATUS_RUNNING, parent_task_id],
    )
    .map_err(to_error)?;
    append_event(
        tx,
        parent_task_id,
        EVENT_RESUME,
        ts,
        Some(json!({
            "reason": "child_stopped",
            "child_id": child_task_id
        })),
    )?;

    Ok(())
}

fn load_tasks(conn: &Connection) -> AppResult<Vec<TaskRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, parent_id, title, status, created_at FROM tasks WHERE archived_at IS NULL ORDER BY created_at ASC",
        )
        .map_err(to_error)?;

    let rows = stmt
        .query_map([], |row| {
            Ok(TaskRow {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                title: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(to_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(to_error)
}

fn load_tags(conn: &Connection) -> AppResult<HashMap<String, Vec<String>>> {
    let mut tags_by_task: HashMap<String, Vec<String>> = HashMap::new();
    let mut stmt = conn
        .prepare(
            "
            SELECT tt.task_id, tg.name
            FROM task_tags tt
            INNER JOIN tags tg ON tg.id = tt.tag_id
            INNER JOIN tasks t ON t.id = tt.task_id
            WHERE t.archived_at IS NULL
            ORDER BY tg.name ASC
            ",
        )
        .map_err(to_error)?;

    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(to_error)?;

    for row in rows {
        let (task_id, tag_name) = row.map_err(to_error)?;
        tags_by_task.entry(task_id).or_default().push(tag_name);
    }

    Ok(tags_by_task)
}

fn replay_exclusive_seconds(
    conn: &Connection,
    window_start: Option<i64>,
    window_end: i64,
) -> AppResult<HashMap<String, i64>> {
    let mut stmt = conn
        .prepare("SELECT task_id, event_type, ts FROM time_events ORDER BY ts ASC, id ASC")
        .map_err(to_error)?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(to_error)?;

    let mut running_since: HashMap<String, i64> = HashMap::new();
    let mut exclusive: HashMap<String, i64> = HashMap::new();

    for row in rows {
        let (task_id, event_type, ts) = row.map_err(to_error)?;
        match event_type.as_str() {
            EVENT_START | EVENT_RESUME => {
                running_since.entry(task_id).or_insert(ts);
            }
            EVENT_PAUSE | EVENT_STOP => {
                if let Some(start) = running_since.remove(&task_id) {
                    add_interval(
                        &mut exclusive,
                        &task_id,
                        start,
                        ts,
                        window_start,
                        window_end,
                    );
                }
            }
            _ => {}
        }
    }

    for (task_id, start) in running_since {
        add_interval(
            &mut exclusive,
            &task_id,
            start,
            window_end,
            window_start,
            window_end,
        );
    }

    Ok(exclusive)
}

fn add_interval(
    exclusive: &mut HashMap<String, i64>,
    task_id: &str,
    start: i64,
    end: i64,
    window_start: Option<i64>,
    window_end: i64,
) {
    let clipped_start = window_start.map_or(start, |window| start.max(window));
    let clipped_end = end.min(window_end);

    if clipped_end > clipped_start {
        *exclusive.entry(task_id.to_string()).or_insert(0) += clipped_end - clipped_start;
    }
}

fn derive_inclusive_seconds(tasks: &[TaskRow], exclusive: &HashMap<String, i64>) -> HashMap<String, i64> {
    let mut children_by_parent: HashMap<String, Vec<String>> = HashMap::new();
    for task in tasks {
        if let Some(parent_id) = &task.parent_id {
            children_by_parent
                .entry(parent_id.clone())
                .or_default()
                .push(task.id.clone());
        }
    }

    let mut memo: HashMap<String, i64> = HashMap::new();
    for task in tasks {
        let mut visiting = HashSet::new();
        let _ = compute_inclusive(
            &task.id,
            &children_by_parent,
            exclusive,
            &mut memo,
            &mut visiting,
        );
    }

    memo
}

fn compute_inclusive(
    task_id: &str,
    children_by_parent: &HashMap<String, Vec<String>>,
    exclusive: &HashMap<String, i64>,
    memo: &mut HashMap<String, i64>,
    visiting: &mut HashSet<String>,
) -> i64 {
    if let Some(cached) = memo.get(task_id) {
        return *cached;
    }

    if !visiting.insert(task_id.to_string()) {
        return *exclusive.get(task_id).unwrap_or(&0);
    }

    let mut total = *exclusive.get(task_id).unwrap_or(&0);
    if let Some(children) = children_by_parent.get(task_id) {
        for child_id in children {
            total += compute_inclusive(child_id, children_by_parent, exclusive, memo, visiting);
        }
    }

    visiting.remove(task_id);
    memo.insert(task_id.to_string(), total);
    total
}

fn sanitize_title(raw: &str) -> AppResult<String> {
    let cleaned = raw.trim();
    if cleaned.is_empty() {
        return Err("title cannot be empty".to_string());
    }
    Ok(cleaned.to_string())
}

fn sanitize_tag(raw: &str) -> AppResult<String> {
    let cleaned = raw.trim();
    if cleaned.is_empty() {
        return Err("tag cannot be empty".to_string());
    }
    Ok(cleaned.to_string())
}

fn resolve_window(range: Option<String>, now: i64) -> AppResult<(Option<i64>, String)> {
    match range.as_deref().unwrap_or("all") {
        "all" => Ok((None, "all".to_string())),
        "day" => Ok((Some(now - 86_400), "day".to_string())),
        "week" => Ok((Some(now - 604_800), "week".to_string())),
        unsupported => Err(format!(
            "unsupported range '{unsupported}', expected one of: all, day, week"
        )),
    }
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn to_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
