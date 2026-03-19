<script lang="ts">
  import {
    APP_DATA_CHANGED_EVENT,
    getOverview,
    pauseTask,
    resumeTask,
    startTask,
    stopTask,
    type OverviewResponse,
    type TaskRecord,
  } from "$lib/api";
  import CommandBar from "$lib/components/CommandBar.svelte";
  import { handleCommandInput, type CommandRunActionOptions } from "$lib/command/handler";
  import { notifyCommandResult, notifyError } from "$lib/notifications";
  import {
    buildTaskChain,
    formatClock,
    formatSeconds,
    getRestElapsedSeconds,
    normalizeError,
    restSession,
    startManualRest,
    statusLabel,
    stopRest,
  } from "$lib/ui";
  import { onMount } from "svelte";

  let overview = $state<OverviewResponse | null>(null);
  let selectedTaskId = $state<string | null>(null);
  let loading = $state(false);
  let currentAction = $state("");
  let nowTs = $state(Math.floor(Date.now() / 1000));
  let commandInput = $state("");
  let lastCommandRunErrorDetail = $state<string | null>(null);
  let refreshInFlight = false;

  const taskMap = $derived.by(() => {
    const map = new Map<string, TaskRecord>();
    for (const task of overview?.tasks ?? []) {
      map.set(task.id, task);
    }
    return map;
  });

  const activeTask = $derived.by(() =>
    overview?.active_task_id ? (taskMap.get(overview.active_task_id) ?? null) : null
  );

  const selectedTask = $derived.by(() =>
    selectedTaskId ? (taskMap.get(selectedTaskId) ?? null) : activeTask
  );

  const restTargetTask = $derived.by(() => activeTask ?? selectedTask);

  const canStartRest = $derived.by(() => {
    const task = restTargetTask;
    return !!task && (task.status === "running" || task.status === "paused");
  });

  const selectedTaskPath = $derived.by(() =>
    buildTaskChain(selectedTask?.id ?? null, taskMap)
      .map((task) => task.title)
      .join(" / ")
  );

  const displayedSeconds = $derived.by(() => {
    const task = selectedTask;
    if (!task) return 0;
    const base = task.exclusive_seconds;
    if (task.status !== "running") return base;
    if (!overview) return base;
    const delta = Math.max(0, nowTs - overview.generated_at);
    return base + delta;
  });

  const restElapsedSeconds = $derived.by(() =>
    $restSession.active ? getRestElapsedSeconds($restSession, nowTs) : 0
  );

  onMount(() => {
    void refresh();
    const onDataChanged = () => {
      if (refreshInFlight || !!currentAction) return;
      void refresh({ background: true });
    };
    window.addEventListener(APP_DATA_CHANGED_EVENT, onDataChanged);
    const ticker = window.setInterval(() => {
      nowTs = Math.floor(Date.now() / 1000);
    }, 1_000);
    const poller = window.setInterval(() => {
      if (refreshInFlight || !!currentAction) return;
      void refresh({ background: true });
    }, 15_000);
    return () => {
      window.removeEventListener(APP_DATA_CHANGED_EVENT, onDataChanged);
      window.clearInterval(ticker);
      window.clearInterval(poller);
    };
  });

  $effect(() => {
    if (!overview) return;
    if (selectedTaskId && taskMap.has(selectedTaskId)) return;
    if (overview.active_task_id) {
      selectedTaskId = overview.active_task_id;
      return;
    }
    selectedTaskId = overview.last_used_task_id ?? overview.tasks[0]?.id ?? null;
  });

  async function refresh(options: { background?: boolean } = {}) {
    const { background = false } = options;
    if (refreshInFlight) return;
    refreshInFlight = true;
    if (!background) {
      loading = true;
    }
    try {
      overview = await getOverview("all");
    } catch (error) {
      notifyError("刷新计时页数据失败", error, "timer-refresh-error");
    } finally {
      refreshInFlight = false;
      if (!background) {
        loading = false;
      }
    }
  }

  async function runAction<T>(
    label: string,
    action: () => Promise<T>,
    options: CommandRunActionOptions = {}
  ): Promise<T | null> {
    const { surfaceError = true } = options;
    currentAction = label;
    lastCommandRunErrorDetail = null;
    try {
      const result = await action();
      await refresh();
      return result;
    } catch (error) {
      lastCommandRunErrorDetail = normalizeError(error);
      if (surfaceError) {
        notifyError(`${label}失败`, error, `timer-action-error:${label}`);
      }
      return null;
    } finally {
      currentAction = "";
    }
  }

  async function ensureSwitchFromActive(
    targetTaskId: string,
    options: CommandRunActionOptions = {}
  ): Promise<boolean> {
    const active = activeTask;
    if (!active || active.id === targetTaskId || active.status !== "running") {
      return true;
    }
    const paused = await runAction("暂停当前任务", () => pauseTask(active.id), options);
    return paused !== null;
  }

  async function onMainToggle() {
    const task = selectedTask;
    if (!task) return;
    if (task.status === "running") {
      await runAction("暂停任务", () => pauseTask(task.id));
      return;
    }
    if (task.status === "paused") {
      if (!(await ensureSwitchFromActive(task.id))) return;
      await runAction("恢复任务", () => resumeTask(task.id));
      return;
    }
    if (!(await ensureSwitchFromActive(task.id))) return;
    await runAction("开始任务", () => startTask(task.id));
  }

  async function onStopSelected() {
    const task = selectedTask;
    if (!task) return;
    if (task.status !== "running" && task.status !== "paused") return;
    await runAction("停止任务", () => stopTask(task.id));
  }

  async function onCommandExecute(input: string) {
    await handleCommandInput({
      input,
      selectedTask,
      selectedTaskId,
      activeTask,
      tasks: overview?.tasks ?? [],
      getLastRunErrorDetail: () => lastCommandRunErrorDetail,
      clearLastRunErrorDetail: () => {
        lastCommandRunErrorDetail = null;
      },
      runAction,
      ensureSwitchFromActive,
      selectTask: (taskId) => (selectedTaskId = taskId),
      clearErrorMessage: () => {},
      setCommandFeedback: (message, tone, detail) => {
        notifyCommandResult(message, tone, detail);
      },
      clearCommandInput: () => {
        commandInput = "";
      },
    });
  }

  async function onToggleRest() {
    if ($restSession.active) {
      stopRest();
      return;
    }

    const task = restTargetTask;
    if (!task) return;
    if (task.status === "running") {
      const paused = await runAction("开始休息", () => pauseTask(task.id));
      if (paused === null) return;
    } else if (task.status !== "paused") {
      return;
    }

    startManualRest();
  }
</script>

<main class="timer-screen">
  <header class="page-head">
    <div>
      <p class="eyebrow">计时页面</p>
      <h1>{$restSession.active ? "休息计时器" : "任务会话计时器"}</h1>
      <p class="sub">
        {$restSession.active
          ? "休息只复用现有暂停语义，前端额外显示休息正计时。"
          : "非番茄模式：只记录任务会话，不做固定时长强制中断"}
      </p>
    </div>
    <button type="button" class="secondary" onclick={() => void refresh()} disabled={loading || !!currentAction}>
      {loading ? "刷新中..." : "手动刷新"}
    </button>
  </header>

  <section class="timer-main">
    <article class="panel focus-panel">
      {#if selectedTask || $restSession.active}
        <p class="task-name">{$restSession.active ? "休息中" : selectedTask?.title}</p>
        <p class="task-path">
          {$restSession.active
            ? selectedTaskPath
              ? `关联任务 ${selectedTaskPath}`
              : "已进入休息正计时"
            : selectedTaskPath}
        </p>
        <p class="task-meta">
          {$restSession.active
            ? `当前任务按暂停语义处理 · 已休息 ${formatSeconds(restElapsedSeconds)}`
            : `状态 ${selectedTask ? statusLabel(selectedTask.status) : "-"}`}
        </p>
        <p class="clock">{formatClock($restSession.active ? restElapsedSeconds : displayedSeconds)}</p>
        <p class="task-meta">
          {#if $restSession.active}
            休息正计时持续中，可点击“结束休息”退出显示
          {:else}
            当前任务累计 {formatSeconds(displayedSeconds)}
          {/if}
        </p>

        <div class="actions">
          <button type="button" onclick={onMainToggle} disabled={!!currentAction || !selectedTask}>
            {selectedTask?.status === "running"
              ? "暂停"
              : selectedTask?.status === "paused"
                ? "恢复"
                : "开始"}
          </button>
          <button type="button" class="secondary" onclick={onToggleRest} disabled={!!currentAction || (!$restSession.active && !canStartRest)}>
            {$restSession.active ? "结束休息" : "休息"}
          </button>
          <button
            type="button"
            class="danger"
            onclick={onStopSelected}
            disabled={
              !!currentAction ||
              !selectedTask ||
              (selectedTask.status !== "running" && selectedTask.status !== "paused")
            }
          >
            停止
          </button>
        </div>

        <section class="timer-command">
          <h2>命令模式</h2>
          <p class="task-meta">输入纯文本可直接创建并开始子任务；需要忽略当前上下文时可用 `/new 根任务标题`。</p>
          <CommandBar
            bind:value={commandInput}
            busy={!!currentAction}
            tasks={overview?.tasks ?? []}
            onexecute={onCommandExecute}
          />
        </section>
      {:else}
        <p class="empty">暂无任务。请先在“任务执行”页面创建任务。</p>
        <div class="actions">
          <button type="button" class="secondary" disabled>
            休息
          </button>
        </div>
      {/if}
    </article>
  </section>

</main>

<style>
  .timer-screen {
    display: flex;
    flex-direction: column;
    gap: 0.95rem;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  .page-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
    flex-shrink: 0;
  }

  .eyebrow {
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.74rem;
    color: #3f608d;
  }

  h1 {
    margin: 0.15rem 0;
    font-size: clamp(1.65rem, 2.1vw, 2.1rem);
  }

  .sub {
    margin: 0;
    color: #3f5f85;
    font-size: 0.9rem;
  }

  .timer-main {
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .panel {
    background: rgba(255, 255, 255, 0.88);
    border: 1px solid rgba(65, 97, 143, 0.28);
    border-radius: 1rem;
    padding: 1rem;
  }

  .focus-panel {
    display: flex;
    flex-direction: column;
    justify-content: center;
    flex: 1;
    gap: 0.55rem;
    align-items: center;
    text-align: center;
    width: min(960px, 100%);
    max-height: 100%;
    height: 100%;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .task-name {
    margin: 0;
    font-size: 1.2rem;
    font-weight: 700;
    color: #102f53;
  }

  .task-path,
  .task-meta {
    margin: 0;
    color: #4a6b90;
    font-size: 0.86rem;
    line-height: 1.35;
  }

  .clock {
    margin: 0.35rem 0;
    font-family: "IBM Plex Mono", "Cascadia Mono", monospace;
    font-size: clamp(2.5rem, 8vw, 5.4rem);
    line-height: 1.05;
    color: #153a66;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
    justify-content: center;
  }

  .timer-command {
    width: min(680px, 100%);
    margin-top: 0.5rem;
    border-top: 1px dashed #c1d4ee;
    padding-top: 0.74rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    text-align: left;
  }

  .timer-command h2 {
    margin: 0;
    font-size: 1rem;
    color: #173c68;
  }

  button {
    font: inherit;
  }

  button {
    border: none;
    border-radius: 0.62rem;
    background: #2f629f;
    color: #fff;
    padding: 0.5rem 0.72rem;
    cursor: pointer;
    transition: background 120ms ease, color 120ms ease, opacity 120ms ease;
  }

  button.secondary {
    background: #edf3ff;
    color: #2f629f;
  }

  button.danger {
    background: #8b2a2a;
  }

  button:hover:not(:disabled) {
    background: #28578f;
  }

  button.secondary:hover:not(:disabled) {
    background: #e2ecff;
  }

  button.danger:hover:not(:disabled) {
    background: #742121;
  }

  button:disabled {
    opacity: 0.56;
    cursor: not-allowed;
  }

  .empty {
    margin: 0;
    color: #54759b;
    font-size: 0.9rem;
  }

  @media (max-height: 700px) {
    .timer-screen {
      height: auto;
      min-height: 100%;
      overflow: visible;
    }

    .timer-main {
      flex: 0 0 auto;
      min-height: fit-content;
      overflow: visible;
    }

    .focus-panel {
      overflow: visible;
    }
  }

  @media (max-width: 760px) {
    .page-head {
      flex-direction: column;
    }
  }
</style>

