<script lang="ts">
  import {
    createTask,
    getOverview,
    insertSubtaskAndStart,
    pauseTask,
    resumeTask,
    startTask,
    stopTask,
    type OverviewRange,
    type OverviewResponse,
    type TaskRecord,
  } from "$lib/api";
  import { buildTaskChain, formatSeconds, normalizeError, statusLabel } from "$lib/ui";

  type VisibleTaskRow = {
    task: TaskRecord;
    depth: number;
    hasChildren: boolean;
  };

  let overview = $state<OverviewResponse | null>(null);
  let range = $state<OverviewRange>("week");
  let selectedTaskId = $state<string | null>(null);
  let expandedTaskIds = $state<Set<string>>(new Set());
  let hasInitializedTreeExpansion = $state(false);
  let loading = $state(false);
  let currentAction = $state("");
  let errorMessage = $state("");
  let treeQuery = $state("");
  let quickAddTitle = $state("");
  let quickAddAsChild = $state(true);

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

  const activeTaskId = $derived.by(() => overview?.active_task_id ?? null);

  const activePathIds = $derived.by(() => {
    const chain = buildTaskChain(activeTaskId, taskMap);
    return new Set(chain.map((task) => task.id));
  });

  const selectedTaskPath = $derived.by(() =>
    buildTaskChain(selectedTask?.id ?? null, taskMap)
      .map((task) => task.title)
      .join(" / ")
  );

  const normalizedTreeQuery = $derived.by(() => treeQuery.trim().toLowerCase());

  const visibleRows = $derived.by(() =>
    flattenTaskRows(rootTasks, childrenByParent, expandedTaskIds, true, null)
  );

  const matchedTaskIds = $derived.by(() => {
    const query = normalizedTreeQuery;
    if (!query) return null;
    const ids = new Set<string>();
    for (const task of overview?.tasks ?? []) {
      if (!task.title.toLowerCase().includes(query)) continue;
      let cursor: string | null = task.id;
      while (cursor) {
        if (ids.has(cursor)) break;
        ids.add(cursor);
        cursor = taskMap.get(cursor)?.parent_id ?? null;
      }
    }
    return ids;
  });

  const displayRows = $derived.by(() =>
    matchedTaskIds
      ? flattenTaskRows(rootTasks, childrenByParent, expandedTaskIds, false, matchedTaskIds)
      : visibleRows
  );

  $effect(() => {
    const selectedRange = range;
    void refresh(selectedRange);
  });

  $effect(() => {
    const tasks = overview?.tasks ?? [];
    if (tasks.length === 0) {
      if (expandedTaskIds.size > 0) {
        expandedTaskIds = new Set();
      }
      hasInitializedTreeExpansion = false;
      return;
    }

    if (!hasInitializedTreeExpansion) {
      const initial = new Set<string>();
      for (const root of rootTasks) {
        initial.add(root.id);
      }
      for (const task of buildTaskChain(activeTaskId, taskMap)) {
        initial.add(task.id);
      }
      expandedTaskIds = initial;
      hasInitializedTreeExpansion = true;
      return;
    }

    const validIds = new Set(tasks.map((task) => task.id));
    const pruned = new Set([...expandedTaskIds].filter((id) => validIds.has(id)));
    if (!areSetsEqual(pruned, expandedTaskIds)) {
      expandedTaskIds = pruned;
    }
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
        selectedTaskId = snapshot.active_task_id ?? snapshot.tasks[0].id;
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

  function toggleExpand(taskId: string) {
    const next = new Set(expandedTaskIds);
    if (next.has(taskId)) {
      next.delete(taskId);
    } else {
      next.add(taskId);
    }
    expandedTaskIds = next;
  }

  function expandAll() {
    expandedTaskIds = new Set((overview?.tasks ?? []).map((task) => task.id));
  }

  function collapseAll() {
    expandedTaskIds = new Set();
  }

  async function ensureSwitchFromActive(targetTaskId: string): Promise<boolean> {
    const active = activeTaskId ? taskMap.get(activeTaskId) ?? null : null;
    if (!active || active.id === targetTaskId || active.status !== "running") {
      return true;
    }
    const paused = await runAction("暂停当前任务", () => pauseTask(active.id));
    return paused !== null;
  }

  async function onTaskQuickToggle(event: MouseEvent, task: TaskRecord) {
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

  async function onStartSelected() {
    if (!selectedTask) return;
    if (!(await ensureSwitchFromActive(selectedTask.id))) return;
    if (selectedTask.status === "paused") {
      await runAction("恢复任务", () => resumeTask(selectedTask.id));
      return;
    }
    if (selectedTask.status === "running") return;
    await runAction("开始任务", () => startTask(selectedTask.id));
  }

  async function onPauseSelected() {
    if (!selectedTask || selectedTask.status !== "running") return;
    await runAction("暂停任务", () => pauseTask(selectedTask.id));
  }

  async function onStopSelected() {
    if (!selectedTask) return;
    if (selectedTask.status !== "running" && selectedTask.status !== "paused") return;
    await runAction("停止任务", () => stopTask(selectedTask.id));
  }

  async function createQuickTask(startAfterCreate: boolean) {
    const title = quickAddTitle.trim();
    if (!title) return;

    const parentId = quickAddAsChild && selectedTaskId ? selectedTaskId : null;
    if (
      startAfterCreate &&
      parentId &&
      selectedTask &&
      selectedTask.id === parentId &&
      selectedTask.status === "running"
    ) {
      const childId = await runAction("插入子任务", () => insertSubtaskAndStart(parentId, title));
      if (childId) {
        selectedTaskId = childId;
        quickAddTitle = "";
      }
      return;
    }

    const createdTaskId = await runAction("创建任务", () => createTask(title, parentId));
    if (!createdTaskId) return;
    selectedTaskId = createdTaskId;
    quickAddTitle = "";

    if (!startAfterCreate) return;
    if (!(await ensureSwitchFromActive(createdTaskId))) return;
    await runAction("开始任务", () => startTask(createdTaskId));
  }

  async function onCreateTask(event: SubmitEvent) {
    event.preventDefault();
    await createQuickTask(false);
  }

  async function onCreateTaskAndStart() {
    await createQuickTask(true);
  }

  function onQuickAddKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      quickAddTitle = "";
      return;
    }
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void createQuickTask(true);
    }
  }

  function onTaskRowKeydown(event: KeyboardEvent, row: VisibleTaskRow) {
    const currentIndex = displayRows.findIndex((item) => item.task.id === row.task.id);
    if (currentIndex < 0) return;

    if (event.key === "ArrowDown") {
      event.preventDefault();
      const next = displayRows[currentIndex + 1];
      if (next) selectedTaskId = next.task.id;
      return;
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      const previous = displayRows[currentIndex - 1];
      if (previous) selectedTaskId = previous.task.id;
      return;
    }

    if (event.key === "ArrowRight") {
      event.preventDefault();
      if (row.hasChildren && !expandedTaskIds.has(row.task.id)) {
        toggleExpand(row.task.id);
        return;
      }
      const next = displayRows[currentIndex + 1];
      if (next && next.depth > row.depth) selectedTaskId = next.task.id;
      return;
    }

    if (event.key === "ArrowLeft") {
      event.preventDefault();
      if (row.hasChildren && expandedTaskIds.has(row.task.id)) {
        toggleExpand(row.task.id);
        return;
      }
      if (row.task.parent_id) selectedTaskId = row.task.parent_id;
      return;
    }
  }

  function taskQuickActionLabel(task: TaskRecord): string {
    if (task.status === "running") return "暂停任务";
    if (task.status === "paused") return "恢复任务";
    return "开始任务";
  }

  function taskQuickActionSymbol(task: TaskRecord): string {
    if (task.status === "running") return "⏸";
    return "▶";
  }

  function resetTreeQuery() {
    treeQuery = "";
  }

  function areSetsEqual(left: Set<string>, right: Set<string>) {
    if (left.size !== right.size) return false;
    for (const id of left) {
      if (!right.has(id)) return false;
    }
    return true;
  }

  function flattenTaskRows(
    roots: TaskRecord[],
    childrenMap: Map<string, TaskRecord[]>,
    expandedIds: Set<string>,
    respectExpanded: boolean,
    includeIds: Set<string> | null
  ): VisibleTaskRow[] {
    const rows: VisibleTaskRow[] = [];
    const visited = new Set<string>();

    const visit = (task: TaskRecord, depth: number) => {
      if (visited.has(task.id)) return;
      visited.add(task.id);
      const children = childrenMap.get(task.id) ?? [];
      if (!includeIds || includeIds.has(task.id)) {
        rows.push({
          task,
          depth,
          hasChildren: children.length > 0,
        });
      }
      if (respectExpanded && !expandedIds.has(task.id)) return;
      for (const child of children) {
        visit(child, depth + 1);
      }
    };

    for (const root of roots) visit(root, 0);
    return rows;
  }
</script>

<main class="tree-screen">
  <header class="page-head">
    <div>
      <p class="eyebrow">任务树工作区</p>
      <h1>任务树</h1>
      <p class="sub">仅保留树操作，详情编辑请到任务详情页</p>
    </div>

    <div class="range-switch">
      <button type="button" class:active={range === "all"} onclick={() => (range = "all")}>全部</button>
      <button type="button" class:active={range === "week"} onclick={() => (range = "week")}>近 7 天</button>
      <button type="button" class:active={range === "day"} onclick={() => (range = "day")}>近 24 小时</button>
    </div>
  </header>

  {#if errorMessage}
    <p class="error">{errorMessage}</p>
  {/if}

  <section class="panel selection-strip">
    <div>
      {#if selectedTask}
        <p class="selected-title">{selectedTask.title}</p>
        <p class="selected-meta">
          {statusLabel(selectedTask.status)} · {selectedTaskPath} · Ex {formatSeconds(selectedTask.exclusive_seconds)}
        </p>
      {:else}
        <p class="selected-title">未选中任务</p>
        <p class="selected-meta">在下方任务树选择一个任务</p>
      {/if}
    </div>
    <div class="selection-actions">
      <a href="/" class="ghost-link">打开任务详情页</a>
      <button type="button" onclick={onStartSelected} disabled={!selectedTask || !!currentAction}>开始/恢复</button>
      <button type="button" class="secondary" onclick={onPauseSelected} disabled={!selectedTask || !!currentAction}>
        暂停
      </button>
      <button type="button" class="danger" onclick={onStopSelected} disabled={!selectedTask || !!currentAction}>
        停止
      </button>
    </div>
  </section>

  <section class="panel tree-panel">
    <div class="panel-head">
      <h2>任务树</h2>
      <span>{overview?.tasks.length ?? 0} 项</span>
    </div>

    <form class="quick-row" onsubmit={onCreateTask}>
      <input
        type="text"
        placeholder="快速创建任务"
        bind:value={quickAddTitle}
        onkeydown={onQuickAddKeydown}
        disabled={loading || !!currentAction}
      />
      <label class="child-toggle">
        <input type="checkbox" bind:checked={quickAddAsChild} disabled={!selectedTaskId} />
        作为选中任务子任务
      </label>
      <button type="submit" disabled={loading || !!currentAction || !quickAddTitle.trim()}>创建</button>
      <button
        type="button"
        class="secondary"
        onclick={onCreateTaskAndStart}
        disabled={loading || !!currentAction || !quickAddTitle.trim()}
      >
        创建并开始
      </button>
    </form>

    <div class="tree-toolbar">
      <input
        type="text"
        placeholder="搜索任务..."
        bind:value={treeQuery}
        disabled={!overview?.tasks.length}
        aria-label="搜索任务"
      />
      <button type="button" class="subtle" onclick={expandAll} disabled={!overview?.tasks.length}>展开</button>
      <button type="button" class="subtle" onclick={collapseAll} disabled={!overview?.tasks.length}>收起</button>
      <button
        type="button"
        class="subtle"
        onclick={resetTreeQuery}
        disabled={!treeQuery.trim() || !overview?.tasks.length}
      >
        清除
      </button>
    </div>

    {#if !overview || overview.tasks.length === 0}
      <p class="empty">当前暂无任务。</p>
    {:else if displayRows.length === 0}
      <p class="empty">没有匹配“{treeQuery}”的任务。</p>
    {:else}
      <div class="tree-frame">
        <ul class="tree-list" role="tree" aria-label="任务树">
          {#each displayRows as row (row.task.id)}
            <li class="tree-item">
              <div
                class="tree-row"
                class:selected={selectedTaskId === row.task.id}
                class:active-ancestor={activePathIds.has(row.task.id) && activeTaskId !== row.task.id}
                class:active-leaf={activeTaskId === row.task.id}
                style={`--depth:${row.depth}`}
              >
                {#if row.hasChildren}
                  <button
                    type="button"
                    class="toggle"
                    class:expanded={expandedTaskIds.has(row.task.id)}
                    onclick={() => toggleExpand(row.task.id)}
                    aria-label={expandedTaskIds.has(row.task.id) ? "收起子任务" : "展开子任务"}
                  >
                    <span class="chevron" aria-hidden="true">▸</span>
                  </button>
                {:else}
                  <span class="toggle placeholder" aria-hidden="true"></span>
                {/if}

                <button
                  type="button"
                  class="row-main"
                  onclick={() => (selectedTaskId = row.task.id)}
                  onkeydown={(event) => onTaskRowKeydown(event, row)}
                  title={`${row.task.title}\n${statusLabel(row.task.status)} · Ex ${formatSeconds(row.task.exclusive_seconds)} · In ${formatSeconds(row.task.inclusive_seconds)}`}
                >
                  <span class="title">{row.task.title}</span>
                </button>

                <button
                  type="button"
                  class="row-quick"
                  onclick={(event) => onTaskQuickToggle(event, row.task)}
                  disabled={!!currentAction}
                  title={taskQuickActionLabel(row.task)}
                >
                  {taskQuickActionSymbol(row.task)}
                </button>
              </div>
            </li>
          {/each}
        </ul>
      </div>
    {/if}
  </section>
</main>

<style>
  .tree-screen {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
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
    margin: 0.14rem 0;
    font-size: clamp(1.6rem, 2vw, 2rem);
  }

  .sub {
    margin: 0;
    color: #3c5a80;
    font-size: 0.92rem;
  }

  .range-switch {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    border: 1px solid #89a9d4;
    border-radius: 0.78rem;
    overflow: hidden;
    min-width: 290px;
    background: #deebff;
  }

  .range-switch button {
    border: none;
    background: transparent;
    color: #25446f;
    padding: 0.54rem 0.45rem;
    cursor: pointer;
  }

  .range-switch button.active {
    background: #1f4f92;
    color: #fff;
  }

  .panel {
    background: rgba(255, 255, 255, 0.9);
    border: 1px solid rgba(65, 97, 143, 0.28);
    border-radius: 1rem;
    padding: 0.9rem;
  }

  .selection-strip {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
    flex-shrink: 0;
  }

  .selected-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 700;
    color: #112d4e;
    line-height: 1.3;
  }

  .selected-meta {
    margin: 0.2rem 0 0;
    color: #4f6f95;
    font-size: 0.84rem;
    line-height: 1.35;
  }

  .selection-actions {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .ghost-link {
    text-decoration: none;
    color: #2f5688;
    border: 1px solid #99b5da;
    border-radius: 0.62rem;
    background: #f1f6ff;
    padding: 0.46rem 0.66rem;
    font-size: 0.88rem;
  }

  .tree-panel {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.6rem;
  }

  .panel-head h2 {
    margin: 0;
    font-size: 1.03rem;
  }

  .panel-head span {
    color: #5c6570;
    font-size: 0.81rem;
  }

  .quick-row {
    display: grid;
    grid-template-columns: minmax(180px, 1fr) auto auto auto;
    gap: 0.45rem;
    align-items: center;
  }

  .child-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    color: #415d82;
    font-size: 0.84rem;
    white-space: nowrap;
  }

  .tree-toolbar {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto auto;
    gap: 0.4rem;
    align-items: center;
  }

  .tree-toolbar input {
    min-width: 0;
    border-radius: 0.5rem;
    border-color: #d0d7de;
    background: #ffffff;
    color: #2f3437;
    padding: 0.42rem 0.54rem;
  }

  .tree-toolbar .subtle {
    border-color: #d0d7de;
    background: #ffffff;
    color: #374151;
    padding: 0.4rem 0.52rem;
    min-width: unset;
    font-size: 0.8rem;
  }

  .tree-frame {
    border: 1px solid #d8dee4;
    border-radius: 0.6rem;
    background: #ffffff;
    flex: 1;
    min-height: 220px;
    max-height: none;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .tree-list {
    margin: 0;
    padding: 0.22rem;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.08rem;
  }

  .tree-item {
    border-radius: 0.35rem;
  }

  .tree-row {
    display: grid;
    grid-template-columns: 1.25rem minmax(0, 1fr) auto;
    gap: 0.16rem;
    padding: 0.06rem 0.12rem 0.06rem calc(0.22rem + var(--depth) * 0.9rem);
    border-radius: 0.35rem;
    align-items: center;
  }

  .toggle {
    border: none;
    border-radius: 0.25rem;
    background: transparent;
    color: #6b7280;
    width: 1.1rem;
    height: 1.1rem;
    padding: 0;
    cursor: pointer;
    display: grid;
    place-items: center;
    font-size: 0.88rem;
    line-height: 1;
    transition: background 120ms ease, color 120ms ease;
  }

  .toggle:hover {
    color: #374151;
    background: #f3f4f6;
  }

  .toggle .chevron {
    display: block;
    width: 0.7rem;
    text-align: center;
    transition: transform 120ms ease;
    transform-origin: center;
  }

  .toggle.expanded .chevron {
    transform: rotate(90deg);
  }

  .toggle.placeholder {
    background: transparent;
    border: none;
    width: 1.1rem;
    height: 1.1rem;
  }

  .row-main {
    width: 100%;
    text-align: left;
    border-radius: 0.32rem;
    border: none;
    background: transparent;
    padding: 0.3rem 0.42rem;
    cursor: pointer;
    min-width: 0;
    color: #2f3437;
    font-size: 0.9rem;
    line-height: 1.3;
  }

  .tree-row:hover .row-main,
  .tree-row:focus-within .row-main {
    background: #f3f4f6;
  }

  .tree-row.active-ancestor .row-main {
    background: #f7f7f8;
  }

  .tree-row.active-leaf .row-main {
    background: #eceff3;
    color: #111827;
    font-weight: 600;
  }

  .tree-row.selected .row-main {
    background: #e5e7eb;
    color: #111827;
  }

  .row-main .title {
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row-quick {
    border: 1px solid #d0d7de;
    background: #ffffff;
    color: #4b5563;
    width: 1.4rem;
    height: 1.4rem;
    border-radius: 0.3rem;
    padding: 0;
    display: grid;
    place-items: center;
    font-size: 0.72rem;
    line-height: 1;
    opacity: 0;
    pointer-events: none;
    transition: opacity 120ms ease;
  }

  .tree-row:hover .row-quick,
  .tree-row:focus-within .row-quick {
    border-color: #9aa4b2;
    color: #1f2937;
    background: #f8fafc;
    opacity: 1;
    pointer-events: auto;
  }

  .row-quick:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  button,
  input {
    font: inherit;
  }

  input {
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

  button:disabled,
  input:disabled {
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

  .empty {
    margin: 0;
    color: #54759b;
    font-size: 0.9rem;
  }

  @media (max-height: 700px) {
    .tree-screen {
      height: auto;
      min-height: 100%;
      overflow: visible;
    }

    .tree-panel {
      flex: 0 0 auto;
      min-height: fit-content;
      overflow: visible;
    }

    .tree-frame {
      min-height: 300px;
    }
  }

  @media (max-width: 980px) {
    .page-head {
      flex-direction: column;
    }

    .range-switch {
      width: 100%;
      min-width: unset;
    }

    .selection-strip {
      flex-direction: column;
      align-items: flex-start;
    }

    .selection-actions {
      width: 100%;
      justify-content: flex-start;
    }

    .quick-row {
      grid-template-columns: 1fr;
    }

    .tree-toolbar {
      grid-template-columns: 1fr 1fr;
    }

    .tree-frame {
      max-height: 60vh;
      min-height: 260px;
    }
  }
</style>
