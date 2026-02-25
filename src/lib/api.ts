import { invoke } from "@tauri-apps/api/core";

export type TaskStatus = "idle" | "running" | "paused" | "stopped";
export type OverviewRange = "all" | "day" | "week";

export interface TaskRecord {
  id: string;
  parent_id: string | null;
  title: string;
  status: TaskStatus;
  created_at: number;
  tags: string[];
  inclusive_seconds: number;
  exclusive_seconds: number;
}

export interface OverviewResponse {
  range: OverviewRange;
  generated_at: number;
  active_task_id: string | null;
  tasks: TaskRecord[];
}

export async function ping(): Promise<string> {
  return invoke<string>("ping");
}

export async function getOverview(range: OverviewRange): Promise<OverviewResponse> {
  return invoke<OverviewResponse>("get_overview", { range });
}

export async function createTask(title: string, parentId?: string | null): Promise<string> {
  return invoke<string>("create_task", {
    title,
    parentId: parentId ?? null,
  });
}

export async function startTask(taskId: string): Promise<void> {
  await invoke("start_task", { taskId });
}

export async function pauseTask(taskId: string): Promise<void> {
  await invoke("pause_task", { taskId });
}

export async function resumeTask(taskId: string): Promise<void> {
  await invoke("resume_task", { taskId });
}

export async function stopTask(taskId: string): Promise<void> {
  await invoke("stop_task", { taskId });
}

export async function insertSubtaskAndStart(parentTaskId: string, title: string): Promise<string> {
  return invoke<string>("insert_subtask_and_start", { parentTaskId, title });
}

export async function addTagToTask(taskId: string, tagName: string): Promise<void> {
  await invoke("add_tag_to_task", { taskId, tagName });
}

export async function removeTagFromTask(taskId: string, tagName: string): Promise<void> {
  await invoke("remove_tag_from_task", { taskId, tagName });
}
