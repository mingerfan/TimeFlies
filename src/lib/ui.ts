import { browser } from "$app/environment";
import type { RestSuggestionRecord, TaskRecord, TaskStatus } from "$lib/api";
import { writable } from "svelte/store";

export type AppErrorPayload = {
  code?: string;
  message: string;
  detail?: string;
};

export type RestSessionSource = "manual" | "suggestion";

export type RestSession = {
  active: boolean;
  startedAt: number;
  source: RestSessionSource;
  suggestionId: number | null;
};

const REST_SESSION_STORAGE_KEY = "timeflies:rest-session";

const EMPTY_REST_SESSION: RestSession = {
  active: false,
  startedAt: 0,
  source: "manual",
  suggestionId: null,
};

function loadRestSession(): RestSession {
  if (!browser) return EMPTY_REST_SESSION;
  const raw = window.localStorage.getItem(REST_SESSION_STORAGE_KEY);
  if (!raw) return EMPTY_REST_SESSION;

  try {
    const parsed = JSON.parse(raw) as Partial<RestSession>;
    if (
      parsed.active !== true ||
      typeof parsed.startedAt !== "number" ||
      (parsed.source !== "manual" && parsed.source !== "suggestion")
    ) {
      return EMPTY_REST_SESSION;
    }

    return {
      active: true,
      startedAt: parsed.startedAt,
      source: parsed.source,
      suggestionId: typeof parsed.suggestionId === "number" ? parsed.suggestionId : null,
    };
  } catch {
    return EMPTY_REST_SESSION;
  }
}

function persistRestSession(session: RestSession) {
  if (!browser) return;
  if (!session.active) {
    window.localStorage.removeItem(REST_SESSION_STORAGE_KEY);
    return;
  }
  window.localStorage.setItem(REST_SESSION_STORAGE_KEY, JSON.stringify(session));
}

function createRestSessionStore() {
  const { subscribe, set } = writable<RestSession>(loadRestSession());

  const write = (session: RestSession) => {
    persistRestSession(session);
    set(session);
  };

  return {
    subscribe,
    start(source: RestSessionSource, suggestionId: number | null = null) {
      write({
        active: true,
        startedAt: Math.floor(Date.now() / 1000),
        source,
        suggestionId,
      });
    },
    stop() {
      write(EMPTY_REST_SESSION);
    },
  };
}

export const restSession = createRestSessionStore();

export function startManualRest() {
  restSession.start("manual");
}

export function startSuggestedRest(suggestionId: number | null = null) {
  restSession.start("suggestion", suggestionId);
}

export function stopRest() {
  restSession.stop();
}

export function getRestElapsedSeconds(session: RestSession, nowTs: number): number {
  if (!session.active) return 0;
  return Math.max(0, nowTs - session.startedAt);
}

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

export function formatDateOnly(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleDateString();
}

export function formatDeviation(ratio: number): string {
  return `${Math.round(Math.max(0, ratio) * 100)}%`;
}

export function formatShareRatio(ratio: number): string {
  const percentage = Math.max(0, ratio) * 100;
  if (percentage >= 10 || Number.isInteger(percentage)) {
    return `${Math.round(percentage)}%`;
  }
  return `${percentage.toFixed(1)}%`;
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

export function extractErrorPayload(error: unknown): AppErrorPayload {
  if (typeof error === "string") {
    const maybeJson = parseJsonObject(error);
    if (maybeJson) {
      return extractErrorPayload(maybeJson);
    }
    return { message: error };
  }
  if (error && typeof error === "object") {
    const maybe = error as Record<string, unknown>;
    if (typeof maybe.message === "string") {
      return {
        code: typeof maybe.code === "string" ? maybe.code : undefined,
        message: maybe.message,
        detail: typeof maybe.detail === "string" ? maybe.detail : undefined,
      };
    }
    if (typeof maybe.error === "string") {
      return { message: maybe.error };
    }
  }
  return { message: "发生未知错误" };
}

export function normalizeError(error: unknown): string {
  return extractErrorPayload(error).message;
}

function parseJsonObject(raw: string): Record<string, unknown> | null {
  const text = raw.trim();
  if (!text.startsWith("{") || !text.endsWith("}")) return null;
  try {
    const parsed = JSON.parse(text);
    if (parsed && typeof parsed === "object") {
      return parsed as Record<string, unknown>;
    }
    return null;
  } catch {
    return null;
  }
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

export function taskOwnRecentActivityTs(task: TaskRecord): number {
  return task.last_activated_at ?? task.created_at;
}

export function buildSubtreeRecentActivityMap(tasks: TaskRecord[]): Map<string, number> {
  const childrenByParent = new Map<string, TaskRecord[]>();
  const taskMap = new Map<string, TaskRecord>();

  for (const task of tasks) {
    taskMap.set(task.id, task);
    if (!task.parent_id) continue;
    const siblings = childrenByParent.get(task.parent_id) ?? [];
    siblings.push(task);
    childrenByParent.set(task.parent_id, siblings);
  }

  const memo = new Map<string, number>();
  const visiting = new Set<string>();

  const visit = (taskId: string): number => {
    const cached = memo.get(taskId);
    if (cached !== undefined) return cached;

    const task = taskMap.get(taskId);
    if (!task) return 0;
    if (visiting.has(taskId)) return taskOwnRecentActivityTs(task);

    visiting.add(taskId);
    let latest = taskOwnRecentActivityTs(task);
    for (const child of childrenByParent.get(taskId) ?? []) {
      latest = Math.max(latest, visit(child.id));
    }
    visiting.delete(taskId);
    memo.set(taskId, latest);
    return latest;
  };

  for (const task of tasks) {
    visit(task.id);
  }

  return memo;
}

export function compareTasksByRecentActivity(
  left: TaskRecord,
  right: TaskRecord,
  subtreeRecentActivityMap?: Map<string, number>
): number {
  const leftTs = subtreeRecentActivityMap?.get(left.id) ?? taskOwnRecentActivityTs(left);
  const rightTs = subtreeRecentActivityMap?.get(right.id) ?? taskOwnRecentActivityTs(right);
  if (leftTs !== rightTs) return rightTs - leftTs;

  const leftOwnTs = taskOwnRecentActivityTs(left);
  const rightOwnTs = taskOwnRecentActivityTs(right);
  if (leftOwnTs !== rightOwnTs) return rightOwnTs - leftOwnTs;

  if (left.created_at !== right.created_at) return right.created_at - left.created_at;
  return left.title.localeCompare(right.title) || left.id.localeCompare(right.id);
}

export function compactTaskPath(
  titles: string[],
  options: { maxSegments?: number; tailSegments?: number; maxSegmentChars?: number } = {}
): string {
  if (titles.length === 0) return "";

  const maxSegments = Math.max(3, options.maxSegments ?? 4);
  const tailSegments = Math.max(1, options.tailSegments ?? 2);
  const maxSegmentChars = Math.max(6, options.maxSegmentChars ?? 18);

  const normalized = titles.map((title) => truncateLabel(title, maxSegmentChars));
  if (normalized.length <= maxSegments) {
    return normalized.join(" / ");
  }

  const safeTail = Math.min(tailSegments, normalized.length - 2);
  return [normalized[0], "...", ...normalized.slice(-safeTail)].join(" / ");
}

function truncateLabel(raw: string, maxChars: number): string {
  const text = raw.trim();
  if (text.length <= maxChars) return text;
  return `${text.slice(0, maxChars - 3)}...`;
}
