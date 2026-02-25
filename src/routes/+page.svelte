<script lang="ts">
  import {
    addTagToTask,
    archiveTask,
    createTask,
    getOverview,
    insertSubtaskAndStart,
    pauseTask,
    removeTagFromTask,
    renameTask,
    reparentTask,
    resumeTask,
    startTask,
    stopTask,
    type OverviewResponse,
    type TaskRecord,
  } from "$lib/api";
  import {
    buildTaskChain,
    formatClock,
    formatDate,
    formatSeconds,
    normalizeError,
    statusLabel,
  } from "$lib/ui";
  import { onMount } from "svelte";

  type MiniNodeKind = "ancestor" | "current" | "child" | "root";
  type MiniNode = {
    task: TaskRecord;
    depth: number;
    kind: MiniNodeKind;
  };

  const ROOT_PARENT_VALUE = "__ROOT__";

  let overview = $state<OverviewResponse | null>(null);
  let selectedTaskId = $state<string | null>(null);
  let loading = $state(false);
  let currentAction = $state("");
  let errorMessage = $state("");
  let nowTs = $state(Math.floor(Date.now() / 1000));

  let quickAddTitle = $state("");
  let renameTitle = $state("");
  let reparentTarget = $state(ROOT_PARENT_VALUE);
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

  const selectedTask = $derived.by(() =>
    selectedTaskId ? (taskMap.get(selectedTaskId) ?? null) : null
  );

  const activeTask = $derived.by(() =>
    overview?.active_task_id ? (taskMap.get(overview.active_task_id) ?? null) : null
  );

  const selectedTaskPath = $derived.by(() =>
    buildTaskChain(selectedTask?.id ?? null, taskMap)
      .map((task) => task.title)
      .join(" / ")
  );

  const blockedParentIds = $derived.by(() => {
    const blocked = new Set<string>();
    if (!selectedTask) return blocked;
    const stack: string[] = [selectedTask.id];
    while (stack.length > 0) {
      const id = stack.pop();
      if (!id || blocked.has(id)) continue;
      blocked.add(id);
      for (const child of childrenByParent.get(id) ?? []) {
        stack.push(child.id);
      }
    }
    return blocked;
  });

  const reparentCandidates = $derived.by(() =>
    (overview?.tasks ?? [])
      .filter((task) => !blockedParentIds.has(task.id))
      .sort((a, b) => a.created_at - b.created_at)
  );

  const miniNodes = $derived.by(() => {
    const nodes: MiniNode[] = [];
    if (selectedTask) {
      const chain = buildTaskChain(selectedTask.id, taskMap);
      chain.forEach((task, index) => {
        nodes.push({
          task,
          depth: index,
          kind: index === chain.length - 1 ? "current" : "ancestor",
        });
      });

      const children = (childrenByParent.get(selectedTask.id) ?? []).slice(0, 10);
      for (const child of children) {
        nodes.push({
          task: child,
          depth: chain.length,
          kind: "child",
        });
      }
      return nodes;
    }

    for (const root of rootTasks.slice(0, 12)) {
      nodes.push({
        task: root,
        depth: 0,
        kind: "root",
      });
    }
    return nodes;
  });

  const selectedElapsedSeconds = $derived.by(() => {
    const task = selectedTask;
    if (!task) return 0;
    const delta =
      activeTask && activeTask.id === task.id && activeTask.status === "running" && overview
        ? Math.max(0, nowTs - overview.generated_at)
        : 0;
    return task.exclusive_seconds + delta;
  });

  const activeElapsedSeconds = $derived.by(() => {
    if (!activeTask) return 0;
    if (activeTask.status !== "running" || !overview) {
      return activeTask.exclusive_seconds;
    }
    return activeTask.exclusive_seconds + Math.max(0, nowTs - overview.generated_at);
  });

  onMount(() => {
    void refresh();
    const ticker = window.setInterval(() => {
      nowTs = Math.floor(Date.now() / 1000);
    }, 1_000);
    const poller = window.setInterval(() => void refresh(), 30_000);
    return () => {
      window.clearInterval(ticker);
      window.clearInterval(poller);
    };
  });

  $effect(() => {
    const task = selectedTask;
    if (!task) {
      renameTitle = "";
      reparentTarget = ROOT_PARENT_VALUE;
      return;
    }
    renameTitle = task.title;
    reparentTarget = task.parent_id ?? ROOT_PARENT_VALUE;
  });

  async function refresh() {
    loading = true;
    errorMessage = "";
    try {
      const snapshot = await getOverview("week");
      overview = snapshot;
      if (selectedTaskId && !snapshot.tasks.some((task) => task.id === selectedTaskId)) {
        selectedTaskId = null;
      }
      if (!selectedTaskId) {
        selectedTaskId = snapshot.active_task_id ?? snapshot.tasks[0]?.id ?? null;
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

  async function ensureSwitchFromActive(targetTaskId: string): Promise<boolean> {
    const active = activeTask;
    if (!active || active.status !== "running" || active.id === targetTaskId) {
      return true;
    }
    const paused = await runAction("暂停当前任务", () => pauseTask(active.id));
    return paused !== null;
  }

  async function onPrimaryToggle() {
    if (!selectedTask) return;
    const task = selectedTask;
    if (task.status === "running") {
      await runAction("暂停任务", () => pauseTask(task.id));
      return;
    }

    if (!(await ensureSwitchFromActive(task.id))) return;
    if (task.status === "paused") {
      await runAction("恢复任务", () => resumeTask(task.id));
      return;
    }
    await runAction("开始任务", () => startTask(task.id));
  }

  async function onStopSelected() {
    if (!selectedTask) return;
    if (selectedTask.status !== "running" && selectedTask.status !== "paused") return;
    await runAction("停止任务", () => stopTask(selectedTask.id));
  }

  function nodeActionSymbol(task: TaskRecord): string {
    return task.status === "running" ? "⏸" : "▶";
  }

  function nodeActionLabel(task: TaskRecord): string {
    if (task.status === "running") return "暂停任务";
    if (task.status === "paused") return "恢复任务";
    return "开始任务";
  }

  async function onMiniNodeToggle(event: MouseEvent, task: TaskRecord) {
    event.stopPropagation();
    selectedTaskId = task.id;

    if (task.status === "running") {
      await runAction("暂停任务", () => pauseTask(task.id));
      return;
    }

    if (!(await ensureSwitchFromActive(task.id))) return;
    if (task.status === "paused") {
      await runAction("恢复任务", () => resumeTask(task.id));
      return;
    }

    await runAction("开始任务", () => startTask(task.id));
  }

  async function onCreateTask(event: SubmitEvent) {
    event.preventDefault();
    const title = quickAddTitle.trim();
    if (!title) return;

    const parentId = selectedTaskId;
    const createdTaskId = await runAction("快速创建任务", () => createTask(title, parentId));
    if (!createdTaskId) return;

    selectedTaskId = createdTaskId;
    quickAddTitle = "";
  }

  async function onCreateTaskAndStart() {
    const title = quickAddTitle.trim();
    if (!title) return;

    if (selectedTask?.status === "running") {
      const childId = await runAction("插入子任务", () =>
        insertSubtaskAndStart(selectedTask.id, title)
      );
      if (!childId) return;
      selectedTaskId = childId;
      quickAddTitle = "";
      return;
    }

    const parentId = selectedTaskId;
    const createdTaskId = await runAction("快速创建任务", () => createTask(title, parentId));
    if (!createdTaskId) return;
    selectedTaskId = createdTaskId;
    quickAddTitle = "";

    if (!(await ensureSwitchFromActive(createdTaskId))) return;
    await runAction("开始任务", () => startTask(createdTaskId));
  }

  function onQuickAddKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      quickAddTitle = "";
      return;
    }
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void onCreateTaskAndStart();
    }
  }

  async function onRenameTask(event: SubmitEvent) {
    event.preventDefault();
    if (!selectedTask) return;
    const title = renameTitle.trim();
    if (!title) return;
    await runAction("重命名任务", () => renameTask(selectedTask.id, title));
  }

  async function onReparentTask(event: SubmitEvent) {
    event.preventDefault();
    if (!selectedTask) return;
    const targetParentId = reparentTarget === ROOT_PARENT_VALUE ? null : reparentTarget;
    if (targetParentId === selectedTask.parent_id) return;
    await runAction("调整父任务", () => reparentTask(selectedTask.id, targetParentId));
  }

  async function onArchiveTask() {
    if (!selectedTask) return;
    const confirmed = window.confirm(`确认归档任务「${selectedTask.title}」及其全部子任务吗？`);
    if (!confirmed) return;
    await runAction("归档任务", () => archiveTask(selectedTask.id));
  }

  async function onInsertSubtask(event: SubmitEvent) {
    event.preventDefault();
    if (!selectedTask || selectedTask.status !== "running") return;
    const title = subtaskTitle.trim();
    if (!title) return;
    const taskId = await runAction("插入子任务", () => insertSubtaskAndStart(selectedTask.id, title));
    if (!taskId) return;
    subtaskTitle = "";
    selectedTaskId = taskId;
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
</script>

<main class="detail-screen">
  <header class="hero">
    <div>
      <p class="eyebrow">主工作台</p>
      {#if selectedTask}
        <h1>{selectedTask.title}</h1>
        <p class="hero-meta">路径 {selectedTaskPath || "-"} · {statusLabel(selectedTask.status)}</p>
        <p class="hero-time">当前任务已用 {formatClock(selectedElapsedSeconds)}</p>
      {:else}
        <h1>任务详情</h1>
        <p class="hero-meta">请在右侧 mini 任务树里选择一个任务</p>
      {/if}
    </div>
    <div class="hero-actions">
      <a href="/tree" class="ghost-link">打开任务树工作区</a>
      <button type="button" class="secondary" onclick={refresh} disabled={loading || !!currentAction}>
        {loading ? "刷新中..." : "刷新"}
      </button>
      <button type="button" onclick={onPrimaryToggle} disabled={!selectedTask || !!currentAction}>
        {selectedTask?.status === "running"
          ? "暂停"
          : selectedTask?.status === "paused"
            ? "恢复"
            : "开始"}
      </button>
      <button
        type="button"
        class="danger"
        onclick={onStopSelected}
        disabled={!selectedTask || !!currentAction}
      >
        停止
      </button>
    </div>
  </header>

  {#if errorMessage}
    <p class="error">{errorMessage}</p>
  {/if}

  <section class="content-grid">
    <article class="panel detail-main">
      {#if selectedTask}
        <section class="detail-top">
          <p class="detail-title">{selectedTask.title}</p>
          <p class="meta">
            创建于 {formatDate(selectedTask.created_at)} · Ex {formatSeconds(selectedTask.exclusive_seconds)} · In
            {formatSeconds(selectedTask.inclusive_seconds)}
          </p>
        </section>

        <section class="detail-block">
          <h2>结构与基础操作</h2>
          <form class="inline-form" onsubmit={onRenameTask}>
            <input type="text" bind:value={renameTitle} placeholder="任务名称" disabled={!!currentAction} />
            <button type="submit" disabled={!!currentAction || !renameTitle.trim()}>重命名</button>
          </form>

          <form class="inline-form" onsubmit={onReparentTask}>
            <select bind:value={reparentTarget} disabled={!!currentAction}>
              <option value={ROOT_PARENT_VALUE}>设为根任务</option>
              {#each reparentCandidates as candidate (candidate.id)}
                <option value={candidate.id}>{candidate.title}</option>
              {/each}
            </select>
            <button type="submit" disabled={!!currentAction}>调整父任务</button>
          </form>

          <button type="button" class="subtle-danger" onclick={onArchiveTask} disabled={!!currentAction}>
            归档当前任务子树
          </button>
        </section>

        <section class="detail-block">
          <h2>标签</h2>
          <div class="tags">
            {#if selectedTask.tags.length === 0}
              <span class="muted">暂无标签</span>
            {:else}
              {#each selectedTask.tags as tag}
                <button type="button" class="tag" onclick={() => onRemoveTag(tag)} disabled={!!currentAction}>
                  #{tag} ×
                </button>
              {/each}
            {/if}
          </div>
          <form class="inline-form" onsubmit={onAddTag}>
            <input type="text" bind:value={newTagName} placeholder="新标签" disabled={!!currentAction} />
            <button type="submit" disabled={!!currentAction || !newTagName.trim()}>添加标签</button>
          </form>
        </section>

        <section class="detail-block">
          <h2>运行中插入子任务</h2>
          <form class="inline-form" onsubmit={onInsertSubtask}>
            <input
              type="text"
              bind:value={subtaskTitle}
              placeholder="子任务标题"
              disabled={selectedTask.status !== "running" || !!currentAction}
            />
            <button type="submit" disabled={selectedTask.status !== "running" || !!currentAction || !subtaskTitle.trim()}>
              插入并开始
            </button>
          </form>
        </section>
      {:else}
        <section class="detail-top">
          <p class="empty">暂无选中任务，可在右侧 mini 树选择或新建。</p>
        </section>
      {/if}

      <section class="quick-add">
        <form class="quick-form" onsubmit={onCreateTask}>
          <label for="quick-add-input">快速添加任务</label>
          <input
            id="quick-add-input"
            type="text"
            bind:value={quickAddTitle}
            onkeydown={onQuickAddKeydown}
            placeholder={selectedTaskId ? "默认创建为当前任务子任务" : "默认创建根任务"}
            disabled={loading || !!currentAction}
          />
          <div class="quick-actions">
            <button type="submit" disabled={loading || !!currentAction || !quickAddTitle.trim()}>
              创建
            </button>
            <button
              type="button"
              class="secondary"
              onclick={onCreateTaskAndStart}
              disabled={loading || !!currentAction || !quickAddTitle.trim()}
            >
              创建并开始
            </button>
          </div>
          <p class="hint">Enter 创建，Ctrl+Enter 创建并开始，Esc 清空。</p>
        </form>
      </section>
    </article>

    <aside class="side-rail">
      <article class="panel mini-timer">
        <h2>Mini 计时器</h2>
        {#if activeTask}
          <p class="mini-title">{activeTask.title}</p>
          <p class="mini-meta">{statusLabel(activeTask.status)}</p>
          <p class="mini-clock">{formatClock(activeElapsedSeconds)}</p>
        {:else}
          <p class="empty">暂无进行中的任务</p>
        {/if}
      </article>

      <article class="panel mini-tree">
        <div class="mini-tree-head">
          <h2>Mini 任务树</h2>
          <a href="/tree">完整树视图</a>
        </div>
        {#if miniNodes.length === 0}
          <p class="empty">暂无任务</p>
        {:else}
          <ul class="mini-list">
            {#each miniNodes as node (`${node.kind}-${node.task.id}`)}
              <li>
                <div
                  class="mini-node-row"
                  class:current={node.kind === "current"}
                  style={`--depth:${node.depth}`}
                >
                  <button
                    type="button"
                    class="mini-node-main"
                    onclick={() => (selectedTaskId = node.task.id)}
                    title={`${node.task.title}\n${statusLabel(node.task.status)} · Ex ${formatSeconds(node.task.exclusive_seconds)}`}
                  >
                    <span class="mini-node-title">{node.task.title}</span>
                    <span class="mini-node-sub">{statusLabel(node.task.status)}</span>
                  </button>
                  <button
                    type="button"
                    class="mini-node-action"
                    onclick={(event) => onMiniNodeToggle(event, node.task)}
                    disabled={!!currentAction}
                    title={nodeActionLabel(node.task)}
                  >
                    {nodeActionSymbol(node.task)}
                  </button>
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </article>
    </aside>
  </section>
</main>

<style>
  .detail-screen {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  .hero {
    background: rgba(255, 255, 255, 0.88);
    border: 1px solid rgba(65, 97, 143, 0.25);
    border-radius: 1rem;
    padding: 1rem 1.1rem;
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    flex-shrink: 0;
  }

  .eyebrow {
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.72rem;
    color: #4f688d;
  }

  h1 {
    margin: 0.2rem 0 0.1rem;
    font-size: clamp(1.6rem, 2.4vw, 2.3rem);
    color: #102b4a;
  }

  .hero-meta {
    margin: 0;
    color: #415d82;
    font-size: 0.9rem;
  }

  .hero-time {
    margin: 0.25rem 0 0;
    font-family: "IBM Plex Mono", "Cascadia Mono", monospace;
    font-size: 1.05rem;
    color: #143d67;
  }

  .hero-actions {
    display: flex;
    align-items: flex-start;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .ghost-link {
    text-decoration: none;
    color: #2d4f7d;
    border: 1px solid #89a9d4;
    border-radius: 0.62rem;
    padding: 0.46rem 0.66rem;
    background: #eff6ff;
  }

  .content-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 340px;
    gap: 1rem;
    align-items: start;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .panel {
    background: rgba(255, 255, 255, 0.88);
    border: 1px solid rgba(65, 97, 143, 0.28);
    border-radius: 1rem;
    padding: 0.9rem;
  }

  .detail-main {
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .detail-top,
  .detail-block,
  .quick-add {
    border-top: 1px dashed #bfd2ef;
    padding-top: 0.72rem;
  }

  .detail-top {
    border-top: none;
    padding-top: 0;
  }

  .detail-title {
    margin: 0;
    font-size: 1.14rem;
    font-weight: 700;
    color: #0f2f54;
  }

  h2 {
    margin: 0 0 0.48rem;
    font-size: 0.96rem;
  }

  .meta,
  .muted,
  .empty {
    margin: 0;
    color: #4d6c91;
    font-size: 0.86rem;
    line-height: 1.35;
  }

  .inline-form {
    margin-top: 0.52rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.44rem;
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 0.42rem;
  }

  .tag {
    border-radius: 999px;
    border: 1px solid #a3bedf;
    background: #edf4ff;
    color: #2b547f;
    padding: 0.2rem 0.52rem;
  }

  .quick-form {
    display: flex;
    flex-direction: column;
    gap: 0.52rem;
  }

  .quick-form label {
    font-size: 0.9rem;
    font-weight: 600;
    color: #1f4672;
  }

  .quick-actions {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
  }

  .hint {
    margin: 0;
    font-size: 0.77rem;
    color: #4c6f96;
  }

  .side-rail {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .mini-timer h2,
  .mini-tree h2 {
    margin: 0 0 0.4rem;
    font-size: 0.95rem;
  }

  .mini-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 700;
    color: #173b68;
    line-height: 1.3;
  }

  .mini-clock {
    margin: 0.2rem 0 0;
    font-family: "IBM Plex Mono", "Cascadia Mono", monospace;
    font-size: 1.05rem;
    color: #174371;
  }

  .mini-meta {
    margin: 0;
    color: #4b6b92;
    font-size: 0.82rem;
  }

  .mini-tree-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.25rem;
  }

  .mini-tree-head a {
    font-size: 0.8rem;
    color: #2f5688;
    text-decoration: none;
  }

  .mini-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    max-height: 340px;
    overflow: auto;
  }

  .mini-node-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.24rem;
    align-items: center;
    padding-left: calc(var(--depth) * 0.8rem);
  }

  .mini-node-main {
    width: 100%;
    text-align: left;
    border: none;
    border-radius: 0.36rem;
    background: transparent;
    padding: 0.35rem 0.42rem;
    color: #2f3437;
  }

  .mini-node-row:hover .mini-node-main,
  .mini-node-row:focus-within .mini-node-main {
    background: #f3f4f6;
  }

  .mini-node-row.current .mini-node-main {
    background: #e7edf6;
    color: #122a46;
  }

  .mini-node-title {
    display: block;
    font-size: 0.86rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mini-node-sub {
    display: block;
    margin-top: 0.06rem;
    font-size: 0.72rem;
    color: #6b7280;
  }

  .mini-node-action {
    width: 1.34rem;
    height: 1.34rem;
    border: 1px solid #d0d7de;
    border-radius: 0.3rem;
    background: #fff;
    color: #4b5563;
    padding: 0;
    display: grid;
    place-items: center;
    font-size: 0.7rem;
    line-height: 1;
    opacity: 0;
    pointer-events: none;
    transition: opacity 120ms ease;
  }

  .mini-node-row:hover .mini-node-action,
  .mini-node-row:focus-within .mini-node-action {
    opacity: 1;
    pointer-events: auto;
  }

  .mini-node-action:disabled {
    opacity: 0.5;
    pointer-events: none;
  }

  button,
  input,
  select {
    font: inherit;
  }

  input,
  select {
    min-width: 8rem;
    border-radius: 0.62rem;
    border: 1px solid #8cafd7;
    padding: 0.5rem 0.62rem;
    background: #fff;
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

  button.subtle-danger {
    border-color: #c87373;
    background: #ffecec;
    color: #7f1f1f;
  }

  button:disabled,
  input:disabled,
  select:disabled {
    opacity: 0.56;
    cursor: not-allowed;
  }

  .error {
    margin: 0;
    border-radius: 0.72rem;
    border: 1px solid #cb7474;
    background: #ffeded;
    color: #7f1a1a;
    padding: 0.56rem 0.7rem;
  }

  @media (max-height: 700px) {
    .detail-screen {
      height: auto;
      min-height: 100%;
      overflow: visible;
    }

    .content-grid {
      flex: 0 0 auto;
      min-height: fit-content;
      overflow: visible;
    }

    .detail-main,
    .side-rail {
      overflow: visible;
    }
  }

  @media (max-width: 1180px) {
    .content-grid {
      grid-template-columns: 1fr;
      overflow: auto;
    }

    .hero {
      flex-direction: column;
    }
  }
</style>
