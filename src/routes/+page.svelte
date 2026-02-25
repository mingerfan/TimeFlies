<script lang="ts">
  import {
    addTagToTask,
    createTask,
    getOverview,
    insertSubtaskAndStart,
    pauseTask,
    ping,
    removeTagFromTask,
    resumeTask,
    startTask,
    stopTask,
    type OverviewRange,
    type OverviewResponse,
    type TaskRecord,
    type TaskStatus,
  } from "$lib/api";

  type FlatTask = { task: TaskRecord; depth: number };

  let overview = $state<OverviewResponse | null>(null);
  let range = $state<OverviewRange>("week");
  let selectedTaskId = $state<string | null>(null);
  let loading = $state(false);
  let currentAction = $state("");
  let errorMessage = $state("");
  let pingResult = $state("");

  let createTitle = $state("");
  let createAsChild = $state(false);
  let subtaskTitle = $state("");
  let newTagName = $state("");

  const taskMap = $derived.by(() => {
    const map = new Map<string, TaskRecord>();
    for (const task of overview?.tasks ?? []) {
      map.set(task.id, task);
    }
    return map;
  });

  const childrenByParent = $derived.by(() => {
    const map = new Map<string, TaskRecord[]>();
    for (const task of overview?.tasks ?? []) {
      if (!task.parent_id) continue;
      const siblings = map.get(task.parent_id) ?? [];
      siblings.push(task);
      map.set(task.parent_id, siblings);
    }
    for (const siblings of map.values()) {
      siblings.sort((a, b) => a.created_at - b.created_at);
    }
    return map;
  });

  const rootTasks = $derived.by(() =>
    (overview?.tasks ?? [])
      .filter((task) => !task.parent_id)
      .sort((a, b) => a.created_at - b.created_at)
  );

  const flatTasks = $derived.by(() => {
    const rows: FlatTask[] = [];
    const visit = (task: TaskRecord, depth: number) => {
      rows.push({ task, depth });
      for (const child of childrenByParent.get(task.id) ?? []) {
        visit(child, depth + 1);
      }
    };
    for (const rootTask of rootTasks) {
      visit(rootTask, 0);
    }
    return rows;
  });

  const selectedTask = $derived.by(() =>
    selectedTaskId ? (taskMap.get(selectedTaskId) ?? null) : null
  );

  const topByExclusive = $derived.by(() =>
    [...(overview?.tasks ?? [])]
      .sort((a, b) => b.exclusive_seconds - a.exclusive_seconds)
      .slice(0, 5)
  );

  $effect(() => {
    const selectedRange = range;
    void refresh(selectedRange);
  });

  async function refresh(targetRange: OverviewRange = range) {
    loading = true;
    errorMessage = "";
    try {
      const snapshot = await getOverview(targetRange);
      overview = snapshot;
      if (selectedTaskId && !snapshot.tasks.some((task) => task.id === selectedTaskId)) {
        selectedTaskId = null;
      }
      if (!selectedTaskId && snapshot.tasks.length > 0) {
        selectedTaskId = snapshot.tasks[0].id;
      }
    } catch (error) {
      errorMessage = normalizeError(error);
    } finally {
      loading = false;
    }
  }

  async function runAction<T>(label: string, action: () => Promise<T>): Promise<T | null> {
    currentAction = label;
    errorMessage = "";
    try {
      const result = await action();
      await refresh();
      return result;
    } catch (error) {
      errorMessage = normalizeError(error);
      return null;
    } finally {
      currentAction = "";
    }
  }

  async function onCreateTask(event: SubmitEvent) {
    event.preventDefault();
    const title = createTitle.trim();
    if (!title) return;
    const parentId = createAsChild ? selectedTaskId : null;
    const taskId = await runAction("创建任务", () => createTask(title, parentId));
    createTitle = "";
    if (taskId) {
      selectedTaskId = taskId;
    }
  }

  async function onInsertSubtask(event: SubmitEvent) {
    event.preventDefault();
    if (!selectedTask || selectedTask.status !== "running") return;
    const title = subtaskTitle.trim();
    if (!title) return;
    const taskId = await runAction("插入子任务", () => insertSubtaskAndStart(selectedTask.id, title));
    subtaskTitle = "";
    if (taskId) {
      selectedTaskId = taskId;
    }
  }

  async function onAddTag(event: SubmitEvent) {
    event.preventDefault();
    if (!selectedTask) return;
    const tagName = newTagName.trim();
    if (!tagName) return;
    await runAction("添加标签", () => addTagToTask(selectedTask.id, tagName));
    newTagName = "";
  }

  async function onRemoveTag(tagName: string) {
    if (!selectedTask) return;
    await runAction("删除标签", () => removeTagFromTask(selectedTask.id, tagName));
  }

  async function testPing() {
    pingResult = "";
    const result = await runAction("连接测试", () => ping());
    if (result) {
      pingResult = result;
    }
  }

  function statusLabel(status: TaskStatus): string {
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

  function formatSeconds(input: number): string {
    const seconds = Math.max(0, Math.floor(input));
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    if (h > 0) return `${h}h ${m}m ${s}s`;
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
  }

  function formatDate(unixSeconds: number): string {
    return new Date(unixSeconds * 1000).toLocaleString();
  }

  function normalizeError(error: unknown): string {
    if (typeof error === "string") return error;
    if (error && typeof error === "object" && "message" in error) {
      return String((error as { message: string }).message);
    }
    return "发生未知错误";
  }
</script>

<main class="screen">
  <header class="header">
    <div>
      <p class="eyebrow">TimeFiles · MVP Slice</p>
      <h1>任务树计时台</h1>
      <p class="sub">按 PRD/MVP/ADR 落地：单活动上下文、事件日志、父子任务自动切换</p>
    </div>
    <div class="header-actions">
      <button class="secondary" type="button" onclick={testPing} disabled={!!currentAction}>
        {currentAction === "连接测试" ? "测试中..." : "Rust Ping"}
      </button>
      {#if pingResult}
        <span class="pill">IPC: {pingResult}</span>
      {/if}
    </div>
  </header>

  <section class="toolbar">
    <div class="range-switch">
      <button type="button" class:active={range === "all"} onclick={() => (range = "all")}>全部</button>
      <button type="button" class:active={range === "week"} onclick={() => (range = "week")}>近 7 天</button>
      <button type="button" class:active={range === "day"} onclick={() => (range = "day")}>近 24 小时</button>
    </div>

    <form class="create-form" onsubmit={onCreateTask}>
      <input
        type="text"
        placeholder="新任务标题"
        bind:value={createTitle}
        disabled={loading || !!currentAction}
      />
      <label>
        <input type="checkbox" bind:checked={createAsChild} disabled={!selectedTaskId} />
        作为当前选中任务的子任务
      </label>
      <button type="submit" disabled={loading || !!currentAction}>创建</button>
    </form>
  </section>

  {#if errorMessage}
    <p class="error">{errorMessage}</p>
  {/if}

  <section class="grid">
    <article class="panel">
      <div class="panel-title">
        <h2>任务树</h2>
        {#if overview}
          <span>{overview.tasks.length} 项</span>
        {/if}
      </div>

      {#if !overview || overview.tasks.length === 0}
        <p class="empty">当前暂无任务，先创建一个主任务开始计时。</p>
      {:else}
        <ul class="task-list">
          {#each flatTasks as item (item.task.id)}
            <li>
              <button
                class="task-row"
                class:selected={selectedTaskId === item.task.id}
                style={`--depth:${item.depth}`}
                type="button"
                onclick={() => (selectedTaskId = item.task.id)}
              >
                <span class="title">{item.task.title}</span>
                <span class="status">{statusLabel(item.task.status)}</span>
                <span class="time">
                  Ex: {formatSeconds(item.task.exclusive_seconds)} · In: {formatSeconds(item.task.inclusive_seconds)}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </article>

    <article class="panel">
      <div class="panel-title">
        <h2>任务详情</h2>
        {#if overview?.active_task_id}
          <span class="pill">活动任务: {overview.active_task_id.slice(0, 8)}</span>
        {/if}
      </div>

      {#if selectedTask}
        <div class="detail-block">
          <p class="detail-title">{selectedTask.title}</p>
          <p class="meta">创建于 {formatDate(selectedTask.created_at)} · 状态 {statusLabel(selectedTask.status)}</p>

          <div class="actions">
            <button
              type="button"
              onclick={() => runAction("开始任务", () => startTask(selectedTask.id))}
              disabled={!!currentAction || selectedTask.status === "running" || selectedTask.status === "paused"}
            >
              开始
            </button>
            <button
              type="button"
              onclick={() => runAction("暂停任务", () => pauseTask(selectedTask.id))}
              disabled={!!currentAction || selectedTask.status !== "running"}
            >
              暂停
            </button>
            <button
              type="button"
              onclick={() => runAction("恢复任务", () => resumeTask(selectedTask.id))}
              disabled={!!currentAction || selectedTask.status !== "paused"}
            >
              恢复
            </button>
            <button
              type="button"
              class="danger"
              onclick={() => runAction("停止任务", () => stopTask(selectedTask.id))}
              disabled={
                !!currentAction ||
                (selectedTask.status !== "running" && selectedTask.status !== "paused")
              }
            >
              停止
            </button>
          </div>
        </div>

        <div class="detail-block">
          <h3>标签</h3>
          <div class="tags">
            {#if selectedTask.tags.length === 0}
              <span class="muted">暂无标签</span>
            {:else}
              {#each selectedTask.tags as tag}
                <button class="tag" type="button" onclick={() => onRemoveTag(tag)} disabled={!!currentAction}>
                  {tag} ×
                </button>
              {/each}
            {/if}
          </div>
          <form class="inline-form" onsubmit={onAddTag}>
            <input type="text" placeholder="新标签" bind:value={newTagName} disabled={!!currentAction} />
            <button type="submit" disabled={!!currentAction}>添加</button>
          </form>
        </div>

        <div class="detail-block">
          <h3>运行中插入子任务</h3>
          <form class="inline-form" onsubmit={onInsertSubtask}>
            <input
              type="text"
              placeholder="子任务标题"
              bind:value={subtaskTitle}
              disabled={selectedTask.status !== "running" || !!currentAction}
            />
            <button type="submit" disabled={selectedTask.status !== "running" || !!currentAction}>
              插入并开始
            </button>
          </form>
          <p class="muted">仅在父任务处于进行中时可用；执行后父任务会自动暂停。</p>
        </div>
      {:else}
        <p class="empty">从左侧任务树选择一个任务以操作。</p>
      {/if}
    </article>

    <article class="panel">
      <div class="panel-title">
        <h2>统计摘要</h2>
        {#if overview}
          <span>{overview.range}</span>
        {/if}
      </div>

      {#if topByExclusive.length === 0}
        <p class="empty">暂无统计数据</p>
      {:else}
        <ol class="stats">
          {#each topByExclusive as task}
            <li>
              <span>{task.title}</span>
              <span>Ex {formatSeconds(task.exclusive_seconds)} / In {formatSeconds(task.inclusive_seconds)}</span>
            </li>
          {/each}
        </ol>
      {/if}
    </article>
  </section>
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
    color: #102039;
    background: radial-gradient(circle at 10% 10%, #f9f4e9 0%, #eaf0fa 45%, #d8e6f6 100%);
  }

  .screen {
    max-width: 1200px;
    margin: 0 auto;
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
  }

  .eyebrow {
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.78rem;
    color: #3f5d8b;
  }

  h1 {
    margin: 0.2rem 0;
    font-size: clamp(1.8rem, 2.2vw, 2.4rem);
  }

  .sub {
    margin: 0;
    color: #35506f;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  .toolbar {
    display: grid;
    grid-template-columns: 240px 1fr;
    gap: 1rem;
  }

  .range-switch {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    border-radius: 0.8rem;
    overflow: hidden;
    border: 1px solid #87a4cc;
    background: #dce7f8;
  }

  .range-switch button {
    border: none;
    background: transparent;
    padding: 0.7rem 0.4rem;
    cursor: pointer;
    color: #27456f;
  }

  .range-switch button.active {
    background: #1e4f91;
    color: #fff;
  }

  .create-form {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  .create-form label {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
    color: #38557a;
    font-size: 0.9rem;
  }

  .grid {
    display: grid;
    grid-template-columns: 1.1fr 1fr 0.9fr;
    gap: 1rem;
  }

  .panel {
    background: rgba(255, 255, 255, 0.88);
    border: 1px solid rgba(67, 103, 150, 0.3);
    border-radius: 1rem;
    padding: 0.95rem;
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
    min-height: 220px;
  }

  .panel-title {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.4rem;
  }

  h2 {
    margin: 0;
    font-size: 1rem;
  }

  h3 {
    margin: 0 0 0.4rem;
    font-size: 0.94rem;
  }

  .task-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    max-height: 510px;
    overflow: auto;
  }

  .task-row {
    width: 100%;
    text-align: left;
    border-radius: 0.7rem;
    border: 1px solid #cadbf4;
    padding: 0.6rem 0.7rem;
    padding-left: calc(0.75rem + (var(--depth) * 1.1rem));
    background: #f6faff;
    display: grid;
    gap: 0.2rem;
    cursor: pointer;
  }

  .task-row.selected {
    border-color: #244b81;
    background: #e4efff;
  }

  .task-row .title {
    font-weight: 600;
  }

  .task-row .status,
  .task-row .time {
    font-size: 0.82rem;
    color: #44678f;
  }

  .detail-title {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 700;
  }

  .detail-block {
    border-top: 1px dashed #bfd0ea;
    padding-top: 0.7rem;
  }

  .detail-block:first-of-type {
    border-top: none;
    padding-top: 0;
  }

  .meta,
  .muted {
    margin: 0;
    color: #4e6d90;
    font-size: 0.85rem;
  }

  .actions {
    margin-top: 0.7rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    min-height: 1.8rem;
  }

  .tag {
    border-radius: 999px;
    border: 1px solid #8cb0df;
    background: #e6f0ff;
    padding: 0.25rem 0.55rem;
    color: #1e4372;
    cursor: pointer;
  }

  .inline-form {
    margin-top: 0.55rem;
    display: flex;
    gap: 0.5rem;
  }

  .stats {
    margin: 0;
    padding-left: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  .stats li {
    display: flex;
    justify-content: space-between;
    gap: 0.8rem;
    font-size: 0.9rem;
  }

  button,
  input {
    font: inherit;
  }

  input {
    min-width: 8rem;
    border-radius: 0.6rem;
    border: 1px solid #8eafd6;
    padding: 0.52rem 0.62rem;
    background: #fff;
  }

  button {
    border: 1px solid #2f629f;
    border-radius: 0.6rem;
    background: #2f629f;
    color: #fff;
    padding: 0.52rem 0.78rem;
    cursor: pointer;
  }

  button.secondary {
    background: #f0f6ff;
    color: #2f629f;
    border-color: #2f629f;
  }

  button.danger {
    background: #8f2a2a;
    border-color: #8f2a2a;
  }

  button:disabled,
  input:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .pill {
    font-size: 0.78rem;
    padding: 0.24rem 0.52rem;
    border-radius: 999px;
    border: 1px solid #91afd8;
    background: #ecf4ff;
  }

  .error {
    margin: 0;
    border: 1px solid #c06161;
    background: #ffe9e9;
    color: #7c1717;
    border-radius: 0.7rem;
    padding: 0.55rem 0.7rem;
  }

  .empty {
    margin: 0;
    color: #526f95;
    font-size: 0.92rem;
  }

  @media (max-width: 1080px) {
    .grid {
      grid-template-columns: 1fr;
    }

    .toolbar {
      grid-template-columns: 1fr;
    }
  }
</style>
