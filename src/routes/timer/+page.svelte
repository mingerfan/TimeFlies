<script lang="ts">
  import {
    APP_DATA_CHANGED_EVENT,
    getOverview,
    insertSubtaskAndStart,
    pauseTask,
    respondRestSuggestion,
    resumeTask,
    startTask,
    stopTask,
    type OverviewResponse,
    type TaskRecord,
  } from "$lib/api";
  import {
    buildTaskChain,
    formatClock,
    formatDeviation,
    formatSeconds,
    normalizeError,
    restHeadline,
    restTriggerLabel,
    statusLabel,
  } from "$lib/ui";
  import { onMount } from "svelte";

  let overview = $state<OverviewResponse | null>(null);
  let selectedTaskId = $state<string | null>(null);
  let loading = $state(false);
  let currentAction = $state("");
  let errorMessage = $state("");
  let nowTs = $state(Math.floor(Date.now() / 1000));
  let subtaskTitle = $state("");

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

  const selectedTaskPath = $derived.by(() =>
    buildTaskChain(selectedTask?.id ?? null, taskMap)
      .map((task) => task.title)
      .join(" / ")
  );

  const recentTasks = $derived.by(() =>
    [...(overview?.tasks ?? [])].sort((a, b) => b.created_at - a.created_at).slice(0, 10)
  );

  const totalDaySeconds = $derived.by(() =>
    (overview?.tasks ?? []).reduce((sum, task) => sum + task.exclusive_seconds, 0)
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

  const restSuggestion = $derived.by(() => overview?.rest_suggestion ?? null);

  onMount(() => {
    void refresh();
    const onDataChanged = () => {
      if (loading || !!currentAction) return;
      void refresh();
    };
    window.addEventListener(APP_DATA_CHANGED_EVENT, onDataChanged);
    const ticker = window.setInterval(() => {
      nowTs = Math.floor(Date.now() / 1000);
    }, 1_000);
    const poller = window.setInterval(() => void refresh(), 15_000);
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
    selectedTaskId = overview.tasks[0]?.id ?? null;
  });

  async function refresh() {
    loading = true;
    errorMessage = "";
    try {
      overview = await getOverview("day");
    } catch (error) {
      errorMessage = normalizeError(error);
    } finally {
      loading = false;
    }
  }

  async function runAction(label: string, action: () => Promise<void>): Promise<boolean> {
    currentAction = label;
    errorMessage = "";
    try {
      await action();
      await refresh();
      return true;
    } catch (error) {
      errorMessage = normalizeError(error);
      return false;
    } finally {
      currentAction = "";
    }
  }

  async function switchToTask(task: TaskRecord) {
    const active = activeTask;
    if (active && active.id !== task.id && active.status === "running") {
      const confirmed = window.confirm(
        `当前任务「${active.title}」正在进行中，切换到「${task.title}」将先暂停当前任务。是否继续？`
      );
      if (!confirmed) return;
      const paused = await runAction("暂停当前任务", () => pauseTask(active.id));
      if (!paused) return;
    }

    if (task.status === "running") {
      selectedTaskId = task.id;
      return;
    }

    if (task.status === "paused") {
      const resumed = await runAction("恢复任务", () => resumeTask(task.id));
      if (resumed) {
        selectedTaskId = task.id;
      }
      return;
    }

    const started = await runAction("开始任务", () => startTask(task.id));
    if (started) {
      selectedTaskId = task.id;
    }
  }

  async function onMainToggle() {
    const task = selectedTask;
    if (!task) return;
    if (task.status === "running") {
      await runAction("暂停任务", () => pauseTask(task.id));
      return;
    }
    if (task.status === "paused") {
      await runAction("恢复任务", () => resumeTask(task.id));
      return;
    }
    await switchToTask(task);
  }

  async function onStopSelected() {
    const task = selectedTask;
    if (!task) return;
    if (task.status !== "running" && task.status !== "paused") return;
    await runAction("停止任务", () => stopTask(task.id));
  }

  async function onInsertSubtask(event: SubmitEvent) {
    event.preventDefault();
    const task = selectedTask;
    if (!task || task.status !== "running") return;
    const title = subtaskTitle.trim();
    if (!title) return;
    const done = await runAction("插入子任务", async () => {
      const childId = await insertSubtaskAndStart(task.id, title);
      selectedTaskId = childId;
    });
    if (done) {
      subtaskTitle = "";
    }
  }

  async function onRespondRestSuggestion(accept: boolean) {
    const suggestion = restSuggestion;
    if (!suggestion) return;
    await runAction(accept ? "接受休息建议" : "忽略休息建议", () =>
      respondRestSuggestion(suggestion.id, accept)
    );
  }

  function recentTaskRootTitle(task: TaskRecord): string {
    const chain = buildTaskChain(task.id, taskMap);
    return chain[0]?.title ?? "";
  }

  function recentTaskMeta(task: TaskRecord): string {
    const status = statusLabel(task.status);
    const rootTitle = recentTaskRootTitle(task);
    if (!rootTitle || rootTitle === task.title) return status;
    return `${status} . ${rootTitle}`;
  }

  function recentTaskTooltip(task: TaskRecord): string {
    const chainText = buildTaskChain(task.id, taskMap)
      .map((node) => node.title)
      .join(" / ");
    return `${task.title}\n状态 ${statusLabel(task.status)} · Ex ${formatSeconds(task.exclusive_seconds)}\n路径 ${chainText}`;
  }
</script>

<main class="timer-screen">
  <header class="page-head">
    <div>
      <p class="eyebrow">计时页面</p>
      <h1>任务会话计时器</h1>
      <p class="sub">非番茄模式：只记录任务会话，不做固定时长强制中断</p>
    </div>
    <button type="button" class="secondary" onclick={refresh} disabled={loading || !!currentAction}>
      {loading ? "刷新中..." : "手动刷新"}
    </button>
  </header>

  {#if errorMessage}
    <p class="error">{errorMessage}</p>
  {/if}

  <section class="timer-grid">
    <article class="panel focus-panel">
      {#if selectedTask}
        <p class="task-name">{selectedTask.title}</p>
        <p class="task-path">{selectedTaskPath}</p>
        <p class="task-meta">状态 {statusLabel(selectedTask.status)}</p>
        <p class="clock">{formatClock(displayedSeconds)}</p>
        <p class="task-meta">当前任务累计 {formatSeconds(displayedSeconds)}</p>

        <div class="actions">
          <button type="button" onclick={onMainToggle} disabled={!!currentAction}>
            {selectedTask.status === "running"
              ? "暂停"
              : selectedTask.status === "paused"
                ? "恢复"
                : "开始"}
          </button>
          <button
            type="button"
            class="danger"
            onclick={onStopSelected}
            disabled={!!currentAction || (selectedTask.status !== "running" && selectedTask.status !== "paused")}
          >
            停止
          </button>
        </div>

        <form class="subtask-form" onsubmit={onInsertSubtask}>
          <label for="subtask-input">运行中插入子任务</label>
          <div class="subtask-row">
            <input
              id="subtask-input"
              type="text"
              bind:value={subtaskTitle}
              placeholder="子任务标题"
              disabled={selectedTask.status !== "running" || !!currentAction}
            />
            <button
              type="submit"
              disabled={selectedTask.status !== "running" || !!currentAction || !subtaskTitle.trim()}
            >
              插入并开始
            </button>
          </div>
        </form>
      {:else}
        <p class="empty">暂无任务。请先在“任务执行”页面创建任务。</p>
      {/if}
    </article>

    <section class="side-column">
      <aside class="panel side-panel metrics-panel">
        <h2>专注时间</h2>
        <div class="metric">
          <p class="label">今日累计专注</p>
          <p class="value">{formatSeconds(totalDaySeconds)}</p>
        </div>
        <div class="metric">
          <p class="label">当前活动任务</p>
          <p class="value small">{activeTask ? activeTask.title : "无"}</p>
        </div>
      </aside>

      <aside class="panel side-panel switch-panel">
        <h2>快速切换任务</h2>
        {#if recentTasks.length === 0}
          <p class="empty">暂无可切换任务</p>
        {:else}
          <ul class="recent-list">
            {#each recentTasks as task (task.id)}
              <li>
                <button
                  type="button"
                  class="recent-btn"
                  class:selected={selectedTask?.id === task.id}
                  onclick={() => switchToTask(task)}
                  disabled={!!currentAction}
                  title={recentTaskTooltip(task)}
                >
                  <span class="recent-title-row">
                    <span class="recent-title">{task.title}</span>
                    <span class="recent-action">切换并开始</span>
                  </span>
                  <span class="recent-meta">{recentTaskMeta(task)}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </aside>
    </section>
  </section>

  {#if restSuggestion}
    <section class="suggestion">
      <p class="headline">{restHeadline(restSuggestion)}</p>
      <p class="detail">
        触发点 {restTriggerLabel(restSuggestion.trigger_type)} · 连续专注
        {formatSeconds(restSuggestion.focus_seconds)} · 30 分钟切换 {restSuggestion.switch_count_30m} 次 · 偏差
        {formatDeviation(restSuggestion.deviation_ratio)}
      </p>
      <div class="actions">
        <button type="button" onclick={() => onRespondRestSuggestion(true)} disabled={!!currentAction}>
          接受建议
        </button>
        <button
          type="button"
          class="secondary"
          onclick={() => onRespondRestSuggestion(false)}
          disabled={!!currentAction}
        >
          忽略
        </button>
      </div>
    </section>
  {/if}
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

  .timer-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.2fr) minmax(280px, 0.8fr);
    gap: 0.95rem;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .side-column {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 0.95rem;
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
    gap: 0.55rem;
    align-items: center;
    text-align: center;
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

  .subtask-form {
    width: min(680px, 100%);
    margin-top: 0.7rem;
    border-top: 1px dashed #c1d4ee;
    padding-top: 0.74rem;
    display: flex;
    flex-direction: column;
    gap: 0.42rem;
    text-align: left;
  }

  .subtask-form label {
    font-size: 0.87rem;
    font-weight: 600;
    color: #23496f;
  }

  .subtask-row {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
  }

  .subtask-row input {
    flex: 1;
    min-width: 220px;
  }

  .side-panel {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .metrics-panel h2,
  .switch-panel h2 {
    margin: 0;
    font-size: 1rem;
    color: #173c68;
  }

  .metric {
    border: 1px solid #c7d8ee;
    background: #f7fbff;
    border-radius: 0.8rem;
    padding: 0.62rem 0.7rem;
  }

  .label {
    margin: 0;
    font-size: 0.81rem;
    color: #4d6f95;
  }

  .value {
    margin: 0.2rem 0 0;
    font-size: 1.1rem;
    font-weight: 700;
    color: #183d69;
  }

  .value.small {
    font-size: 0.95rem;
    line-height: 1.3;
  }

  .recent-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .recent-list li {
    display: block;
  }

  .recent-btn {
    text-align: left;
    border: 1px solid #c7d8f0;
    background: #f8fbff;
    color: #1e3e67;
    border-radius: 0.64rem;
    padding: 0.45rem 0.55rem;
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
    width: 100%;
    transition: border-color 120ms ease, background 120ms ease;
  }

  .recent-btn.selected {
    border-color: #1f4f92;
    background: #eaf2ff;
  }

  .recent-btn:hover,
  .recent-btn:focus-visible {
    border-color: #7598c8;
    background: #f1f7ff;
  }

  .recent-title-row {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    align-items: baseline;
  }

  .recent-title {
    font-size: 0.9rem;
    font-weight: 600;
    color: inherit;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .recent-action {
    font-size: 0.78rem;
    color: inherit;
    white-space: nowrap;
    opacity: 0.86;
  }

  .recent-meta {
    font-size: 0.8rem;
    color: #4b6e96;
  }

  .suggestion {
    border: 1px solid #a6c1e7;
    border-radius: 0.9rem;
    background: linear-gradient(180deg, #f5f9ff 0%, #e9f1ff 100%);
    padding: 0.8rem 0.9rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    flex-shrink: 0;
  }

  .suggestion .headline {
    margin: 0;
    font-size: 1rem;
    font-weight: 700;
    color: #1b436f;
  }

  .suggestion .detail {
    margin: 0;
    color: #486b92;
    font-size: 0.86rem;
    line-height: 1.34;
  }

  button,
  input {
    font: inherit;
  }

  button {
    border: 1px solid #2f629f;
    border-radius: 0.62rem;
    background: #2f629f;
    color: #fff;
    padding: 0.5rem 0.72rem;
    cursor: pointer;
  }

  button.secondary {
    border-color: #2f629f;
    background: #f2f7ff;
    color: #2f629f;
  }

  button.danger {
    background: #8b2a2a;
    border-color: #8b2a2a;
  }

  button:disabled,
  input:disabled {
    opacity: 0.56;
    cursor: not-allowed;
  }

  input {
    border-radius: 0.62rem;
    border: 1px solid #8cafd7;
    padding: 0.5rem 0.62rem;
    background: #fff;
  }

  .error {
    margin: 0;
    border-radius: 0.72rem;
    border: 1px solid #cb7474;
    background: #ffeded;
    color: #7f1a1a;
    padding: 0.56rem 0.7rem;
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

    .timer-grid {
      flex: 0 0 auto;
      min-height: fit-content;
      overflow: visible;
    }

    .side-column {
      overflow: visible;
      grid-template-rows: auto auto;
    }

    .focus-panel,
    .side-panel {
      overflow: visible;
    }
  }

  @media (max-width: 1080px) {
    .timer-grid {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 760px) {
    .page-head {
      flex-direction: column;
    }
  }
</style>

