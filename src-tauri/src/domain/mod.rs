use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TaskRecord {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub status: String,
    pub created_at: i64,
    pub tags: Vec<String>,
    pub inclusive_seconds: i64,
    pub exclusive_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewResponse {
    pub range: String,
    pub generated_at: i64,
    pub active_task_id: Option<String>,
    pub tasks: Vec<TaskRecord>,
}
