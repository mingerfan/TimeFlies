import type { TaskRecord } from "$lib/api";
import { COMMAND_NAMES, type ParsedCommandInput } from "./parser";

export type CommandFeedbackTone = "success" | "error" | "warning";

export type CommandExecutionResult = {
  ok: boolean;
  tone: CommandFeedbackTone;
  message: string;
  clearInput: boolean;
};

type CommandRunApi = {
  createTask: (title: string, parentId: string | null) => Promise<string | null>;
  renameTask: (taskId: string, title: string) => Promise<boolean>;
  reparentTask: (taskId: string, parentId: string | null) => Promise<boolean>;
  startTask: (taskId: string) => Promise<boolean>;
  pauseTask: (taskId: string) => Promise<boolean>;
  resumeTask: (taskId: string) => Promise<boolean>;
  stopTask: (taskId: string) => Promise<boolean>;
  insertSubtaskAndStart: (parentTaskId: string, title: string) => Promise<string | null>;
  addTagToTask: (taskId: string, tagName: string) => Promise<boolean>;
};

export type CommandExecutionContext = {
  parsed: ParsedCommandInput;
  selectedTask: TaskRecord | null;
  selectedTaskId: string | null;
  activeTask: TaskRecord | null;
  tasks: TaskRecord[];
  run: CommandRunApi;
  ensureSwitchFromActive: (targetTaskId: string) => Promise<boolean>;
  selectTask: (taskId: string | null) => void;
};

type MainExecutionResult = {
  ok: boolean;
  message: string;
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
    return failure(
      `命令错误：未知命令 /${parsed.name}（可用：${COMMAND_NAMES.map((name) => `/${name}`).join(" ")})`,
      false
    );
  }

  const mainResult =
    parsed.kind === "create"
      ? await executeCreateAction(context, parsed.title)
      : await executeCommandAction(context, parsed.name, parsed.argument);

  if (!mainResult.ok) {
    return failure(mainResult.message, mainResult.clearInput);
  }

  const tagResult = await applyTags(mainResult.targetTaskId, parsed.tags, context.run.addTagToTask);
  return decorateTagFeedback(mainResult, tagResult, parsed.invalidTags);
}

async function executeCreateAction(
  context: CommandExecutionContext,
  title: string
): Promise<MainExecutionResult> {
  const createdTaskId = await context.run.createTask(title, context.selectedTaskId);
  if (!createdTaskId) {
    return {
      ok: false,
      message: "创建任务失败",
      targetTaskId: null,
      clearInput: false,
    };
  }

  context.selectTask(createdTaskId);
  return {
    ok: true,
    message: `已创建任务「${title}」`,
    targetTaskId: createdTaskId,
    clearInput: true,
  };
}

async function executeCommandAction(
  context: CommandExecutionContext,
  commandName: (typeof COMMAND_NAMES)[number],
  argument: string
): Promise<MainExecutionResult> {
  const selectedTask = context.selectedTask;
  if (!selectedTask) {
    return {
      ok: false,
      message: "请先在右侧 Mini 任务树选择一个任务",
      targetTaskId: null,
      clearInput: false,
    };
  }

  if (commandName === "rename") {
    const title = argument.trim();
    if (!title) return failMain("命令错误：/rename 需要标题");
    const ok = await context.run.renameTask(selectedTask.id, title);
    return ok ? succeedMain(`已重命名为「${title}」`, selectedTask.id) : failMain("重命名失败");
  }

  if (commandName === "parent") {
    const target = resolveParentTarget(argument, context.tasks);
    if (!target.ok) return failMain(target.message);
    if (target.parentId === selectedTask.id) return failMain("命令错误：父任务不能是自己");

    const ok = await context.run.reparentTask(selectedTask.id, target.parentId);
    if (!ok) return failMain("调整父任务失败");
    return succeedMain(target.parentId ? `已调整父任务为「${target.label}」` : "已设为根任务", selectedTask.id);
  }

  if (commandName === "start") {
    if (selectedTask.status === "running") {
      return succeedMain(`任务「${selectedTask.title}」已在进行中`, selectedTask.id);
    }
    if (!(await context.ensureSwitchFromActive(selectedTask.id))) {
      return failMain("命令错误：无法切换当前活动任务");
    }
    const ok =
      selectedTask.status === "paused"
        ? await context.run.resumeTask(selectedTask.id)
        : await context.run.startTask(selectedTask.id);
    return ok ? succeedMain(`已开始任务「${selectedTask.title}」`, selectedTask.id) : failMain("开始任务失败");
  }

  if (commandName === "pause") {
    if (selectedTask.status !== "running") {
      return failMain("命令错误：当前任务不在进行中");
    }
    const ok = await context.run.pauseTask(selectedTask.id);
    return ok ? succeedMain(`已暂停任务「${selectedTask.title}」`, selectedTask.id) : failMain("暂停任务失败");
  }

  if (commandName === "resume") {
    if (selectedTask.status !== "paused") {
      return failMain("命令错误：当前任务不在暂停状态");
    }
    if (!(await context.ensureSwitchFromActive(selectedTask.id))) {
      return failMain("命令错误：无法切换当前活动任务");
    }
    const ok = await context.run.resumeTask(selectedTask.id);
    return ok ? succeedMain(`已恢复任务「${selectedTask.title}」`, selectedTask.id) : failMain("恢复任务失败");
  }

  if (commandName === "stop") {
    if (selectedTask.status !== "running" && selectedTask.status !== "paused") {
      return failMain("命令错误：当前任务未开始，无法停止");
    }
    const ok = await context.run.stopTask(selectedTask.id);
    return ok ? succeedMain(`已停止任务「${selectedTask.title}」`, selectedTask.id) : failMain("停止任务失败");
  }

  const subtaskTitle = argument.trim();
  if (!subtaskTitle) {
    return failMain("命令错误：/sub 需要子任务标题");
  }

  if (selectedTask.status === "running") {
    const childTaskId = await context.run.insertSubtaskAndStart(selectedTask.id, subtaskTitle);
    if (!childTaskId) return failMain("插入子任务失败");
    context.selectTask(childTaskId);
    return succeedMain(`已插入并开始子任务「${subtaskTitle}」`, childTaskId);
  }

  const childTaskId = await context.run.createTask(subtaskTitle, selectedTask.id);
  if (!childTaskId) return failMain("创建子任务失败");
  context.selectTask(childTaskId);
  return succeedMain(`已创建子任务「${subtaskTitle}」`, childTaskId);
}

function resolveParentTarget(
  argument: string,
  tasks: TaskRecord[]
): { ok: true; parentId: string | null; label: string } | { ok: false; message: string } {
  const rawTarget = argument.trim();
  if (!rawTarget) {
    return {
      ok: false,
      message: "命令错误：/parent 需要目标（root 或 task_id_or_title）",
    };
  }

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
      message: `命令错误：/parent 命中多个同名任务「${rawTarget}」，请改用 task id`,
    };
  }

  return {
    ok: false,
    message: `命令错误：/parent 目标不存在（${rawTarget}）`,
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

function failMain(message: string): MainExecutionResult {
  return {
    ok: false,
    message,
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

function failure(message: string, clearInput: boolean): CommandExecutionResult {
  return {
    ok: false,
    tone: "error",
    message,
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
