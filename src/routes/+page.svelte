<script lang="ts">
  import {
    APP_DATA_CHANGED_EVENT,
    addTagToTask,
    createTask,
    getOverview,
    insertSubtaskAndStart,
    pauseTask,
    renameTask,
    reparentTask,
    resumeTask,
    startTask,
    stopTask,
    type OverviewResponse,
    type TaskRecord,
  } from "$lib/api";
  import CommandBar from "$lib/components/CommandBar.svelte";
  import { executeParsedCommand, type CommandFeedbackTone } from "$lib/command/executor";
  import { parseCommandInput } from "$lib/command/parser";
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
    hasChildren: boolean;
    childCount: number;
  };

  type RunActionOptions = {
    surfaceError?: boolean;
  };

  let overview = $state<OverviewResponse | null>(null);
  let selectedTaskId = $state<string | null>(null);
  let loading = $state(false);
  let currentAction = $state("");
  let errorMessage = $state("");
  let nowTs = $state(Math.floor(Date.now() / 1000));

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

  const activeTaskPath = $derived.by(() =>
    buildTaskChain(activeTask?.id ?? null, taskMap)
      .map((task) => task.title)
      .join(" / ")
  );

  const heroControlTask = $derived.by(() => activeTask ?? selectedTask);

  const miniNodes = $derived.by(() => {
    const nodes: MiniNode[] = [];

    const pushNode = (task: TaskRecord, depth: number, kind: MiniNodeKind) => {
      const childCount = (childrenByParent.get(task.id) ?? []).length;
      nodes.push({
        task,
        depth,
        kind,
        hasChildren: childCount > 0,
        childCount,
      });
    };

    if (selectedTask) {
      const chain = buildTaskChain(selectedTask.id, taskMap);
      chain.forEach((task, index) => {
        pushNode(task, index, index === chain.length - 1 ? "current" : "ancestor");
      });

      const children = childrenByParent.get(selectedTask.id) ?? [];
      for (const child of children) {
        pushNode(child, chain.length, "child");
      }
      return nodes;
    }

    for (const root of rootTasks) {
      pushNode(root, 0, "root");
    }
    return nodes;
  });

  const activeElapsedSeconds = $derived.by(() => {
    if (!activeTask) return 0;
    if (activeTask.status !== "running" || !overview) {
      return activeTask.exclusive_seconds;
    }
    return activeTask.exclusive_seconds + Math.max(0, nowTs - overview.generated_at);
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

  async function runAction<T>(
    label: string,
    action: () => Promise<T>,
    options: RunActionOptions = {}
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
    options: RunActionOptions = {}
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
    errorMessage = "";
    const parsed = parseCommandInput(input);
    const result = await executeParsedCommand({
      parsed,
      selectedTask,
      selectedTaskId,
      activeTask,
      tasks: overview?.tasks ?? [],
      run: {
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
      },
      ensureSwitchFromActive: (targetTaskId) =>
        ensureSwitchFromActive(targetTaskId, { surfaceError: false }),
      selectTask: (taskId) => {
        selectedTaskId = taskId;
      },
    });

    commandFeedback = result.message;
    commandFeedbackTone = result.tone;
    if (result.clearInput) {
      commandInput = "";
    }
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
        <p class="hero-time">当前任务已用 {formatClock(activeElapsedSeconds)}</p>
      {:else}
        <h1>暂无活动任务</h1>
        <p class="hero-meta">请在右侧 mini 任务树中开始一个任务</p>
      {/if}
    </div>
    <div class="hero-actions">
      <a href="/tree" class="ghost-link">打开任务树工作区</a>
      <button type="button" class="secondary" onclick={refresh} disabled={loading || !!currentAction}>
        {loading ? "刷新中..." : "刷新"}
      </button>
      <button type="button" onclick={onPrimaryToggle} disabled={!heroControlTask || !!currentAction}>
        {heroControlTask?.status === "running"
          ? "暂停"
          : heroControlTask?.status === "paused"
            ? "恢复"
            : "开始"}
      </button>
      <button
        type="button"
        class="danger"
        onclick={onStopSelected}
        disabled={!heroControlTask || !!currentAction}
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
      </section>

      <section class="detail-guide">
        <div class="guide-head">
          <h2>命令速查</h2>
          <p class="meta">支持 `/rename`、`/parent`、`/start`、`/pause`、`/resume`、`/stop`、`/sub`</p>
        </div>
        <div class="guide-grid">
          <article class="guide-card">
            <p class="guide-cmd">/rename 新标题</p>
            <p class="guide-desc">重命名当前选中任务</p>
          </article>
          <article class="guide-card">
            <p class="guide-cmd">/parent root</p>
            <p class="guide-desc">将当前任务提升为根任务</p>
          </article>
          <article class="guide-card">
            <p class="guide-cmd">/parent 任务ID</p>
            <p class="guide-desc">将当前任务挂到指定父任务下</p>
          </article>
          <article class="guide-card">
            <p class="guide-cmd">/start | /pause | /resume | /stop</p>
            <p class="guide-desc">控制当前任务状态机</p>
          </article>
          <article class="guide-card">
            <p class="guide-cmd">/sub 子任务标题</p>
            <p class="guide-desc">在当前任务下创建子任务（运行中则插入并开始）</p>
          </article>
          <article class="guide-card">
            <p class="guide-cmd">写周报 #work #writing</p>
            <p class="guide-desc">纯文本创建任务并追加标签</p>
          </article>
        </div>
        <ul class="guide-context">
          {#each commandContextHints as hint}
            <li>{hint}</li>
          {/each}
        </ul>
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
                    title={`${node.task.title}\n${statusLabel(node.task.status)} · Ex ${formatSeconds(node.task.exclusive_seconds)}${node.hasChildren ? ` · 子任务 ${node.childCount}` : ""}`}
                  >
                    <span class="mini-node-title-line">
                      <span class="mini-node-title">{node.task.title}</span>
                      {#if node.hasChildren}
                        <span class="mini-node-branch" title={`下有 ${node.childCount} 个子任务`}>
                          ↳{node.childCount}
                        </span>
                      {/if}
                    </span>
                    <span class="mini-node-sub">
                      {statusLabel(node.task.status)}{node.hasChildren ? ` · 子任务 ${node.childCount}` : ""}
                    </span>
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
    align-items: stretch;
    flex: 1;
    height: 100%;
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
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    gap: 0.8rem;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    overscroll-behavior: contain;
  }

  .detail-top,
  .detail-command,
  .detail-guide {
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
  .empty {
    margin: 0;
    color: #4d6c91;
    font-size: 0.86rem;
    line-height: 1.35;
  }

  .detail-guide {
    display: flex;
    flex-direction: column;
    gap: 0.56rem;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
    padding-right: 0.16rem;
  }

  .guide-head {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .guide-head h2 {
    margin-bottom: 0;
  }

  .guide-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.48rem;
  }

  .guide-card {
    border: 1px solid #cad9ee;
    border-radius: 0.68rem;
    background: #f7fbff;
    padding: 0.52rem 0.56rem;
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
    min-width: 0;
  }

  .guide-cmd {
    margin: 0;
    font-family: "IBM Plex Mono", "Cascadia Mono", monospace;
    font-size: 0.77rem;
    color: #1d436d;
    line-height: 1.32;
    word-break: break-word;
  }

  .guide-desc {
    margin: 0;
    color: #52749a;
    font-size: 0.77rem;
    line-height: 1.34;
  }

  .guide-context {
    margin: 0;
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
    grid-template-rows: minmax(0, 1fr) minmax(0, 3fr);
    gap: 1rem;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  .mini-timer {
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

  .mini-tree {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }

  .mini-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    flex: 1;
    min-height: 0;
    max-height: none;
    padding-right: 0.15rem;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .mini-node-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.24rem;
    align-items: center;
    padding-left: calc(min(var(--depth), 10) * 0.8rem);
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
    font-size: 0.86rem;
    line-height: 1.2;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mini-node-title-line {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.3rem;
    align-items: center;
  }

  .mini-node-branch {
    font-size: 0.68rem;
    color: #315986;
    border: 1px solid #bad0eb;
    border-radius: 999px;
    padding: 0.04rem 0.34rem;
    background: #eef5ff;
    white-space: nowrap;
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

    .detail-main,
    .side-rail {
      height: auto;
      grid-template-rows: auto;
      overflow: visible;
    }

    .guide-grid {
      grid-template-columns: 1fr;
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

    .detail-main {
      height: auto;
      grid-template-rows: auto;
      overflow: visible;
    }

    .detail-guide {
      overflow: visible;
    }

    .side-rail {
      height: auto;
      grid-template-rows: auto auto;
      overflow: visible;
    }

    .guide-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
