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
pub struct NotificationRecord {
    pub id: i64,
    pub kind: String,
    pub level: String,
    pub status: String,
    pub title: String,
    pub message: Option<String>,
    pub detail: Option<String>,
    pub created_at: i64,
    pub rest_suggestion: Option<RestSuggestionRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewResponse {
    pub range: String,
    pub generated_at: i64,
    pub active_task_id: Option<String>,
    pub last_used_task_id: Option<String>,
    pub rest_suggestion: Option<RestSuggestionRecord>,
    pub notifications: Vec<NotificationRecord>,
    pub tasks: Vec<TaskRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayTaskBreakdown {
    pub task_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub exclusive_seconds: i64,
    pub share_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FocusSummaryDay {
    pub date_key: String,
    pub day_start_ts: i64,
    pub day_end_ts: i64,
    pub total_focus_seconds: i64,
    pub tasks: Vec<DayTaskBreakdown>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FocusSummaryResponse {
    pub range: String,
    pub generated_at: i64,
    pub days: Vec<FocusSummaryDay>,
}
