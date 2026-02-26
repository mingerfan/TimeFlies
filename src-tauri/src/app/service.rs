use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use chrono::{Local, TimeZone};
use serde_json::json;
use uuid::Uuid;

use crate::domain::{OverviewResponse, RestSuggestionRecord, TaskRecord};
use crate::infra::AppResult;

const STATUS_IDLE: &str = "idle";
const STATUS_RUNNING: &str = "running";
const STATUS_PAUSED: &str = "paused";
const STATUS_STOPPED: &str = "stopped";

const EVENT_START: &str = "start";
const EVENT_PAUSE: &str = "pause";
const EVENT_RESUME: &str = "resume";
const EVENT_STOP: &str = "stop";
const EVENT_REPARENT: &str = "reparent";
const EVENT_TAG_ADD: &str = "tag_add";
const EVENT_TAG_REMOVE: &str = "tag_remove";

const REST_TRIGGER_SUBTASK_END: &str = "subtask_end";
const REST_TRIGGER_TASK_SWITCH: &str = "task_switch";
const REST_STATUS_PENDING: &str = "pending";
const REST_STATUS_ACCEPTED: &str = "accepted";
const REST_STATUS_IGNORED: &str = "ignored";
const SWITCH_WINDOW_SECONDS: i64 = 30 * 60;

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

pub fn create_task(
    conn: &mut Connection,
    title: String,
    parent_id: Option<String>,
) -> AppResult<String> {
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

pub fn rename_task(conn: &mut Connection, task_id: String, title: String) -> AppResult<()> {
    ensure_task_exists(conn, &task_id)?;
    let clean_title = sanitize_title(&title)?;
    conn.execute(
        "UPDATE tasks SET title = ?1 WHERE id = ?2 AND archived_at IS NULL",
        params![clean_title, task_id],
    )
    .map_err(to_error)?;
    Ok(())
}

pub fn archive_task(conn: &mut Connection, task_id: String) -> AppResult<()> {
    delete_tasks(conn, vec![task_id], false)
}

pub fn delete_tasks(
    conn: &mut Connection,
    task_ids: Vec<String>,
    hard_delete: bool,
) -> AppResult<()> {
    if task_ids.is_empty() {
        return Err("task_ids cannot be empty".to_string());
    }

    let expanded_ids = expand_unique_subtree_ids(conn, &task_ids)?;
    if expanded_ids.is_empty() {
        return Ok(());
    }

    if let Some((active_id, active_status)) = find_active_in_subtree(conn, &expanded_ids)? {
        let action = if hard_delete {
            "hard delete"
        } else {
            "archive"
        };
        return Err(format!(
            "cannot {action} task {active_id} because it is currently {active_status}"
        ));
    }

    let tx = conn.transaction().map_err(to_error)?;
    if hard_delete {
        hard_delete_task_ids(&tx, &expanded_ids)?;
    } else {
        archive_task_ids(&tx, &expanded_ids, now_ts())?;
    }
    tx.commit().map_err(to_error)?;

    Ok(())
}

pub fn reparent_task(
    conn: &mut Connection,
    task_id: String,
    new_parent_id: Option<String>,
) -> AppResult<()> {
    let task = get_task_state(conn, &task_id)?;
    let old_parent_id = task.parent_id.clone();

    if new_parent_id.as_deref() == Some(task_id.as_str()) {
        return Err("task cannot be its own parent".to_string());
    }

    if old_parent_id == new_parent_id {
        return Ok(());
    }

    let subtree_ids = collect_subtree_ids(conn, &task_id)?;
    if let Some((active_id, active_status)) = find_active_in_subtree(conn, &subtree_ids)? {
        return Err(format!(
            "cannot reparent while task {active_id} is {active_status}; stop or pause transitions first"
        ));
    }

    let subtree_set: HashSet<String> = subtree_ids.into_iter().collect();

    if let Some(parent_id) = &new_parent_id {
        ensure_task_exists(conn, parent_id)?;
        if subtree_set.contains(parent_id) {
            return Err("cannot reparent task under itself or its descendants".to_string());
        }
        ensure_ancestor_chain_valid(conn, parent_id, &task_id)?;
    }

    let ts = now_ts();
    let tx = conn.transaction().map_err(to_error)?;
    tx.execute(
        "UPDATE tasks SET parent_id = ?1 WHERE id = ?2 AND archived_at IS NULL",
        params![new_parent_id, task_id],
    )
    .map_err(to_error)?;
    append_event(
        &tx,
        &task_id,
        EVENT_REPARENT,
        ts,
        Some(json!({
            "old_parent_id": old_parent_id,
            "new_parent_id": new_parent_id
        })),
    )?;
    tx.commit().map_err(to_error)?;

    Ok(())
}

pub fn start_task(conn: &mut Connection, task_id: String) -> AppResult<()> {
    let previous_focus_task = latest_focus_task(conn)?;
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

    maybe_create_task_switch_suggestion(conn, previous_focus_task, &task_id, ts)?;

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
    let previous_focus_task = latest_focus_task(conn)?;
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

    maybe_create_task_switch_suggestion(conn, previous_focus_task, &task_id, ts)?;

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
    let mut should_trigger_subtask_rest = false;
    let tx = conn.transaction().map_err(to_error)?;
    tx.execute(
        "UPDATE tasks SET status = ?1 WHERE id = ?2",
        params![STATUS_STOPPED, task_id],
    )
    .map_err(to_error)?;
    append_event(&tx, &task_id, EVENT_STOP, ts, None)?;

    if let Some(parent_id) = task.parent_id {
        should_trigger_subtask_rest = maybe_auto_resume_parent(&tx, &parent_id, &task_id, ts)?;
    }

    tx.commit().map_err(to_error)?;

    if should_trigger_subtask_rest {
        create_rest_suggestion(conn, REST_TRIGGER_SUBTASK_END, Some(task_id.as_str()), ts)?;
    }

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
        params![
            child_task_id,
            parent_task_id,
            clean_title,
            STATUS_RUNNING,
            ts
        ],
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

    create_rest_suggestion(
        conn,
        REST_TRIGGER_TASK_SWITCH,
        Some(parent_task_id.as_str()),
        ts,
    )?;

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

pub fn remove_tag_from_task(
    conn: &mut Connection,
    task_id: String,
    tag_name: String,
) -> AppResult<()> {
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

pub fn respond_rest_suggestion(
    conn: &mut Connection,
    suggestion_id: i64,
    accept: bool,
) -> AppResult<()> {
    if suggestion_id <= 0 {
        return Err("suggestion_id must be positive".to_string());
    }

    let status = if accept {
        REST_STATUS_ACCEPTED
    } else {
        REST_STATUS_IGNORED
    };
    let ts = now_ts();

    let updated = conn
        .execute(
            "UPDATE rest_suggestions
             SET status = ?1, responded_at = ?2
             WHERE id = ?3 AND status = ?4",
            params![status, ts, suggestion_id, REST_STATUS_PENDING],
        )
        .map_err(to_error)?;

    if updated > 0 {
        return Ok(());
    }

    let existing: Option<String> = conn
        .query_row(
            "SELECT status FROM rest_suggestions WHERE id = ?1 LIMIT 1",
            params![suggestion_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_error)?;

    match existing {
        Some(_) => Ok(()),
        None => Err(format!("rest suggestion {suggestion_id} not found")),
    }
}

pub fn get_overview(conn: &Connection, range: Option<String>) -> AppResult<OverviewResponse> {
    let now = now_ts();
    let (window_start, resolved_range) = resolve_window(range, now)?;

    let tasks = load_tasks(conn)?;
    let tags_by_task = load_tags(conn)?;
    let exclusive_seconds = replay_exclusive_seconds(conn, window_start, now)?;
    let inclusive_seconds = derive_inclusive_seconds(&tasks, &exclusive_seconds);
    let active_task_id = find_running_task(conn)?;
    let rest_suggestion = load_latest_pending_rest_suggestion(conn)?;

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
        rest_suggestion,
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

fn latest_focus_task(conn: &Connection) -> AppResult<Option<String>> {
    conn.query_row(
        "SELECT task_id
         FROM time_events
         WHERE event_type IN ('start', 'resume')
         ORDER BY ts DESC, id DESC
         LIMIT 1",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(to_error)
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
) -> AppResult<bool> {
    let parent_status: Option<String> = tx
        .query_row(
            "SELECT status FROM tasks WHERE id = ?1 AND archived_at IS NULL LIMIT 1",
            params![parent_task_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_error)?;

    if parent_status.as_deref() != Some(STATUS_PAUSED) {
        return Ok(false);
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
        return Ok(false);
    };

    if latest_event_type != EVENT_PAUSE {
        return Ok(false);
    }

    let child_id_from_payload = latest_payload
        .and_then(|payload| serde_json::from_str::<serde_json::Value>(&payload).ok())
        .and_then(|value| {
            value
                .get("child_id")
                .and_then(|raw| raw.as_str())
                .map(str::to_owned)
        });

    if child_id_from_payload.as_deref() != Some(child_task_id) {
        return Ok(false);
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
        return Ok(false);
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

    Ok(true)
}

fn load_tasks(conn: &Connection) -> AppResult<Vec<TaskRow>> {
    let mut stmt = conn
        .prepare("SELECT id, parent_id, title, status, created_at FROM tasks WHERE archived_at IS NULL ORDER BY created_at ASC")
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
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(to_error)?;

    for row in rows {
        let (task_id, tag_name) = row.map_err(to_error)?;
        tags_by_task.entry(task_id).or_default().push(tag_name);
    }

    Ok(tags_by_task)
}

fn load_latest_pending_rest_suggestion(
    conn: &Connection,
) -> AppResult<Option<RestSuggestionRecord>> {
    let row: Option<(
        i64,
        String,
        Option<String>,
        i64,
        i64,
        f64,
        i64,
        String,
        String,
        i64,
    )> = conn
        .query_row(
            "SELECT id, trigger_type, task_id, focus_seconds, switch_count_30m, deviation_ratio,
                    suggested_minutes, reasons, status, created_at
             FROM rest_suggestions
             WHERE status = ?1
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            params![REST_STATUS_PENDING],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .optional()
        .map_err(to_error)?;

    match row {
        Some((
            id,
            trigger_type,
            task_id,
            focus_seconds,
            switch_count_30m,
            deviation_ratio,
            suggested_minutes,
            reasons,
            status,
            created_at,
        )) => {
            let reasons = serde_json::from_str::<Vec<String>>(&reasons)
                .unwrap_or_else(|_| vec!["unable to parse rule reasons".to_string()]);
            Ok(Some(RestSuggestionRecord {
                id,
                trigger_type,
                task_id,
                focus_seconds,
                switch_count_30m,
                deviation_ratio,
                suggested_minutes,
                reasons,
                status,
                created_at,
            }))
        }
        None => Ok(None),
    }
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

fn derive_inclusive_seconds(
    tasks: &[TaskRow],
    exclusive: &HashMap<String, i64>,
) -> HashMap<String, i64> {
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

fn collect_subtree_ids(conn: &Connection, root_task_id: &str) -> AppResult<Vec<String>> {
    let mut result = Vec::new();
    let mut stack = vec![root_task_id.to_string()];
    let mut visited = HashSet::new();

    while let Some(task_id) = stack.pop() {
        if !visited.insert(task_id.clone()) {
            return Err(format!(
                "detected cycle while traversing task subtree at {task_id}"
            ));
        }
        result.push(task_id.clone());

        let mut stmt = conn
            .prepare(
                "SELECT id
                 FROM tasks
                 WHERE parent_id = ?1 AND archived_at IS NULL
                 ORDER BY created_at ASC",
            )
            .map_err(to_error)?;

        let rows = stmt
            .query_map(params![task_id], |row| row.get::<_, String>(0))
            .map_err(to_error)?;

        for row in rows {
            stack.push(row.map_err(to_error)?);
        }
    }

    Ok(result)
}

fn expand_unique_subtree_ids(conn: &Connection, root_task_ids: &[String]) -> AppResult<Vec<String>> {
    let mut expanded = Vec::new();
    let mut seen = HashSet::new();

    for root_task_id in root_task_ids {
        let task_id = root_task_id.trim();
        if task_id.is_empty() {
            continue;
        }
        if seen.contains(task_id) {
            continue;
        }

        ensure_task_exists(conn, task_id)?;
        let subtree_ids = collect_subtree_ids(conn, task_id)?;
        for subtree_id in subtree_ids {
            if seen.insert(subtree_id.clone()) {
                expanded.push(subtree_id);
            }
        }
    }

    Ok(expanded)
}

fn archive_task_ids(tx: &Transaction<'_>, task_ids: &[String], archived_at: i64) -> AppResult<()> {
    for task_id in task_ids {
        tx.execute(
            "UPDATE tasks SET archived_at = ?1 WHERE id = ?2 AND archived_at IS NULL",
            params![archived_at, task_id],
        )
        .map_err(to_error)?;
    }
    Ok(())
}

fn hard_delete_task_ids(tx: &Transaction<'_>, task_ids: &[String]) -> AppResult<()> {
    for task_id in task_ids {
        tx.execute(
            "DELETE FROM rest_suggestions WHERE task_id = ?1",
            params![task_id],
        )
        .map_err(to_error)?;
        tx.execute("DELETE FROM time_events WHERE task_id = ?1", params![task_id])
            .map_err(to_error)?;
    }

    for task_id in task_ids.iter().rev() {
        tx.execute("DELETE FROM tasks WHERE id = ?1", params![task_id])
            .map_err(to_error)?;
    }

    tx.execute(
        "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM task_tags)",
        [],
    )
    .map_err(to_error)?;

    Ok(())
}

fn find_active_in_subtree(
    conn: &Connection,
    task_ids: &[String],
) -> AppResult<Option<(String, String)>> {
    for task_id in task_ids {
        let status: Option<String> = conn
            .query_row(
                "SELECT status FROM tasks WHERE id = ?1 AND archived_at IS NULL LIMIT 1",
                params![task_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(to_error)?;

        if let Some(status) = status {
            if status == STATUS_RUNNING || status == STATUS_PAUSED {
                return Ok(Some((task_id.clone(), status)));
            }
        }
    }

    Ok(None)
}

fn ensure_ancestor_chain_valid(
    conn: &Connection,
    new_parent_id: &str,
    blocked_task_id: &str,
) -> AppResult<()> {
    let mut current_id = Some(new_parent_id.to_string());
    let mut visited = HashSet::new();

    while let Some(task_id) = current_id {
        if !visited.insert(task_id.clone()) {
            return Err(format!("detected existing cycle involving task {task_id}"));
        }

        if task_id == blocked_task_id {
            return Err("cannot reparent task under itself or its descendants".to_string());
        }

        let parent: Option<Option<String>> = conn
            .query_row(
                "SELECT parent_id FROM tasks WHERE id = ?1 AND archived_at IS NULL LIMIT 1",
                params![task_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(to_error)?;

        let Some(next_parent) = parent else {
            return Err(format!("task {task_id} not found or archived"));
        };

        current_id = next_parent;
    }

    Ok(())
}

fn maybe_create_task_switch_suggestion(
    conn: &mut Connection,
    previous_focus_task: Option<String>,
    current_task_id: &str,
    ts: i64,
) -> AppResult<()> {
    let Some(previous_task_id) = previous_focus_task else {
        return Ok(());
    };
    if previous_task_id == current_task_id {
        return Ok(());
    }
    create_rest_suggestion(
        conn,
        REST_TRIGGER_TASK_SWITCH,
        Some(previous_task_id.as_str()),
        ts,
    )
}

fn create_rest_suggestion(
    conn: &mut Connection,
    trigger_type: &str,
    source_task_id: Option<&str>,
    trigger_ts: i64,
) -> AppResult<()> {
    let focus_seconds = if let Some(task_id) = source_task_id {
        latest_closed_session_duration(conn, task_id, trigger_ts)?.unwrap_or(0)
    } else {
        0
    };
    let switch_count_30m =
        count_task_switches(conn, trigger_ts - SWITCH_WINDOW_SECONDS, trigger_ts)?;
    let deviation_ratio = if let Some(task_id) = source_task_id {
        compute_deviation_ratio(conn, task_id, focus_seconds, trigger_ts)?
    } else {
        0.0
    };
    let (suggested_minutes, reasons) =
        evaluate_rest_rules(focus_seconds, switch_count_30m, deviation_ratio);

    let tx = conn.transaction().map_err(to_error)?;
    insert_rest_suggestion(
        &tx,
        trigger_type,
        source_task_id,
        focus_seconds,
        switch_count_30m,
        deviation_ratio,
        suggested_minutes,
        &reasons,
        trigger_ts,
    )?;
    tx.commit().map_err(to_error)?;

    Ok(())
}

fn insert_rest_suggestion(
    tx: &Transaction<'_>,
    trigger_type: &str,
    task_id: Option<&str>,
    focus_seconds: i64,
    switch_count_30m: i64,
    deviation_ratio: f64,
    suggested_minutes: i64,
    reasons: &[String],
    ts: i64,
) -> AppResult<()> {
    let reasons_json = serde_json::to_string(reasons).map_err(to_error)?;

    tx.execute(
        "UPDATE rest_suggestions
         SET status = ?1, responded_at = ?2
         WHERE status = ?3",
        params![REST_STATUS_IGNORED, ts, REST_STATUS_PENDING],
    )
    .map_err(to_error)?;

    tx.execute(
        "INSERT INTO rest_suggestions
            (trigger_type, task_id, focus_seconds, switch_count_30m, deviation_ratio,
             suggested_minutes, reasons, status, created_at, responded_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
        params![
            trigger_type,
            task_id,
            focus_seconds,
            switch_count_30m,
            deviation_ratio,
            suggested_minutes,
            reasons_json,
            REST_STATUS_PENDING,
            ts
        ],
    )
    .map_err(to_error)?;

    Ok(())
}

fn latest_closed_session_duration(
    conn: &Connection,
    task_id: &str,
    until_ts: i64,
) -> AppResult<Option<i64>> {
    let sessions = completed_session_durations(conn, task_id, until_ts)?;
    Ok(sessions.last().copied())
}

fn completed_session_durations(
    conn: &Connection,
    task_id: &str,
    until_ts: i64,
) -> AppResult<Vec<i64>> {
    let mut stmt = conn
        .prepare(
            "SELECT event_type, ts
             FROM time_events
             WHERE task_id = ?1
               AND event_type IN ('start', 'resume', 'pause', 'stop')
               AND ts <= ?2
             ORDER BY ts ASC, id ASC",
        )
        .map_err(to_error)?;

    let rows = stmt
        .query_map(params![task_id, until_ts], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(to_error)?;

    let mut running_since: Option<i64> = None;
    let mut sessions = Vec::new();

    for row in rows {
        let (event_type, ts) = row.map_err(to_error)?;
        match event_type.as_str() {
            EVENT_START | EVENT_RESUME => {
                if running_since.is_none() {
                    running_since = Some(ts);
                }
            }
            EVENT_PAUSE | EVENT_STOP => {
                if let Some(start) = running_since.take() {
                    if ts > start {
                        sessions.push(ts - start);
                    }
                }
            }
            _ => {}
        }
    }

    Ok(sessions)
}

fn count_task_switches(conn: &Connection, window_start: i64, window_end: i64) -> AppResult<i64> {
    let mut stmt = conn
        .prepare(
            "SELECT task_id
             FROM time_events
             WHERE event_type IN ('start', 'resume')
               AND ts >= ?1 AND ts <= ?2
             ORDER BY ts ASC, id ASC",
        )
        .map_err(to_error)?;

    let rows = stmt
        .query_map(params![window_start, window_end], |row| {
            row.get::<_, String>(0)
        })
        .map_err(to_error)?;

    let mut previous_task_id: Option<String> = None;
    let mut switches = 0i64;

    for row in rows {
        let task_id = row.map_err(to_error)?;
        if let Some(previous) = &previous_task_id {
            if previous != &task_id {
                switches += 1;
            }
        }
        previous_task_id = Some(task_id);
    }

    Ok(switches)
}

fn compute_deviation_ratio(
    conn: &Connection,
    task_id: &str,
    focus_seconds: i64,
    until_ts: i64,
) -> AppResult<f64> {
    if focus_seconds <= 0 {
        return Ok(0.0);
    }

    let mut sessions = completed_session_durations(conn, task_id, until_ts)?;
    if sessions.len() < 2 {
        return Ok(0.0);
    }

    let latest = sessions.pop().unwrap_or(focus_seconds);
    let current = if focus_seconds > 0 {
        focus_seconds
    } else {
        latest
    };
    let baseline = median_i64(&sessions);
    if baseline <= 0 {
        return Ok(0.0);
    }

    Ok(((current - baseline) as f64 / baseline as f64).max(0.0))
}

fn median_i64(values: &[i64]) -> i64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2
    } else {
        sorted[mid]
    }
}

fn evaluate_rest_rules(
    focus_seconds: i64,
    switch_count_30m: i64,
    deviation_ratio: f64,
) -> (i64, Vec<String>) {
    let mut score = 0;
    let mut reasons = Vec::new();

    if focus_seconds >= 5_400 {
        score += 4;
        reasons.push("continuous focus reached 90+ minutes".to_string());
    } else if focus_seconds >= 3_000 {
        score += 2;
        reasons.push("continuous focus reached 50+ minutes".to_string());
    }

    if switch_count_30m >= 6 {
        score += 4;
        reasons.push("task switching was very frequent in the last 30 minutes".to_string());
    } else if switch_count_30m >= 3 {
        score += 2;
        reasons.push("task switching increased in the last 30 minutes".to_string());
    }

    if deviation_ratio >= 1.0 {
        score += 2;
        reasons.push("focus duration is significantly above historical median".to_string());
    } else if deviation_ratio >= 0.5 {
        score += 1;
        reasons.push("focus duration is above historical median".to_string());
    }

    let minutes = if score >= 7 {
        15
    } else if score >= 4 {
        8
    } else if score >= 2 {
        3
    } else {
        0
    };

    if reasons.is_empty() {
        reasons.push("current rhythm is stable; continuing is reasonable".to_string());
    }

    (minutes, reasons)
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
        "today" => Ok((Some(local_day_start_ts(now)), "today".to_string())),
        unsupported => Err(format!(
            "unsupported range '{unsupported}', expected one of: all, day, week, today"
        )),
    }
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn local_day_start_ts(now: i64) -> i64 {
    let Some(local_now) = Local.timestamp_opt(now, 0).single() else {
        return now;
    };
    let Some(naive_midnight) = local_now.date_naive().and_hms_opt(0, 0, 0) else {
        return local_now.timestamp();
    };
    Local
        .from_local_datetime(&naive_midnight)
        .single()
        .or_else(|| Local.from_local_datetime(&naive_midnight).earliest())
        .or_else(|| Local.from_local_datetime(&naive_midnight).latest())
        .unwrap_or(local_now)
        .timestamp()
}

fn to_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
