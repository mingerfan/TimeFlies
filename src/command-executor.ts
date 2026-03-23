import type { TaskRecord } from "$lib/api";
import { formatFocusAdjustmentDelta, parseFocusAdjustmentInput } from "$lib/focus-adjustment";

import { listPrimaryCommandNames } from "./command-catalog";
import type { CommandCatalog, CommandDefinition } from "./command-types";
import type { ParsedCommandInput } from "./command-parser";

export type CommandFeedbackTone = "success" | "error" | "warning";

export type CommandExecutionResult = {
  ok: boolean;
  tone: CommandFeedbackTone;
  message: string;
  detail?: string;
  clearInput: boolean;
};

export type CommandRunApi = {
  createTask: (title: string, parentId: string | null) => Promise<string | null>;
  renameTask: (taskId: string, title: string) => Promise<boolean>;
  reparentTask: (taskId: string, parentId: string | null) => Promise<boolean>;
  startTask: (taskId: string) => Promise<boolean>;
  pauseTask: (taskId: string) => Promise<boolean>;
  resumeTask: (taskId: string) => Promise<boolean>;
  stopTask: (taskId: string) => Promise<boolean>;
  adjustTaskFocus: (taskId: string, deltaSeconds: number) => Promise<boolean>;
  insertSubtaskAndStart: (parentTaskId: string, title: string) => Promise<string | null>;
  addTagToTask: (taskId: string, tagName: string) => Promise<boolean>;
};

export type CommandExecutionContext = {
  catalog: CommandCatalog;
  parsed: ParsedCommandInput;
  selectedTask: TaskRecord | null;
  selectedTaskId: string | null;
  activeTask: TaskRecord | null;
  tasks: TaskRecord[];
  run: CommandRunApi;
  getLastRunErrorDetail: () => string | null;
  ensureSwitchFromActive: (targetTaskId: string) => Promise<boolean>;
  selectTask: (taskId: string | null) => void;
};

type MainExecutionResult = {
  ok: boolean;
  message: string;
  detail?: string;
  targetTaskId: string | null;
  clearInput: boolean;
};

type TagApplyResult = {
  added: string[];
  failed: string[];
};

export async function executeParsedCommand(
  context: CommandExecutionContext
): Promise<CommandExecutionResult> {
  const { parsed } = context;

  if (parsed.kind === "empty") {
    return failure("请输入命令或任务标题", false);
  }

  if (parsed.kind === "invalid") {
    return failure(parsed.message, false);
  }

  if (parsed.kind === "unknown-command") {
    const available = listPrimaryCommandNames(context.catalog).map((name) => `/${name}`).join(" ");
    return failure(`命令错误：未知命令 /${parsed.name}（可用：${available}）`, false);
  }

  const mainResult =
    parsed.kind === "create"
      ? await executeCreateAction(context, parsed.title)
      : await executeCommandAction(context, parsed.command, parsed.argument);

  if (!mainResult.ok) {
    return failure(mainResult.message, mainResult.clearInput, mainResult.detail);
  }

  const tagResult = await applyTags(mainResult.targetTaskId, parsed.tags, context.run.addTagToTask);
  return decorateTagFeedback(mainResult, tagResult, parsed.invalidTags);
}

async function executeCreateAction(
  context: CommandExecutionContext,
  title: string
): Promise<MainExecutionResult> {
  const selectedTask = context.selectedTask;
  if (selectedTask) {
    return createAndStartSubtask(context, selectedTask, title);
  }

  const createdTaskId = await context.run.createTask(title, null);
  if (!createdTaskId) return failMain("创建任务失败", context.getLastRunErrorDetail());

  context.selectTask(createdTaskId);
  return succeedMain(`已创建任务「${title}」`, createdTaskId);
}

async function executeCommandAction(
  context: CommandExecutionContext,
  command: CommandDefinition,
  argument: string
): Promise<MainExecutionResult> {
  if (command.requires_selected_task && !context.selectedTask) {
    return {
      ok: false,
      message: "请先在右侧 Mini 任务树选择一个任务",
      targetTaskId: null,
      clearInput: false,
    };
  }

  switch (command.handler) {
    case "create_root_and_start":
      return executeCreateRootAndStart(context, command, argument);
    case "rename_selected":
      return executeRenameSelected(context, command, argument);
    case "reparent_selected":
      return executeReparentSelected(context, command, argument);
    case "start_selected":
      return executeStartSelected(context, command, argument);
    case "pause_selected":
      return executePauseSelected(context, command, argument);
    case "resume_selected":
      return executeResumeSelected(context, command, argument);
    case "stop_selected":
      return executeStopSelected(context, command, argument);
    case "adjust_focus_selected":
      return executeAdjustFocusSelected(context, command, argument);
    case "create_or_insert_subtask":
      return executeCreateOrInsertSubtask(context, command, argument);
  }
}

async function executeCreateRootAndStart(
  context: CommandExecutionContext,
  command: CommandDefinition,
  argument: string
): Promise<MainExecutionResult> {
  const title = requireArgument(command, argument);
  if (!title.ok) return failMain(title.message);

  const createdTaskId = await context.run.createTask(title.value, null);
  if (!createdTaskId) return failMain("创建根任务失败", context.getLastRunErrorDetail());
  context.selectTask(createdTaskId);

  if (!(await context.ensureSwitchFromActive(createdTaskId))) {
    return failMain("命令错误：无法切换当前活动任务", context.getLastRunErrorDetail());
  }

  const started = await context.run.startTask(createdTaskId);
  if (!started) return failMain("开始根任务失败", context.getLastRunErrorDetail());
  return succeedMain(`已创建并开始根任务「${title.value}」`, createdTaskId);
}

async function executeRenameSelected(
  context: CommandExecutionContext,
  command: CommandDefinition,
  argument: string
): Promise<MainExecutionResult> {
  const title = requireArgument(command, argument);
  if (!title.ok) return failMain(title.message);

  const selectedTask = context.selectedTask;
  if (!selectedTask) return failMain("请先选择一个任务");

  const ok = await context.run.renameTask(selectedTask.id, title.value);
  return ok
    ? succeedMain(`已重命名为「${title.value}」`, selectedTask.id)
    : failMain("重命名失败", context.getLastRunErrorDetail());
}

async function executeReparentSelected(
  context: CommandExecutionContext,
  command: CommandDefinition,
  argument: string
): Promise<MainExecutionResult> {
  const selectedTask = context.selectedTask;
  if (!selectedTask) return failMain("请先选择一个任务");

  const target = resolveParentTarget(command, argument, context.tasks);
  if (!target.ok) return failMain(target.message);
  if (target.parentId === selectedTask.id) return failMain("命令错误：父任务不能是自己");

  const ok = await context.run.reparentTask(selectedTask.id, target.parentId);
  if (!ok) return failMain("调整父任务失败", context.getLastRunErrorDetail());
  return succeedMain(target.parentId ? `已调整父任务为「${target.label}」` : "已设为根任务", selectedTask.id);
}

async function executeStartSelected(
  context: CommandExecutionContext,
  command: CommandDefinition,
  argument: string
): Promise<MainExecutionResult> {
  const noArgs = ensureNoUnexpectedArgument(command, argument);
  if (!noArgs.ok) return failMain(noArgs.message);

  const selectedTask = context.selectedTask;
  if (!selectedTask) return failMain("请先选择一个任务");

  if (selectedTask.status === "running") {
    return succeedMain(`任务「${selectedTask.title}」已在进行中`, selectedTask.id);
  }
  if (!(await context.ensureSwitchFromActive(selectedTask.id))) {
    return failMain("命令错误：无法切换当前活动任务", context.getLastRunErrorDetail());
  }
  const ok =
    selectedTask.status === "paused"
      ? await context.run.resumeTask(selectedTask.id)
      : await context.run.startTask(selectedTask.id);
  return ok
    ? succeedMain(`已开始任务「${selectedTask.title}」`, selectedTask.id)
    : failMain("开始任务失败", context.getLastRunErrorDetail());
}

async function executePauseSelected(
  context: CommandExecutionContext,
  command: CommandDefinition,
  argument: string
): Promise<MainExecutionResult> {
  const noArgs = ensureNoUnexpectedArgument(command, argument);
  if (!noArgs.ok) return failMain(noArgs.message);

  const selectedTask = context.selectedTask;
  if (!selectedTask) return failMain("请先选择一个任务");

  if (selectedTask.status !== "running") {
    return failMain("命令错误：当前任务不在进行中");
  }
  const ok = await context.run.pauseTask(selectedTask.id);
  return ok
    ? succeedMain(`已暂停任务「${selectedTask.title}」`, selectedTask.id)
    : failMain("暂停任务失败", context.getLastRunErrorDetail());
}

async function executeResumeSelected(
  context: CommandExecutionContext,
  command: CommandDefinition,
  argument: string
): Promise<MainExecutionResult> {
  const noArgs = ensureNoUnexpectedArgument(command, argument);
  if (!noArgs.ok) return failMain(noArgs.message);

  const selectedTask = context.selectedTask;
  if (!selectedTask) return failMain("请先选择一个任务");

  if (selectedTask.status !== "paused") {
    return failMain("命令错误：当前任务不在暂停状态");
  }
  if (!(await context.ensureSwitchFromActive(selectedTask.id))) {
    return failMain("命令错误：无法切换当前活动任务", context.getLastRunErrorDetail());
  }
  const ok = await context.run.resumeTask(selectedTask.id);
  return ok
    ? succeedMain(`已恢复任务「${selectedTask.title}」`, selectedTask.id)
    : failMain("恢复任务失败", context.getLastRunErrorDetail());
}

async function executeStopSelected(
  context: CommandExecutionContext,
  command: CommandDefinition,
  argument: string
): Promise<MainExecutionResult> {
  const noArgs = ensureNoUnexpectedArgument(command, argument);
  if (!noArgs.ok) return failMain(noArgs.message);

  const selectedTask = context.selectedTask;
  if (!selectedTask) return failMain("请先选择一个任务");

  if (selectedTask.status !== "running" && selectedTask.status !== "paused") {
    return failMain("命令错误：当前任务未开始，无法停止");
  }
  const ok = await context.run.stopTask(selectedTask.id);
  return ok
    ? succeedMain(`已停止任务「${selectedTask.title}」`, selectedTask.id)
    : failMain("停止任务失败", context.getLastRunErrorDetail());
}

async function executeAdjustFocusSelected(
  context: CommandExecutionContext,
  command: CommandDefinition,
  argument: string
): Promise<MainExecutionResult> {
  const selectedTask = context.selectedTask;
  if (!selectedTask) return failMain("请先选择一个任务");

  const parsed = parseFocusAdjustmentInput(argument, command.name);
  if (!parsed.ok) return failMain(parsed.message);

  const ok = await context.run.adjustTaskFocus(selectedTask.id, parsed.deltaSeconds);
  return ok
    ? succeedMain(
        `已调整任务「${selectedTask.title}」的专注时间（${formatFocusAdjustmentDelta(parsed.deltaSeconds)}）`,
        selectedTask.id
      )
    : failMain("调整专注时间失败", context.getLastRunErrorDetail());
}

async function executeCreateOrInsertSubtask(
  context: CommandExecutionContext,
  command: CommandDefinition,
  argument: string
): Promise<MainExecutionResult> {
  const title = requireArgument(command, argument);
  if (!title.ok) return failMain(title.message);

  const selectedTask = context.selectedTask;
  if (!selectedTask) return failMain("请先选择一个任务");

  return createAndStartSubtask(context, selectedTask, title.value);
}

async function createAndStartSubtask(
  context: CommandExecutionContext,
  parentTask: TaskRecord,
  subtaskTitle: string
): Promise<MainExecutionResult> {
  if (parentTask.status === "running") {
    const childTaskId = await context.run.insertSubtaskAndStart(parentTask.id, subtaskTitle);
    if (!childTaskId) return failMain("插入子任务失败", context.getLastRunErrorDetail());
    context.selectTask(childTaskId);
    return succeedMain(`已插入并开始子任务「${subtaskTitle}」`, childTaskId);
  }

  const childTaskId = await context.run.createTask(subtaskTitle, parentTask.id);
  if (!childTaskId) return failMain("创建子任务失败", context.getLastRunErrorDetail());
  context.selectTask(childTaskId);
  if (!(await context.ensureSwitchFromActive(childTaskId))) {
    return failMain("命令错误：无法切换当前活动任务", context.getLastRunErrorDetail());
  }

  const started = await context.run.startTask(childTaskId);
  if (!started) return failMain("开始子任务失败", context.getLastRunErrorDetail());
  return succeedMain(`已创建并开始子任务「${subtaskTitle}」`, childTaskId);
}

function requireArgument(
  command: CommandDefinition,
  argument: string
): { ok: true; value: string } | { ok: false; message: string } {
  const value = argument.trim();
  if (value) {
    return {
      ok: true,
      value,
    };
  }

  const argumentName = command.argument?.placeholder ?? command.argument?.name ?? "参数";
  return {
    ok: false,
    message: `命令错误：/${command.name} 需要${argumentName}`,
  };
}

function ensureNoUnexpectedArgument(
  command: CommandDefinition,
  argument: string
): { ok: true } | { ok: false; message: string } {
  if (!argument.trim()) {
    return { ok: true };
  }

  return {
    ok: false,
    message: `命令错误：/${command.name} 不接受额外参数`,
  };
}

function resolveParentTarget(
  command: CommandDefinition,
  argument: string,
  tasks: TaskRecord[]
): { ok: true; parentId: string | null; label: string } | { ok: false; message: string } {
  const required = requireArgument(command, argument);
  if (!required.ok) {
    return {
      ok: false,
      message: required.message,
    };
  }

  const rawTarget = required.value;
  if (rawTarget.toLowerCase() === "root") {
    return {
      ok: true,
      parentId: null,
      label: "根任务",
    };
  }

  const byId = tasks.find((task) => task.id === rawTarget);
  if (byId) {
    return {
      ok: true,
      parentId: byId.id,
      label: byId.title,
    };
  }

  const byTitle = tasks.filter((task) => task.title.toLowerCase() === rawTarget.toLowerCase());
  if (byTitle.length === 1) {
    return {
      ok: true,
      parentId: byTitle[0].id,
      label: byTitle[0].title,
    };
  }

  if (byTitle.length > 1) {
    return {
      ok: false,
      message: `命令错误：/${command.name} 命中多个同名任务「${rawTarget}」，请改用 task id`,
    };
  }

  return {
    ok: false,
    message: `命令错误：/${command.name} 目标不存在（${rawTarget}）`,
  };
}

async function applyTags(
  targetTaskId: string | null,
  tags: string[],
  addTag: (taskId: string, tagName: string) => Promise<boolean>
): Promise<TagApplyResult> {
  if (!targetTaskId || tags.length === 0) {
    return {
      added: [],
      failed: targetTaskId ? [] : tags,
    };
  }

  const added: string[] = [];
  const failed: string[] = [];

  for (const tag of tags) {
    const ok = await addTag(targetTaskId, tag);
    if (ok) {
      added.push(tag);
    } else {
      failed.push(tag);
    }
  }

  return { added, failed };
}

function decorateTagFeedback(
  main: MainExecutionResult,
  tagResult: TagApplyResult,
  invalidTags: string[]
): CommandExecutionResult {
  const notices: string[] = [];
  if (tagResult.added.length > 0) {
    notices.push(`标签已添加：${tagResult.added.map((tag) => `#${tag}`).join(" ")}`);
  }
  if (tagResult.failed.length > 0) {
    notices.push(`标签失败：${tagResult.failed.map((tag) => `#${tag}`).join(" ")}`);
  }
  if (invalidTags.length > 0) {
    notices.push(`标签语法无效：${invalidTags.join(" ")}`);
  }

  if (notices.length === 0) {
    return success(main.message, main.clearInput);
  }

  const tone: CommandFeedbackTone =
    tagResult.failed.length > 0 || invalidTags.length > 0 ? "warning" : "success";

  return {
    ok: true,
    tone,
    message: `${main.message} · ${notices.join("；")}`,
    clearInput: main.clearInput,
  };
}

function failMain(message: string, detail?: string | null): MainExecutionResult {
  return {
    ok: false,
    message,
    detail: detail ?? undefined,
    targetTaskId: null,
    clearInput: false,
  };
}

function succeedMain(message: string, targetTaskId: string): MainExecutionResult {
  return {
    ok: true,
    message,
    targetTaskId,
    clearInput: true,
  };
}

function failure(message: string, clearInput: boolean, detail?: string): CommandExecutionResult {
  return {
    ok: false,
    tone: "error",
    message,
    detail,
    clearInput,
  };
}

function success(message: string, clearInput: boolean): CommandExecutionResult {
  return {
    ok: true,
    tone: "success",
    message,
    clearInput,
  };
}

