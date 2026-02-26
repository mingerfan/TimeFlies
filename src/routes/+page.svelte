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
  import { type CommandFeedbackTone } from "$lib/command/executor";
  import {
    buildTaskChain,
    formatClock,
    formatDate,
    formatSeconds,
    normalizeError,
    statusLabel,
  } from "$lib/ui";
  import { onMount } from "svelte";

  type MiniTreeRow = {
    task: TaskRecord;
    depth: number;
    hasChildren: boolean;
  };

  let overview = $state<OverviewResponse | null>(null);
  let dayOverview = $state<OverviewResponse | null>(null);
  let selectedTaskId = $state<string | null>(null);
  let loading = $state(false);
  let currentAction = $state("");
  let errorMessage = $state("");
  let nowTs = $state(Math.floor(Date.now() / 1000));
  let expandedMiniTaskIds = $state<Set<string>>(new Set());
  let miniAutoRootId = $state<string | null>(null);
  let miniAutoFocusId = $state<string | null>(null);

  let commandInput = $state("");
  let commandFeedback = $state("");
  let commandFeedbackTone = $state<CommandFeedbackTone>("success");

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

  const selectedTask = $derived.by(() =>
    selectedTaskId ? (taskMap.get(selectedTaskId) ?? null) : null
  );

  const activeTask = $derived.by(() =>
    overview?.active_task_id ? (taskMap.get(overview.active_task_id) ?? null) : null
  );

  const activeTaskPath = $derived.by(() =>
    buildTaskChain(activeTask?.id ?? null, taskMap)
      .map((task) => task.title)
      .join(" / ")
  );

  const heroControlTask = $derived.by(() => activeTask ?? selectedTask);

  const activePathIds = $derived.by(() => {
    const chain = buildTaskChain(activeTask?.id ?? null, taskMap);
    return new Set(chain.map((task) => task.id));
  });

  const miniTreeFocusTask = $derived.by(() => selectedTask ?? activeTask);

  const miniTreeRootTask = $derived.by(() => {
    const chain = buildTaskChain(miniTreeFocusTask?.id ?? null, taskMap);
    return chain[0] ?? null;
  });

  const miniTreeRoots = $derived.by(() => (miniTreeRootTask ? [miniTreeRootTask] : []));

  const miniTreeRows = $derived.by(() =>
    flattenMiniTreeRows(miniTreeRoots, childrenByParent, expandedMiniTaskIds)
  );

  const activeElapsedSeconds = $derived.by(() => {
    if (!activeTask) return 0;
    if (activeTask.status !== "running" || !overview) {
      return activeTask.exclusive_seconds;
    }
    return activeTask.exclusive_seconds + Math.max(0, nowTs - overview.generated_at);
  });

  const activeInclusiveSeconds = $derived.by(() => {
    if (!activeTask) return 0;
    if (activeTask.status !== "running" || !overview) {
      return activeTask.inclusive_seconds;
    }
    return activeTask.inclusive_seconds + Math.max(0, nowTs - overview.generated_at);
  });

  const timerElapsedSeconds = $derived.by(() => {
    const task = heroControlTask;
    if (!task) return 0;
    if (task.status !== "running" || !overview) {
      return task.exclusive_seconds;
    }
    return task.exclusive_seconds + Math.max(0, nowTs - overview.generated_at);
  });

  const timerInclusiveSeconds = $derived.by(() => {
    const task = heroControlTask;
    if (!task) return 0;
    if (task.status !== "running" || !overview) {
      return task.inclusive_seconds;
    }
    return task.inclusive_seconds + Math.max(0, nowTs - overview.generated_at);
  });

  const dayActiveLiveDelta = $derived.by(() => {
    const snapshot = dayOverview;
    if (!snapshot || !snapshot.active_task_id) return 0;
    const running = snapshot.tasks.find((task) => task.id === snapshot.active_task_id);
    if (!running || running.status !== "running") return 0;
    return Math.max(0, nowTs - snapshot.generated_at);
  });

  const todayFocusedSeconds = $derived.by(() => {
    if (!dayOverview) return 0;
    const base = dayOverview.tasks.reduce((sum, task) => sum + task.exclusive_seconds, 0);
    return base + dayActiveLiveDelta;
  });

  const commandContextHints = $derived.by(() => {
    if (!selectedTask) {
      return [
        "先在右侧 Mini 任务树选择目标任务，再执行 /rename、/parent、/sub。",
        "直接输入纯文本会创建任务；有选中任务时默认创建为它的子任务。",
        "输入 #tag 会在主动作成功后附加标签，不会阻断主命令执行。",
      ];
    }

    const hints = [`当前目标：${selectedTask.title}（${statusLabel(selectedTask.status)}）`];
    if (selectedTask.status === "running") {
      hints.push("可用 /sub <title> 直接插入并开始子任务。");
      hints.push("需要暂时中断可执行 /pause，需要结束当前任务可执行 /stop。");
    } else if (selectedTask.status === "paused") {
      hints.push("可用 /resume 恢复，也可执行 /start 直接恢复。");
    } else {
      hints.push("可用 /start 开始当前任务。");
    }

    if (activeTask && activeTask.id !== selectedTask.id) {
      hints.push(`当前运行中的任务是「${activeTask.title}」，执行 /start 时会自动先暂停它。`);
    }
    return hints;
  });

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
    const poller = window.setInterval(() => void refresh(), 30_000);
    return () => {
      window.removeEventListener(APP_DATA_CHANGED_EVENT, onDataChanged);
      window.clearInterval(ticker);
      window.clearInterval(poller);
    };
  });

  $effect(() => {
    const rootTask = miniTreeRootTask;
    const focusId = miniTreeFocusTask?.id ?? null;
    if (!rootTask) {
      if (expandedMiniTaskIds.size > 0) {
        expandedMiniTaskIds = new Set();
      }
      miniAutoRootId = null;
      miniAutoFocusId = null;
      return;
    }

    const scopedTaskIds = new Set(collectMiniSubtreeTaskIds(rootTask.id, childrenByParent));
    let nextExpanded = new Set([...expandedMiniTaskIds].filter((id) => scopedTaskIds.has(id)));
    const rootChanged = miniAutoRootId !== rootTask.id;
    const focusChanged = miniAutoFocusId !== focusId;

    if (rootChanged) {
      nextExpanded = new Set();
    }

    if (rootChanged || focusChanged) {
      nextExpanded.add(rootTask.id);
      for (const task of buildTaskChain(focusId, taskMap)) {
        if (scopedTaskIds.has(task.id)) {
          nextExpanded.add(task.id);
        }
      }
    }

    miniAutoRootId = rootTask.id;
    miniAutoFocusId = focusId;

    if (!areMiniSetsEqual(nextExpanded, expandedMiniTaskIds)) {
      expandedMiniTaskIds = nextExpanded;
    }
  });

  async function refresh() {
    loading = true;
    errorMessage = "";
    try {
      const [allSnapshot, daySnapshot] = await Promise.all([getOverview("all"), getOverview("day")]);
      overview = allSnapshot;
      dayOverview = daySnapshot;
      if (selectedTaskId && !allSnapshot.tasks.some((task) => task.id === selectedTaskId)) {
        selectedTaskId = null;
      }
      if (!selectedTaskId) {
        selectedTaskId = allSnapshot.active_task_id ?? allSnapshot.tasks[0]?.id ?? null;
      }
    } catch (error) {
      errorMessage = normalizeError(error);
    } finally {
      loading = false;
    }
  }

  async function runAction<T>(
    label: string,
    action: () => Promise<T>,
    options: CommandRunActionOptions = {}
  ): Promise<T | null> {
    const { surfaceError = true } = options;
    currentAction = label;
    if (surfaceError) {
      errorMessage = "";
    }
    try {
      const result = await action();
      await refresh();
      return result;
    } catch (error) {
      if (surfaceError) {
        errorMessage = normalizeError(error);
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
    if (!active || active.status !== "running" || active.id === targetTaskId) {
      return true;
    }
    const paused = await runAction("暂停当前任务", () => pauseTask(active.id), options);
    return paused !== null;
  }

  async function onPrimaryToggle() {
    const task = heroControlTask;
    if (!task) return;
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
    const task = heroControlTask;
    if (!task) return;
    if (task.status !== "running" && task.status !== "paused") return;
    await runAction("停止任务", () => stopTask(task.id));
  }

  function toggleMiniExpand(taskId: string) {
    const next = new Set(expandedMiniTaskIds);
    if (next.has(taskId)) {
      next.delete(taskId);
    } else {
      next.add(taskId);
    }
    expandedMiniTaskIds = next;
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

  async function onCommandExecute(input: string) {
    await handleCommandInput({
      input,
      selectedTask,
      selectedTaskId,
      activeTask,
      tasks: overview?.tasks ?? [],
      runAction,
      ensureSwitchFromActive,
      selectTask: (taskId) => (selectedTaskId = taskId),
      clearErrorMessage: () => {
        errorMessage = "";
      },
      setCommandFeedback: (message, tone) => {
        commandFeedback = message;
        commandFeedbackTone = tone;
      },
      clearCommandInput: () => {
        commandInput = "";
      },
    });
  }

  function onFocusActiveTask() {
    if (!activeTask) return;
    if (selectedTaskId === activeTask.id) return;
    selectedTaskId = activeTask.id;
    commandFeedbackTone = "success";
    commandFeedback = `操控目标已切换为活动任务「${activeTask.title}」`;
  }

  function onHeroKeydown(event: KeyboardEvent) {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    onFocusActiveTask();
  }

  function areMiniSetsEqual(left: Set<string>, right: Set<string>) {
    if (left.size !== right.size) return false;
    for (const id of left) {
      if (!right.has(id)) return false;
    }
    return true;
  }

  function collectMiniSubtreeTaskIds(
    rootTaskId: string,
    childrenMap: Map<string, TaskRecord[]>
  ): string[] {
    const stack = [rootTaskId];
    const visited = new Set<string>();
    const ids: string[] = [];

    while (stack.length > 0) {
      const taskId = stack.pop();
      if (!taskId || visited.has(taskId)) continue;
      visited.add(taskId);
      ids.push(taskId);
      for (const child of childrenMap.get(taskId) ?? []) {
        stack.push(child.id);
      }
    }
    return ids;
  }

  function flattenMiniTreeRows(
    roots: TaskRecord[],
    childrenMap: Map<string, TaskRecord[]>,
    expandedIds: Set<string>
  ): MiniTreeRow[] {
    const rows: MiniTreeRow[] = [];
    const visited = new Set<string>();

    const visit = (task: TaskRecord, depth: number) => {
      if (visited.has(task.id)) return;
      visited.add(task.id);
      const children = childrenMap.get(task.id) ?? [];
      rows.push({
        task,
        depth,
        hasChildren: children.length > 0,
      });
      if (!expandedIds.has(task.id)) return;
      for (const child of children) {
        visit(child, depth + 1);
      }
    };

    for (const root of roots) {
      visit(root, 0);
    }

    return rows;
  }
</script>

<main class="detail-screen">
  <header
    class="hero"
    class:clickable={!!activeTask}
    role="button"
    tabindex={activeTask ? 0 : -1}
    aria-label="聚焦当前活动任务作为操控目标"
    onclick={onFocusActiveTask}
    onkeydown={onHeroKeydown}
  >
    <div>
      <p class="eyebrow">主工作台</p>
      {#if activeTask}
        <h1>{activeTask.title}</h1>
        <p class="hero-meta">路径 {activeTaskPath || "-"} · {statusLabel(activeTask.status)}</p>
        <p class="hero-stats">
          Ex {formatSeconds(activeElapsedSeconds)} · In {formatSeconds(activeInclusiveSeconds)}
        </p>
      {:else}
        <h1>暂无活动任务</h1>
        <p class="hero-meta">请在右侧任务树中开始一个任务</p>
      {/if}
    </div>
    <div class="hero-actions">
      <a href="/tree" class="ghost-link">打开任务树工作区</a>
      <button type="button" class="secondary" onclick={refresh} disabled={loading || !!currentAction}>
        {loading ? "刷新中..." : "刷新"}
      </button>
    </div>
  </header>

  {#if errorMessage}
    <p class="error">{errorMessage}</p>
  {/if}

  <section class="content-grid">
    <section class="main-stack">
      <article class="panel session-panel">
        <div class="session-head">
          <h2>会话计时器</h2>
          <p>近 24 小时已专注 {formatSeconds(todayFocusedSeconds)}</p>
        </div>
        {#if heroControlTask}
          <p class="session-target">{heroControlTask.title}</p>
          <p class="session-meta">
            状态 {statusLabel(heroControlTask.status)} · Ex {formatSeconds(timerElapsedSeconds)} · In
            {formatSeconds(timerInclusiveSeconds)}
          </p>
          <p class="session-clock">{formatClock(timerElapsedSeconds)}</p>
          <div class="session-actions">
            <button type="button" onclick={onPrimaryToggle} disabled={!!currentAction}>
              {heroControlTask.status === "running"
                ? "暂停"
                : heroControlTask.status === "paused"
                  ? "恢复"
                  : "开始"}
            </button>
            <button
              type="button"
              class="danger"
              onclick={onStopSelected}
              disabled={!!currentAction || (heroControlTask.status !== "running" && heroControlTask.status !== "paused")}
            >
              停止
            </button>
          </div>
        {:else}
          <p class="session-clock">{formatClock(todayFocusedSeconds)}</p>
          <p class="empty">暂无可操控任务，可先在右侧任务树中选择一个任务。</p>
        {/if}
      </article>

      <article class="panel detail-main">
        {#if selectedTask}
          <section class="detail-top">
            <p class="detail-title">{selectedTask.title}</p>
            <p class="meta">
              创建于 {formatDate(selectedTask.created_at)} · Ex {formatSeconds(selectedTask.exclusive_seconds)} · In
              {formatSeconds(selectedTask.inclusive_seconds)}
            </p>
          </section>
        {:else}
          <section class="detail-top">
            <p class="empty">暂无选中任务，可在右侧 mini 树选择或新建。</p>
          </section>
        {/if}

        <section class="detail-command">
          <h2>命令模式</h2>
          <p class="meta">
            当前操控目标：{selectedTask ? selectedTask.title : "未选择任务"}
            {#if activeTask && selectedTask && activeTask.id !== selectedTask.id}
              （点击上方主工作台可切换为活动任务）
            {/if}
          </p>
          <CommandBar
            bind:value={commandInput}
            busy={loading || !!currentAction}
            feedback={commandFeedback}
            tone={commandFeedbackTone}
            onexecute={onCommandExecute}
          />
          <ul class="command-hints">
            {#each commandContextHints as hint}
              <li>{hint}</li>
            {/each}
          </ul>
        </section>
      </article>
    </section>

    <aside class="side-rail">
      <article class="panel today-focus-panel">
        <div class="today-head">
          <h2>近 24 小时已专注</h2>
          <p>{formatSeconds(todayFocusedSeconds)}</p>
        </div>
        <p class="today-clock">{formatClock(todayFocusedSeconds)}</p>
        {#if activeTask}
          <p class="today-active">{activeTask.title}</p>
          <p class="today-meta">当前 {statusLabel(activeTask.status)} · Ex {formatSeconds(activeElapsedSeconds)}</p>
        {:else}
          <p class="empty">暂无进行中的任务</p>
        {/if}
      </article>

      <article class="panel mini-tree">
        <div class="mini-tree-head">
          <h2>当前任务系</h2>
          <a href="/tree">完整树视图</a>
        </div>
        {#if !miniTreeRootTask}
          <p class="empty">暂无可显示的任务系</p>
        {:else if miniTreeRows.length === 0}
          <p class="empty">当前任务系暂无节点</p>
        {:else}
          <div class="mini-tree-frame">
            <ul class="mini-list" role="tree" aria-label="当前任务系任务树">
              {#each miniTreeRows as row (row.task.id)}
                <li class="mini-item">
                  <div
                    class="mini-tree-row"
                    class:selected={selectedTaskId === row.task.id}
                    class:active-ancestor={activePathIds.has(row.task.id) && activeTask?.id !== row.task.id}
                    class:active-leaf={activeTask?.id === row.task.id}
                    style={`--depth:${row.depth}`}
                  >
                    {#if row.hasChildren}
                      <button
                        type="button"
                        class="mini-toggle"
                        class:expanded={expandedMiniTaskIds.has(row.task.id)}
                        onclick={() => toggleMiniExpand(row.task.id)}
                        aria-label={expandedMiniTaskIds.has(row.task.id) ? "收起子任务" : "展开子任务"}
                      >
                        <span class="chevron" aria-hidden="true">▸</span>
                      </button>
                    {:else}
                      <span class="mini-toggle placeholder" aria-hidden="true"></span>
                    {/if}

                    <button
                      type="button"
                      class="mini-row-main"
                      onclick={() => (selectedTaskId = row.task.id)}
                      title={`${row.task.title}\n${statusLabel(row.task.status)} · Ex ${formatSeconds(row.task.exclusive_seconds)} · In ${formatSeconds(row.task.inclusive_seconds)}`}
                    >
                      <span class="mini-row-title">{row.task.title}</span>
                      <span class="mini-row-meta">
                        {statusLabel(row.task.status)} · Ex {formatSeconds(row.task.exclusive_seconds)} · In
                        {formatSeconds(row.task.inclusive_seconds)}
                      </span>
                    </button>

                    <button
                      type="button"
                      class="mini-row-quick"
                      onclick={(event) => onMiniNodeToggle(event, row.task)}
                      disabled={!!currentAction}
                      title={nodeActionLabel(row.task)}
                    >
                      {nodeActionSymbol(row.task)}
                    </button>
                  </div>
                </li>
              {/each}
            </ul>
          </div>
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

  .hero.clickable {
    cursor: pointer;
  }

  .hero.clickable:hover {
    border-color: rgba(65, 97, 143, 0.42);
    background: rgba(255, 255, 255, 0.95);
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

  .hero-stats {
    margin: 0.25rem 0 0;
    color: #36587f;
    font-size: 0.9rem;
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
    grid-template-columns: minmax(0, 1fr) 386px;
    gap: 1rem;
    align-items: stretch;
    flex: 1;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  .main-stack {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 1rem;
    min-height: 0;
    overflow: hidden;
  }

  .panel {
    background: rgba(255, 255, 255, 0.88);
    border: 1px solid rgba(65, 97, 143, 0.28);
    border-radius: 1rem;
    padding: 0.9rem;
  }

  .session-panel {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .detail-main {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 0.8rem;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    overscroll-behavior: contain;
  }

  .detail-top,
  .detail-command {
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

  .session-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.6rem;
  }

  .session-head h2 {
    margin: 0;
    font-size: 1rem;
  }

  .session-head p {
    margin: 0;
    color: #3d5f86;
    font-size: 0.84rem;
  }

  .session-target {
    margin: 0;
    font-size: 1.06rem;
    font-weight: 700;
    color: #173b68;
    line-height: 1.3;
  }

  .session-meta {
    margin: 0;
    color: #4b6b92;
    font-size: 0.86rem;
    line-height: 1.35;
  }

  .session-clock {
    margin: 0.12rem 0 0.08rem;
    font-family: "IBM Plex Mono", "Cascadia Mono", monospace;
    font-size: clamp(2rem, 4.2vw, 3.05rem);
    color: #174371;
    line-height: 1.04;
  }

  .session-actions {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
  }

  .session-actions button {
    flex: 1 1 120px;
  }

  .meta,
  .empty {
    margin: 0;
    color: #4d6c91;
    font-size: 0.86rem;
    line-height: 1.35;
  }

  .detail-command {
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
    padding-right: 0.14rem;
  }

  .command-hints {
    margin: 0.5rem 0 0;
    padding-left: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.32rem;
    color: #48698f;
    font-size: 0.82rem;
    line-height: 1.35;
  }

  .side-rail {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 1rem;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  .today-focus-panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    gap: 0.45rem;
  }

  .today-focus-panel h2,
  .mini-tree h2 {
    margin: 0 0 0.1rem;
    font-size: 0.95rem;
  }

  .today-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.5rem;
  }

  .today-head p {
    margin: 0;
    color: #3d5f86;
    font-size: 0.8rem;
    white-space: nowrap;
  }

  .today-clock {
    margin: 0;
    font-family: "IBM Plex Mono", "Cascadia Mono", monospace;
    font-size: clamp(1.6rem, 3.4vw, 2.1rem);
    line-height: 1.1;
    color: #174371;
  }

  .today-active {
    margin: 0;
    font-size: 0.92rem;
    font-weight: 700;
    color: #173b68;
    line-height: 1.3;
  }

  .today-meta {
    margin: 0;
    color: #4b6b92;
    font-size: 0.82rem;
    line-height: 1.35;
  }

  .mini-tree {
    min-height: 0;
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

  .mini-tree {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  .mini-tree-frame {
    border: 1px solid #d8dee4;
    border-radius: 0.68rem;
    background: #ffffff;
    flex: 1;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
    scrollbar-width: none;
    -ms-overflow-style: none;
  }

  .mini-tree-frame::-webkit-scrollbar {
    width: 0;
    height: 0;
    display: none;
  }

  .mini-list {
    margin: 0;
    padding: 0.24rem;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .mini-item {
    border-radius: 0.45rem;
  }

  .mini-tree-row {
    display: grid;
    grid-template-columns: 1.22rem minmax(0, 1fr) auto;
    gap: 0.2rem;
    align-items: center;
    padding: 0.08rem 0.12rem 0.08rem calc(0.22rem + min(var(--depth), 10) * 0.95rem);
    border-radius: 0.45rem;
  }

  .mini-toggle {
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
    font-size: 0.86rem;
    line-height: 1;
    transition: background 120ms ease, color 120ms ease;
  }

  .mini-toggle:hover {
    color: #374151;
    background: #f3f4f6;
  }

  .mini-toggle .chevron {
    display: block;
    width: 0.68rem;
    text-align: center;
    transition: transform 120ms ease;
    transform-origin: center;
  }

  .mini-toggle.expanded .chevron {
    transform: rotate(90deg);
  }

  .mini-toggle.placeholder {
    border: none;
    background: transparent;
    width: 1.1rem;
    height: 1.1rem;
  }

  .mini-row-main {
    width: 100%;
    text-align: left;
    border: none;
    border-radius: 0.45rem;
    background: transparent;
    padding: 0.42rem 0.52rem;
    color: #2f3437;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
  }

  .mini-tree-row:hover .mini-row-main,
  .mini-tree-row:focus-within .mini-row-main {
    background: #f3f4f6;
  }

  .mini-tree-row.active-ancestor .mini-row-main {
    background: #f7f8fa;
  }

  .mini-tree-row.active-leaf .mini-row-main {
    background: #eceff3;
    color: #111827;
  }

  .mini-tree-row.selected .mini-row-main {
    background: #e5e7eb;
    color: #111827;
  }

  .mini-row-title {
    font-size: 0.88rem;
    font-weight: 600;
    line-height: 1.25;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mini-row-meta {
    font-size: 0.76rem;
    color: #60718b;
    line-height: 1.32;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mini-row-quick {
    width: 1.42rem;
    height: 1.42rem;
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

  .mini-tree-row:hover .mini-row-quick,
  .mini-tree-row:focus-within .mini-row-quick {
    opacity: 1;
    pointer-events: auto;
    border-color: #9aa4b2;
    color: #1f2937;
    background: #f8fafc;
  }

  .mini-row-quick:disabled {
    opacity: 0.5;
    pointer-events: none;
  }

  button {
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

  button:disabled {
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
      height: auto;
      min-height: fit-content;
      overflow: visible;
    }

    .main-stack,
    .detail-main,
    .side-rail {
      height: auto;
      grid-template-rows: auto;
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

    .main-stack,
    .detail-main {
      height: auto;
      grid-template-rows: auto;
      overflow: visible;
    }

    .side-rail {
      height: auto;
      grid-template-rows: auto auto;
      overflow: visible;
    }
  }
</style>
