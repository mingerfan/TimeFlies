import type { RestSuggestionRecord, TaskRecord, TaskStatus } from "$lib/api";

export function statusLabel(status: TaskStatus): string {
  switch (status) {
    case "idle":
      return "待开始";
    case "running":
      return "进行中";
    case "paused":
      return "已暂停";
    case "stopped":
      return "已停止";
    default:
      return status;
  }
}

export function formatSeconds(input: number): string {
  const seconds = Math.max(0, Math.floor(input));
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h}h ${m}m ${s}s`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

export function formatClock(input: number): string {
  const seconds = Math.max(0, Math.floor(input));
  const h = Math.floor(seconds / 3600)
    .toString()
    .padStart(2, "0");
  const m = Math.floor((seconds % 3600) / 60)
    .toString()
    .padStart(2, "0");
  const s = (seconds % 60).toString().padStart(2, "0");
  return `${h}:${m}:${s}`;
}

export function formatDate(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString();
}

export function formatDeviation(ratio: number): string {
  return `${Math.round(Math.max(0, ratio) * 100)}%`;
}

export function restTriggerLabel(triggerType: RestSuggestionRecord["trigger_type"]): string {
  switch (triggerType) {
    case "subtask_end":
      return "子任务结束";
    case "task_switch":
      return "任务切换";
    default:
      return triggerType;
  }
}

export function restHeadline(suggestion: RestSuggestionRecord): string {
  if (suggestion.suggested_minutes === 0) {
    return "建议继续专注（0 分钟休息）";
  }
  return `建议休息 ${suggestion.suggested_minutes} 分钟`;
}

export function normalizeError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: string }).message);
  }
  return "发生未知错误";
}

export function buildTaskChain(
  startTaskId: string | null | undefined,
  taskMap: Map<string, TaskRecord>
): TaskRecord[] {
  if (!startTaskId) return [];
  const chain: TaskRecord[] = [];
  const visited = new Set<string>();
  let cursor: string | null | undefined = startTaskId;
  while (cursor) {
    if (visited.has(cursor)) break;
    visited.add(cursor);
    const task = taskMap.get(cursor);
    if (!task) break;
    chain.push(task);
    cursor = task.parent_id;
  }
  return chain.reverse();
}
