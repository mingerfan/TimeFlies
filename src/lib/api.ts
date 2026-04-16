import { invoke } from "@tauri-apps/api/core";

export type TaskStatus = "idle" | "running" | "paused" | "stopped";
export type OverviewRange = "all" | "day" | "week" | "today";
export type FocusSummaryRange = "today" | "7d" | "30d" | "all";
export const APP_DATA_CHANGED_EVENT = "timeflies:data-changed";

export interface TaskRecord {
  id: string;
  parent_id: string | null;
  title: string;
  status: TaskStatus;
  created_at: number;
  last_activated_at: number | null;
  tags: string[];
  inclusive_seconds: number;
  exclusive_seconds: number;
}

export interface RestSuggestionRecord {
  id: number;
  trigger_type: "subtask_end" | "task_switch";
  task_id: string | null;
  focus_seconds: number;
  switch_count_30m: number;
  deviation_ratio: number;
  suggested_minutes: 0 | 3 | 8 | 15;
  reasons: string[];
  status: "pending" | "accepted" | "ignored";
  created_at: number;
}

export interface NotificationRecord {
  id: number;
  kind: "rest_suggestion";
  level: "info" | "warning" | "error" | "success";
  status: "pending" | "accepted" | "ignored" | "dismissed";
  title: string;
  message: string | null;
  detail: string | null;
  created_at: number;
  rest_suggestion: RestSuggestionRecord | null;
}

export interface OverviewResponse {
  range: OverviewRange;
  generated_at: number;
  active_task_id: string | null;
  last_used_task_id: string | null;
  rest_suggestion: RestSuggestionRecord | null;
  notifications: NotificationRecord[];
  tasks: TaskRecord[];
}

export interface DayTaskBreakdown {
  task_id: string;
  parent_id: string | null;
  title: string;
  exclusive_seconds: number;
  share_ratio: number;
}

export interface FocusTimelineSegment {
  task_id: string;
  parent_id: string | null;
  title: string;
  start_ts: number;
  end_ts: number;
  start_offset_seconds: number;
  end_offset_seconds: number;
  duration_seconds: number;
}

export interface FocusSummaryDay {
  date_key: string;
  day_start_ts: number;
  day_end_ts: number;
  total_focus_seconds: number;
  tasks: DayTaskBreakdown[];
  timeline_segments: FocusTimelineSegment[];
}

export interface FocusSummaryResponse {
  range: FocusSummaryRange;
  generated_at: number;
  days: FocusSummaryDay[];
}

function notifyDataChanged() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(APP_DATA_CHANGED_EVENT));
}

export async function ping(): Promise<string> {
  return invoke<string>("ping");
}

export async function getOverview(range: OverviewRange): Promise<OverviewResponse> {
  return invoke<OverviewResponse>("get_overview", { range });
}

export async function getFocusSummary(range: FocusSummaryRange): Promise<FocusSummaryResponse> {
  return invoke<FocusSummaryResponse>("get_focus_summary", { range });
}

export async function createTask(title: string, parentId?: string | null): Promise<string> {
  const createdTaskId = await invoke<string>("create_task", {
    title,
    parentId: parentId ?? null,
  });
  notifyDataChanged();
  return createdTaskId;
}

export async function renameTask(taskId: string, title: string): Promise<void> {
  await invoke("rename_task", { taskId, title });
  notifyDataChanged();
}

export async function archiveTask(taskId: string): Promise<void> {
  await invoke("archive_task", { taskId });
  notifyDataChanged();
}

export async function deleteTasks(taskIds: string[], hardDelete = false): Promise<void> {
  const normalizedTaskIds = [...new Set(taskIds.map((id) => id.trim()).filter((id) => id.length > 0))];
  if (normalizedTaskIds.length === 0) return;
  await invoke("delete_tasks", {
    taskIds: normalizedTaskIds,
    hardDelete,
  });
  notifyDataChanged();
}

export async function reparentTask(taskId: string, newParentId?: string | null): Promise<void> {
  await invoke("reparent_task", {
    taskId,
    newParentId: newParentId ?? null,
  });
  notifyDataChanged();
}

export async function startTask(taskId: string): Promise<void> {
  await invoke("start_task", { taskId });
  notifyDataChanged();
}

export async function pauseTask(taskId: string): Promise<void> {
  await invoke("pause_task", { taskId });
  notifyDataChanged();
}

export async function resumeTask(taskId: string): Promise<void> {
  await invoke("resume_task", { taskId });
  notifyDataChanged();
}

export async function stopTask(taskId: string): Promise<void> {
  await invoke("stop_task", { taskId });
  notifyDataChanged();
}

export async function adjustTaskFocus(taskId: string, deltaSeconds: number): Promise<void> {
  await invoke("adjust_task_focus", { taskId, deltaSeconds });
  notifyDataChanged();
}

export async function insertSubtaskAndStart(parentTaskId: string, title: string): Promise<string> {
  const childTaskId = await invoke<string>("insert_subtask_and_start", { parentTaskId, title });
  notifyDataChanged();
  return childTaskId;
}

export async function addTagToTask(taskId: string, tagName: string): Promise<void> {
  await invoke("add_tag_to_task", { taskId, tagName });
  notifyDataChanged();
}

export async function removeTagFromTask(taskId: string, tagName: string): Promise<void> {
  await invoke("remove_tag_from_task", { taskId, tagName });
  notifyDataChanged();
}

export async function respondRestSuggestion(suggestionId: number, accept: boolean): Promise<void> {
  await invoke("respond_rest_suggestion", { suggestionId, accept });
  notifyDataChanged();
}

