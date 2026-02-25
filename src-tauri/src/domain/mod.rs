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
pub struct RestSuggestionRecord {
    pub id: i64,
    pub trigger_type: String,
    pub task_id: Option<String>,
    pub focus_seconds: i64,
    pub switch_count_30m: i64,
    pub deviation_ratio: f64,
    pub suggested_minutes: i64,
    pub reasons: Vec<String>,
    pub status: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewResponse {
    pub range: String,
    pub generated_at: i64,
    pub active_task_id: Option<String>,
    pub rest_suggestion: Option<RestSuggestionRecord>,
    pub tasks: Vec<TaskRecord>,
}
