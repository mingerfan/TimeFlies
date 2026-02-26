import {
  addTagToTask,
  createTask,
  insertSubtaskAndStart,
  pauseTask,
  renameTask,
  reparentTask,
  resumeTask,
  startTask,
  stopTask,
  type TaskRecord,
} from "$lib/api";
import {
  executeParsedCommand,
  type CommandExecutionResult,
  type CommandFeedbackTone,
  type CommandRunApi,
} from "./executor";
import { parseCommandInput } from "./parser";

export type CommandRunActionOptions = {
  surfaceError?: boolean;
};

export type CommandRunAction = <T>(
  label: string,
  action: () => Promise<T>,
  options?: CommandRunActionOptions
) => Promise<T | null>;

export type HandleCommandInputArgs = {
  input: string;
  selectedTask: TaskRecord | null;
  selectedTaskId: string | null;
  activeTask: TaskRecord | null;
  tasks: TaskRecord[];
  runAction: CommandRunAction;
  ensureSwitchFromActive: (
    targetTaskId: string,
    options?: CommandRunActionOptions
  ) => Promise<boolean>;
  selectTask: (taskId: string | null) => void;
  clearErrorMessage: () => void;
  setCommandFeedback: (message: string, tone: CommandFeedbackTone) => void;
  clearCommandInput: () => void;
};

export async function handleCommandInput(
  args: HandleCommandInputArgs
): Promise<CommandExecutionResult> {
  args.clearErrorMessage();
  const parsed = parseCommandInput(args.input);
  const result = await executeParsedCommand({
    parsed,
    selectedTask: args.selectedTask,
    selectedTaskId: args.selectedTaskId,
    activeTask: args.activeTask,
    tasks: args.tasks,
    run: createCommandRunApi(args.runAction),
    ensureSwitchFromActive: (targetTaskId) =>
      args.ensureSwitchFromActive(targetTaskId, { surfaceError: false }),
    selectTask: args.selectTask,
  });

  args.setCommandFeedback(result.message, result.tone);
  if (result.clearInput) {
    args.clearCommandInput();
  }

  return result;
}

function createCommandRunApi(runAction: CommandRunAction): CommandRunApi {
  return {
    createTask: (title, parentId) =>
      runAction("创建任务", () => createTask(title, parentId), { surfaceError: false }),
    renameTask: async (taskId, title) =>
      (await runAction("重命名任务", () => renameTask(taskId, title), { surfaceError: false })) !== null,
    reparentTask: async (taskId, parentId) =>
      (await runAction("调整父任务", () => reparentTask(taskId, parentId), { surfaceError: false })) !== null,
    startTask: async (taskId) =>
      (await runAction("开始任务", () => startTask(taskId), { surfaceError: false })) !== null,
    pauseTask: async (taskId) =>
      (await runAction("暂停任务", () => pauseTask(taskId), { surfaceError: false })) !== null,
    resumeTask: async (taskId) =>
      (await runAction("恢复任务", () => resumeTask(taskId), { surfaceError: false })) !== null,
    stopTask: async (taskId) =>
      (await runAction("停止任务", () => stopTask(taskId), { surfaceError: false })) !== null,
    insertSubtaskAndStart: (parentTaskId, title) =>
      runAction("插入子任务", () => insertSubtaskAndStart(parentTaskId, title), {
        surfaceError: false,
      }),
    addTagToTask: async (taskId, tagName) =>
      (await runAction(`添加标签 #${tagName}`, () => addTagToTask(taskId, tagName), {
        surfaceError: false,
      })) !== null,
  };
}
