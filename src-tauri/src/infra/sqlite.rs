use std::fs;
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{AppHandle, Manager};

pub type AppResult<T> = Result<T, String>;

pub struct AppState {
    pub db: Mutex<Connection>,
}

impl AppState {
    pub fn initialize(app: &AppHandle) -> AppResult<Self> {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("failed to resolve app data directory: {error}"))?;

        fs::create_dir_all(&app_data_dir).map_err(|error| {
            format!(
                "failed to create app data directory {}: {error}",
                app_data_dir.display()
            )
        })?;

        let db_path = app_data_dir.join("timeflies.db");
        let connection = Connection::open(&db_path).map_err(|error| {
            format!(
                "failed to open sqlite database {}: {error}",
                db_path.display()
            )
        })?;

        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| format!("failed to enable sqlite foreign_keys pragma: {error}"))?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| format!("failed to enable sqlite WAL mode: {error}"))?;

        run_migrations(&connection)?;

        Ok(Self {
            db: Mutex::new(connection),
        })
    }
}

fn run_migrations(connection: &Connection) -> AppResult<()> {
    let current_version: i64 = connection
        .query_row("PRAGMA user_version;", [], |row| row.get(0))
        .map_err(|error| format!("failed to fetch sqlite user_version: {error}"))?;

    if current_version < 1 {
        connection.execute_batch(
            "
            BEGIN;

            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                parent_id TEXT REFERENCES tasks(id),
                title TEXT NOT NULL CHECK(length(trim(title)) > 0),
                status TEXT NOT NULL CHECK(status IN ('idle', 'running', 'paused', 'stopped')),
                created_at INTEGER NOT NULL,
                archived_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_parent_id ON tasks(parent_id);

            CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS task_tags (
                task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (task_id, tag_id)
            );
            CREATE INDEX IF NOT EXISTS idx_task_tags_task_id ON task_tags(task_id);
            CREATE INDEX IF NOT EXISTS idx_task_tags_tag_id ON task_tags(tag_id);

            CREATE TABLE IF NOT EXISTS time_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL REFERENCES tasks(id),
                event_type TEXT NOT NULL CHECK(
                    event_type IN ('start', 'pause', 'resume', 'stop', 'reparent', 'tag_add', 'tag_remove')
                ),
                ts INTEGER NOT NULL,
                payload TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_time_events_task_ts ON time_events(task_id, ts, id);
            CREATE INDEX IF NOT EXISTS idx_time_events_ts ON time_events(ts, id);

            PRAGMA user_version = 1;

            COMMIT;
            ",
        )
        .map_err(|error| format!("failed to apply sqlite migration v1: {error}"))?;
    }

    if current_version < 2 {
        connection
            .execute_batch(
                "
                BEGIN;

                CREATE TABLE IF NOT EXISTS rest_suggestions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    trigger_type TEXT NOT NULL CHECK(trigger_type IN ('subtask_end', 'task_switch')),
                    task_id TEXT REFERENCES tasks(id),
                    focus_seconds INTEGER NOT NULL,
                    switch_count_30m INTEGER NOT NULL,
                    deviation_ratio REAL NOT NULL,
                    suggested_minutes INTEGER NOT NULL CHECK(suggested_minutes IN (0, 3, 8, 15)),
                    reasons TEXT NOT NULL,
                    status TEXT NOT NULL CHECK(status IN ('pending', 'accepted', 'ignored')),
                    created_at INTEGER NOT NULL,
                    responded_at INTEGER
                );
                CREATE INDEX IF NOT EXISTS idx_rest_suggestions_status_created_at
                    ON rest_suggestions(status, created_at DESC, id DESC);

                PRAGMA user_version = 2;

                COMMIT;
                ",
            )
            .map_err(|error| format!("failed to apply sqlite migration v2: {error}"))?;
    }

    Ok(())
}
