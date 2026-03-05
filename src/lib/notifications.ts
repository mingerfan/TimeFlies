import { browser } from "$app/environment";
import { writable } from "svelte/store";

import { extractErrorPayload } from "$lib/ui";

export type NotificationLevel = "error" | "warning" | "info" | "success";
export type NotificationKind = "generic" | "command" | "system" | "rest-suggestion";
export type NotificationActionVariant = "primary" | "secondary";
export type NotificationTone = "success" | "error" | "warning";

export type NotificationAction = {
  label: string;
  variant?: NotificationActionVariant;
  closeOnClick?: boolean;
  run: () => void | Promise<void>;
};

export type AppNotification = {
  id: string;
  kind: NotificationKind;
  level: NotificationLevel;
  title: string;
  message?: string;
  detail?: string;
  createdAt: number;
  autoCloseMs: number | null;
  dedupeKey?: string;
  actions?: NotificationAction[];
};

type NotificationInput = {
  kind?: NotificationKind;
  level: NotificationLevel;
  title: string;
  message?: string;
  detail?: string;
  autoCloseMs?: number | null;
  dedupeKey?: string;
  actions?: NotificationAction[];
};

const DEFAULT_AUTO_CLOSE_MS: Record<NotificationLevel, number | null> = {
  error: null,
  warning: 7000,
  info: 5000,
  success: 3000,
};

const MAX_NOTIFICATIONS = 8;
const timers = new Map<string, ReturnType<typeof setTimeout>>();

export const notifications = writable<AppNotification[]>([]);

export function pushNotification(input: NotificationInput): string {
  if (!browser) return "server-skip";
  const now = Date.now();
  const notification: AppNotification = {
    id: "",
    kind: input.kind ?? "generic",
    level: input.level,
    title: input.title,
    message: input.message,
    detail: input.detail,
    createdAt: now,
    autoCloseMs: input.autoCloseMs ?? DEFAULT_AUTO_CLOSE_MS[input.level],
    dedupeKey: input.dedupeKey,
    actions: input.actions,
  };

  let createdId = "";

  notifications.update((list) => {
    const dedupeIndex = notification.dedupeKey
      ? list.findIndex((item) => item.dedupeKey === notification.dedupeKey)
      : -1;

    if (dedupeIndex >= 0) {
      const existing = list[dedupeIndex];
      const next = { ...notification, id: existing.id };
      createdId = existing.id;
      list.splice(dedupeIndex, 1);
      const updated = [next, ...list].slice(0, MAX_NOTIFICATIONS);
      syncTimer(next);
      cleanupDroppedTimers(updated);
      return updated;
    }

    const id = createNotificationId();
    const next = { ...notification, id };
    createdId = id;
    const updated = [next, ...list].slice(0, MAX_NOTIFICATIONS);
    syncTimer(next);
    cleanupDroppedTimers(updated);
    return updated;
  });

  return createdId;
}

export function dismissNotification(id: string) {
  if (!browser) return;
  clearTimer(id);
  notifications.update((list) => list.filter((item) => item.id !== id));
}

export function dismissByDedupeKey(dedupeKey: string) {
  if (!browser) return;
  notifications.update((list) => {
    const ids = list
      .filter((item) => item.dedupeKey === dedupeKey)
      .map((item) => item.id);
    for (const id of ids) {
      clearTimer(id);
    }
    return list.filter((item) => item.dedupeKey !== dedupeKey);
  });
}

export function notifyError(title: string, error: unknown, dedupeKey?: string) {
  if (!browser) return;
  const payload = extractErrorPayload(error);
  const level = errorLevelFromCode(payload.code);
  const detailParts: string[] = [];
  if (payload.code) {
    detailParts.push(`错误码: ${payload.code}`);
  }
  if (payload.detail) {
    detailParts.push(payload.detail);
  }

  pushNotification({
    kind: "system",
    level,
    title,
    message: payload.message,
    detail: detailParts.join("\n") || undefined,
    dedupeKey,
    autoCloseMs: level === "error" ? null : 7000,
  });
}

export function notifyCommandResult(message: string, tone: NotificationTone, detail?: string) {
  if (!browser) return;
  const level: NotificationLevel = tone === "error" ? "error" : tone === "warning" ? "warning" : "success";
  pushNotification({
    kind: "command",
    level,
    title: message,
    detail,
    dedupeKey: "command-last-result",
    autoCloseMs: tone === "error" ? null : tone === "warning" ? 6000 : 2500,
  });
}

function syncTimer(notification: AppNotification) {
  clearTimer(notification.id);
  if (notification.autoCloseMs === null || notification.autoCloseMs <= 0) return;
  const timer = setTimeout(() => {
    dismissNotification(notification.id);
  }, notification.autoCloseMs);
  timers.set(notification.id, timer);
}

function cleanupDroppedTimers(list: AppNotification[]) {
  const ids = new Set(list.map((item) => item.id));
  for (const [id, timer] of timers.entries()) {
    if (ids.has(id)) continue;
    clearTimeout(timer);
    timers.delete(id);
  }
}

function clearTimer(id: string) {
  const timer = timers.get(id);
  if (!timer) return;
  clearTimeout(timer);
  timers.delete(id);
}

function createNotificationId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(16).slice(2, 10)}`;
}

function errorLevelFromCode(code: string | undefined): NotificationLevel {
  if (!code) return "error";
  if (code === "validation" || code === "conflict" || code === "not_found") {
    return "warning";
  }
  return "error";
}
