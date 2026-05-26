<script lang="ts">
  import {
    APP_DATA_CHANGED_EVENT,
    completeTaskTree,
    createTask,
    deleteTasks,
    getOverview,
    pauseTask,
    resumeTask,
    startTask,
    stopTask,
    type OverviewResponse,
    type TaskRecord,
  } from "$lib/api";
  import CommandBar from "../ConfiguredCommandBar.svelte";
  import TodoList from "$lib/components/TodoList.svelte";
  import { handleCommandInput, type CommandRunActionOptions } from "../command-handler";
  import { notifyCommandResult, notifyError } from "$lib/notifications";
  import {
    buildSubtreeRecentActivityMap,
    buildTaskChain,
    compareTasksByRecentActivity,
    compactTaskPath,
    formatClock,
    formatDate,
    formatSeconds,
    getRestElapsedSeconds,
    normalizeError,
    restSession,
    startManualRest,
    statusLabel,
    stopRest,
  } from "$lib/ui";
  import { onMount } from "svelte";

  type MiniTreeRow = {
    task: TaskRecord;
    depth: number;
    hasChildren: boolean;
  };
  type DetailContextMenu = {
    taskId: string;
    x: number;
    y: number;
  };

  let overview = $state<OverviewResponse | null>(null);
  let dayOverview = $state<OverviewResponse | null>(null);
  let selectedTaskId = $state<string | null>(null);
  let loading = $state(false);
  let currentAction = $state("");
  let nowTs = $state(Math.floor(Date.now() / 1000));
  let expandedMiniTaskIds = $state<Set<string>>(new Set());
  let miniAutoRootId = $state<string | null>(null);
  let miniAutoFocusId = $state<string | null>(null);
  let refreshInFlight = false;

  let commandInput = $state("");
  let lastCommandRunErrorDetail = $state<string | null>(null);
  let detailContextMenu = $state<DetailContextMenu | null>(null);

  const taskMap = $derived.by(() => {
    const map = new Map<string, TaskRecord>();
    for (const task of overview?.tasks ?? []) {
      map.set(task.id, task);
    }
    return map;
  });

  const subtreeRecentActivityMap = $derived.by(() =>
    buildSubtreeRecentActivityMap(overview?.tasks ?? [])
  );

  const childrenByParent = $derived.by(() => {
    const map = new Map<string, TaskRecord[]>();
    for (const task of overview?.tasks ?? []) {
      if (!task.parent_id) continue;
      const siblings = map.get(task.parent_id) ?? [];
      siblings.push(task);
      map.set(task.parent_id, siblings);
    }
    for (const siblings of map.values()) {
      siblings.sort((a, b) => compareTasksByRecentActivity(a, b, subtreeRecentActivityMap));
    }
    return map;
  });

  const selectedTask = $derived.by(() =>
    selectedTaskId ? (taskMap.get(selectedTaskId) ?? null) : null
  );

  const activeTask = $derived.by(() =>
    overview?.active_task_id ? (taskMap.get(overview.active_task_id) ?? null) : null
  );

  const activeTaskPathTitles = $derived.by(() =>
    buildTaskChain(activeTask?.id ?? null, taskMap).map((task) => task.title)
  );

  const activeTaskPath = $derived.by(() => activeTaskPathTitles.join(" / "));
  const activeTaskPathCompact = $derived.by(() => compactTaskPath(activeTaskPathTitles));

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

  const restElapsedSeconds = $derived.by(() =>
    $restSession.active ? getRestElapsedSeconds($restSession, nowTs) : 0
  );

  const canStartRest = $derived.by(() => {
    const task = heroControlTask;
    return !!task && (task.status === "running" || task.status === "paused");
  });
  const contextMenuTask = $derived.by(() =>
    detailContextMenu ? (taskMap.get(detailContextMenu.taskId) ?? null) : null
  );
  const contextMenuMiniRow = $derived.by(() =>
    detailContextMenu
      ? (miniTreeRows.find((row) => row.task.id === detailContextMenu?.taskId) ?? null)
      : null
  );

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
        "先在右侧 Mini 任务树选择目标任务，再执行 /rename、/parent、/adjust、/sub。",
        "需要直接新建并开始根任务时，使用 /new <title>。",
        "直接输入纯文本时：有选中任务则创建并开始其子任务；未选中时仅创建根任务。",
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
    hints.push("忘按暂停时可用 /adjust -15m 这类命令修正累计专注时长。");

    if (activeTask && activeTask.id !== selectedTask.id) {
      hints.push(`当前运行中的任务是「${activeTask.title}」，执行 /start 时会自动先暂停它。`);
    }
    return hints;
  });

  onMount(() => {
    void refresh();
    const onDataChanged = () => {
      if (refreshInFlight || !!currentAction) return;
      void refresh({ background: true });
    };
    window.addEventListener(APP_DATA_CHANGED_EVENT, onDataChanged);
    const onDocumentClick = () => closeDetailContextMenu();
    const onDocumentKeydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeDetailContextMenu();
    };
    window.addEventListener("click", onDocumentClick);
    window.addEventListener("keydown", onDocumentKeydown);
    window.addEventListener("resize", closeDetailContextMenu);
    const ticker = window.setInterval(() => {
      nowTs = Math.floor(Date.now() / 1000);
    }, 1_000);
    const poller = window.setInterval(() => {
      if (refreshInFlight || !!currentAction) return;
      void refresh({ background: true });
    }, 30_000);
    return () => {
      window.removeEventListener(APP_DATA_CHANGED_EVENT, onDataChanged);
      window.removeEventListener("click", onDocumentClick);
      window.removeEventListener("keydown", onDocumentKeydown);
      window.removeEventListener("resize", closeDetailContextMenu);
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


  $effect(() => {
    if (!$restSession.active) return;
    if (activeTask?.status !== "running") return;
    stopRest();
  });
  async function refresh(options: { background?: boolean } = {}) {
    const { background = false } = options;
    if (refreshInFlight) return;
    refreshInFlight = true;
    if (!background) {
      loading = true;
    }
    try {
      const [allSnapshot, daySnapshot] = await Promise.all([
        getOverview("all"),
        getOverview("today"),
      ]);
      overview = allSnapshot;
      dayOverview = daySnapshot;
      if (selectedTaskId && !allSnapshot.tasks.some((task) => task.id === selectedTaskId)) {
        selectedTaskId = null;
      }
      if (!selectedTaskId) {
        selectedTaskId =
          allSnapshot.active_task_id ??
          allSnapshot.last_used_task_id ??
          allSnapshot.tasks[0]?.id ??
          null;
      }
    } catch (error) {
      notifyError("刷新任务概览失败", error, "overview-refresh-error");
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
        notifyError(`${label}失败`, error, `action-error:${label}`);
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

  function openDetailContextMenu(event: MouseEvent, task: TaskRecord) {
    event.preventDefault();
    event.stopPropagation();
    selectedTaskId = task.id;
    const menuWidth = 230;
    const menuHeight = 340;
    const margin = 8;
    detailContextMenu = {
      taskId: task.id,
      x: Math.max(margin, Math.min(event.clientX, window.innerWidth - menuWidth - margin)),
      y: Math.max(margin, Math.min(event.clientY, window.innerHeight - menuHeight - margin)),
    };
  }

  function closeDetailContextMenu() {
    detailContextMenu = null;
  }

  function onDetailContextMenu(event: MouseEvent) {
    if (shouldUseNativeContextMenu(event.target)) return;
    if ((event.target as Element | null)?.closest(".detail-task-context-menu")) return;
    event.preventDefault();
    closeDetailContextMenu();
  }

  function shouldUseNativeContextMenu(target: EventTarget | null): boolean {
    if (!(target instanceof Element)) return false;
    return !!target.closest('input, textarea, select, [contenteditable="true"]');
  }

  async function toggleTaskRunState(task: TaskRecord) {
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

  async function onPrimaryToggle() {
    const task = heroControlTask;
    if (!task) return;
    await toggleTaskRunState(task);
  }

  async function onStopSelected() {
    const task = heroControlTask;
    if (!task) return;
    await stopTaskAction(task);
  }

  async function stopTaskAction(task: TaskRecord) {
    selectedTaskId = task.id;
    if (task.status !== "running" && task.status !== "paused") return;
    await runAction("停止任务", () => stopTask(task.id));
  }

  async function completeTaskAction(task: TaskRecord) {
    selectedTaskId = task.id;
    if (task.status === "stopped") return;
    await runAction(task.parent_id ? "完成任务分支" : "完成待办", () =>
      completeTaskTree(task.id)
    );
  }

  async function deleteTaskAction(task: TaskRecord, hardDelete: boolean) {
    selectedTaskId = task.id;
    const modeLabel = hardDelete ? "硬删除" : "软删除（归档）";
    const warning = hardDelete
      ? "该操作不可恢复，将彻底移除任务、其子任务及相关事件记录。"
      : "该操作会归档任务子树，可视为软删除。";
    const confirmed = window.confirm(
      `确认${modeLabel}任务「${task.title}」及其全部子任务吗？\n${warning}`
    );
    if (!confirmed) return;
    await runAction(hardDelete ? "硬删除任务" : "删除任务", () =>
      deleteTasks([task.id], hardDelete)
    );
  }

  async function onToggleRest() {
    if ($restSession.active) {
      stopRest();
      return;
    }

    const task = heroControlTask;
    if (!task) return;
    if (task.status === "running") {
      const paused = await runAction("开始休息", () => pauseTask(task.id));
      if (paused === null) return;
    } else if (task.status !== "paused") {
      return;
    }

    startManualRest();
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

  function taskLiveExclusiveSeconds(task: TaskRecord | null): number {
    if (!task) return 0;
    if (task.status !== "running" || !overview) {
      return task.exclusive_seconds;
    }
    return task.exclusive_seconds + Math.max(0, nowTs - overview.generated_at);
  }

  function taskLiveInclusiveSeconds(task: TaskRecord | null): number {
    if (!task) return 0;
    if (!overview || !activePathIds.has(task.id)) {
      return task.inclusive_seconds;
    }
    return task.inclusive_seconds + Math.max(0, nowTs - overview.generated_at);
  }

  async function onMiniNodeToggle(event: MouseEvent, task: TaskRecord) {
    event.stopPropagation();
    await toggleTaskRunState(task);
  }

  async function onContextPrimaryAction() {
    const task = contextMenuTask;
    if (!task) return;
    closeDetailContextMenu();
    await toggleTaskRunState(task);
  }

  async function onContextStopTask() {
    const task = contextMenuTask;
    if (!task) return;
    closeDetailContextMenu();
    await stopTaskAction(task);
  }

  async function onContextCompleteTask() {
    const task = contextMenuTask;
    if (!task) return;
    closeDetailContextMenu();
    await completeTaskAction(task);
  }

  async function onContextCreateSubtask() {
    const task = contextMenuTask;
    if (!task) return;
    closeDetailContextMenu();
    const title = window.prompt(`给「${task.title}」新增子任务`);
    const trimmedTitle = title?.trim();
    if (!trimmedTitle) return;

    const childId = await runAction("创建子任务", () => createTask(trimmedTitle, task.id));
    if (!childId) return;
    selectedTaskId = childId;
    const next = new Set(expandedMiniTaskIds);
    next.add(task.id);
    expandedMiniTaskIds = next;
  }

  function onContextToggleMiniExpand() {
    const task = contextMenuTask;
    if (!task) return;
    toggleMiniExpand(task.id);
    closeDetailContextMenu();
  }

  async function onContextCopyPath() {
    const task = contextMenuTask;
    if (!task) return;
    const path = buildTaskChain(task.id, taskMap)
      .map((item) => item.title)
      .join(" / ");
    closeDetailContextMenu();
    try {
      await navigator.clipboard.writeText(path);
      notifyCommandResult("已复制任务路径", "success", path);
    } catch (error) {
      notifyError("复制任务路径失败", error, "detail-copy-path-error");
    }
  }

  function onContextFocusTask() {
    const task = contextMenuTask;
    if (!task) return;
    selectedTaskId = task.id;
    closeDetailContextMenu();
  }

  function onContextOpenTree() {
    const task = contextMenuTask;
    if (!task) return;
    selectedTaskId = task.id;
    closeDetailContextMenu();
    window.location.href = "/tree";
  }

  async function onContextDeleteTask(hardDelete: boolean) {
    const task = contextMenuTask;
    if (!task) return;
    closeDetailContextMenu();
    await deleteTaskAction(task, hardDelete);
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

  function onFocusActiveTask() {
    if (!activeTask) return;
    if (selectedTaskId === activeTask.id) return;
    selectedTaskId = activeTask.id;
    notifyCommandResult(`操控目标已切换为活动任务「${activeTask.title}」`, "success");
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

<main class="detail-screen" oncontextmenu={onDetailContextMenu}>
  {#if !$restSession.active && activeTask}
    <header
      class="hero clickable"
      role="button"
      tabindex="0"
      aria-label="聚焦当前活动任务作为操控目标"
      onclick={onFocusActiveTask}
      onkeydown={onHeroKeydown}
      oncontextmenu={(event) => openDetailContextMenu(event, activeTask)}
    >
      <div class="hero-main">
        <p class="eyebrow">主工作台</p>
        <h1 class="hero-title" title={activeTask.title}>{activeTask.title}</h1>
        <p class="hero-meta">
          <span class="hero-path" title={activeTaskPath || "-"}>路径 {activeTaskPathCompact || "-"}</span>
          <span class="hero-status">· {statusLabel(activeTask.status)}</span>
        </p>
        <p class="hero-stats">
          Ex {formatSeconds(activeElapsedSeconds)} · In {formatSeconds(activeInclusiveSeconds)}
        </p>
      </div>
      <div class="hero-actions">
        <a href="/tree" class="ghost-link">打开任务树工作区</a>
        <button type="button" class="secondary" onclick={() => void refresh()} disabled={loading || !!currentAction}>
          {loading ? "刷新中..." : "刷新"}
        </button>
      </div>
    </header>
  {:else}
    <header class="hero">
      <div class="hero-main">
        <p class="eyebrow">主工作台</p>
        {#if $restSession.active}
          <h1 class="hero-title">休息中</h1>
          <p class="hero-meta">
            <span class="hero-path">
              已切换为休息计时器
              {#if activeTask}
                · 关联任务 {activeTask.title}
              {/if}
            </span>
          </p>
          <p class="hero-stats">
            当前任务按暂停语义处理 · 已休息 {formatSeconds(restElapsedSeconds)}
          </p>
        {:else}
          <h1>暂无活动任务</h1>
          <p class="hero-meta">请在右侧任务树中开始一个任务</p>
        {/if}
      </div>
      <div class="hero-actions">
        <a href="/tree" class="ghost-link">打开任务树工作区</a>
        <button type="button" class="secondary" onclick={() => void refresh()} disabled={loading || !!currentAction}>
          {loading ? "刷新中..." : "刷新"}
        </button>
      </div>
    </header>
  {/if}

  <section class="content-grid">
    <section class="main-stack">
      <article class="panel session-panel">
        <div class="session-head">
          <h2>{$restSession.active ? "休息计时器" : "会话计时器"}</h2>
          <p>当日已专注 {formatSeconds(todayFocusedSeconds)}</p>
        </div>
        {#if heroControlTask || $restSession.active}
          <p class="session-target">{$restSession.active ? "休息中" : heroControlTask?.title}</p>
          <p class="session-meta">
            {#if $restSession.active}
              当前任务按暂停语义处理 · 已休息 {formatSeconds(restElapsedSeconds)}
              {#if heroControlTask}
                · 关联任务 {heroControlTask.title}
              {/if}
            {:else if heroControlTask}
              状态 {statusLabel(heroControlTask.status)} · Ex {formatSeconds(timerElapsedSeconds)} · In
              {formatSeconds(timerInclusiveSeconds)}
            {/if}
          </p>
          <p class="session-clock">
            {formatClock($restSession.active ? restElapsedSeconds : timerElapsedSeconds)}
          </p>
          {#if $restSession.active}
            <p class="meta">休息正计时持续中，可点击“结束休息”返回任务控制。</p>
          {/if}
          <div class="session-actions">
            <button type="button" onclick={onPrimaryToggle} disabled={!!currentAction || !heroControlTask}>
              {heroControlTask?.status === "running"
                ? "暂停"
                : heroControlTask?.status === "paused"
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
                !heroControlTask ||
                (heroControlTask.status !== "running" && heroControlTask.status !== "paused")
              }
            >
              停止
            </button>
          </div>
        {:else}
          <p class="session-clock">{formatClock(todayFocusedSeconds)}</p>
          <p class="empty">暂无可操控任务，可先在右侧任务树中选择一个任务。</p>
          <div class="session-actions">
            <button type="button" class="secondary" onclick={onToggleRest} disabled={!!currentAction || (!$restSession.active && !canStartRest)}>
              {$restSession.active ? "结束休息" : "休息"}
            </button>
          </div>
        {/if}
      </article>

      <article class="panel detail-main">
        {#if selectedTask}
          <section
            class="detail-top"
            role="group"
            aria-label="选中任务详情"
            oncontextmenu={(event) => openDetailContextMenu(event, selectedTask)}
          >
            <p class="detail-title" title={selectedTask.title}>{selectedTask.title}</p>
            <p class="meta">
              创建于 {formatDate(selectedTask.created_at)} · Ex {formatSeconds(taskLiveExclusiveSeconds(selectedTask))} · In
              {formatSeconds(taskLiveInclusiveSeconds(selectedTask))}
            </p>
          </section>
        {:else}
          <section class="detail-top">
            <p class="empty">暂无选中任务，可在右侧 mini 树选择或新建。</p>
          </section>
        {/if}

        <section class="detail-workspace">
          <TodoList
            tasks={overview?.tasks ?? []}
            {selectedTaskId}
            activeTaskId={overview?.active_task_id ?? null}
            busy={!!currentAction}
            onselect={(taskId) => (selectedTaskId = taskId)}
          />

          <section class="detail-command scroll-hint">
            <h2>命令模式</h2>
            <p class="meta">
              当前操控目标：{selectedTask ? selectedTask.title : "未选择任务"}
              {#if activeTask && selectedTask && activeTask.id !== selectedTask.id}
                （点击上方主工作台可切换为活动任务）
              {/if}
            </p>
            <CommandBar
              bind:value={commandInput}
              busy={!!currentAction}
              tasks={overview?.tasks ?? []}
              onexecute={onCommandExecute}
            />
            <ul class="command-hints">
              {#each commandContextHints as hint}
                <li>{hint}</li>
              {/each}
            </ul>
          </section>
        </section>
      </article>
    </section>

    <aside class="side-rail">
      <article class="panel today-focus-panel">
        <div class="today-head">
          <h2>当日已专注</h2>
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
          <div class="mini-tree-frame scroll-hint" onscroll={closeDetailContextMenu}>
            <ul class="mini-list" role="tree" aria-label="当前任务系任务树">
              {#each miniTreeRows as row (row.task.id)}
                <li class="mini-item">
                  <div
                    class="mini-tree-row"
                    role="treeitem"
                    tabindex="-1"
                    aria-selected={selectedTaskId === row.task.id}
                    class:selected={selectedTaskId === row.task.id}
                    class:active-ancestor={activePathIds.has(row.task.id) && activeTask?.id !== row.task.id}
                    class:active-leaf={activeTask?.id === row.task.id}
                    class:context-open={detailContextMenu?.taskId === row.task.id}
                    oncontextmenu={(event) => openDetailContextMenu(event, row.task)}
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
                      title={`${row.task.title}\n${statusLabel(row.task.status)} · Ex ${formatSeconds(taskLiveExclusiveSeconds(row.task))} · In ${formatSeconds(taskLiveInclusiveSeconds(row.task))}`}
                    >
                      <span class="mini-row-title">{row.task.title}</span>
                      <span class="mini-row-meta">
                        {statusLabel(row.task.status)} · Ex {formatSeconds(taskLiveExclusiveSeconds(row.task))} · In
                        {formatSeconds(taskLiveInclusiveSeconds(row.task))}
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

  {#if detailContextMenu && contextMenuTask}
    <div
      class="detail-task-context-menu"
      role="menu"
      tabindex="-1"
      aria-label={`任务操作：${contextMenuTask.title}`}
      style={`left:${detailContextMenu.x}px;top:${detailContextMenu.y}px`}
      oncontextmenu={(event) => event.preventDefault()}
    >
      <div class="context-menu-head">
        <span class="context-menu-title" title={contextMenuTask.title}>{contextMenuTask.title}</span>
        <span>{statusLabel(contextMenuTask.status)}</span>
      </div>
      <button type="button" role="menuitem" onclick={() => void onContextPrimaryAction()} disabled={!!currentAction}>
        {contextMenuTask.status === "running"
          ? "暂停"
          : contextMenuTask.status === "paused"
            ? "恢复"
            : "开始"}
      </button>
      <button
        type="button"
        role="menuitem"
        onclick={() => void onContextStopTask()}
        disabled={!!currentAction || (contextMenuTask.status !== "running" && contextMenuTask.status !== "paused")}
      >
        停止
      </button>
      <button
        type="button"
        role="menuitem"
        onclick={() => void onContextCompleteTask()}
        disabled={!!currentAction || contextMenuTask.status === "stopped"}
      >
        {contextMenuTask.parent_id ? "完成分支" : "完成待办"}
      </button>
      <div class="context-separator"></div>
      <button type="button" role="menuitem" onclick={onContextFocusTask}>设为操控目标</button>
      <button type="button" role="menuitem" onclick={() => void onContextCreateSubtask()} disabled={!!currentAction}>
        新增子任务
      </button>
      {#if contextMenuMiniRow?.hasChildren}
        <button type="button" role="menuitem" onclick={onContextToggleMiniExpand}>
          {expandedMiniTaskIds.has(contextMenuTask.id) ? "收起子任务" : "展开子任务"}
        </button>
      {/if}
      <div class="context-separator"></div>
      <button type="button" role="menuitem" onclick={() => void onContextCopyPath()}>复制路径</button>
      <button type="button" role="menuitem" onclick={onContextOpenTree}>打开完整任务树</button>
      <div class="context-separator"></div>
      <button
        type="button"
        role="menuitem"
        class="context-danger"
        onclick={() => void onContextDeleteTask(false)}
        disabled={!!currentAction}
      >
        删除（归档）
      </button>
      <button
        type="button"
        role="menuitem"
        class="context-danger strong"
        onclick={() => void onContextDeleteTask(true)}
        disabled={!!currentAction}
      >
        删除（硬）
      </button>
    </div>
  {/if}
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
    background: rgba(255, 255, 255, 0.9);
    border-radius: 1rem;
    padding: 1rem 1.1rem;
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    flex-shrink: 0;
  }

  .hero-main {
    min-width: 0;
    flex: 1;
  }

  .hero.clickable {
    cursor: pointer;
  }

  .hero.clickable:hover {
    background: rgba(248, 251, 255, 0.96);
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

  .hero-title {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .hero-meta {
    margin: 0;
    color: #415d82;
    font-size: 0.9rem;
    display: flex;
    align-items: baseline;
    gap: 0.3rem;
    min-width: 0;
  }

  .hero-path {
    min-width: 0;
    max-width: min(100%, 62ch);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .hero-status {
    white-space: nowrap;
    flex-shrink: 0;
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

  .detail-task-context-menu {
    position: fixed;
    z-index: 30;
    width: 230px;
    border: 1px solid #cfd8e6;
    border-radius: 0.62rem;
    background: #ffffff;
    box-shadow: 0 18px 42px rgba(17, 36, 63, 0.2);
    padding: 0.36rem;
    display: flex;
    flex-direction: column;
    gap: 0.12rem;
  }

  .context-menu-head {
    display: grid;
    gap: 0.08rem;
    padding: 0.24rem 0.38rem 0.34rem;
    color: #64748b;
    font-size: 0.72rem;
    border-bottom: 1px solid #edf1f6;
    margin-bottom: 0.12rem;
  }

  .context-menu-title {
    min-width: 0;
    color: #172b46;
    font-size: 0.84rem;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .detail-task-context-menu button {
    width: 100%;
    display: flex;
    justify-content: flex-start;
    align-items: center;
    border-radius: 0.42rem;
    background: transparent;
    color: #26364a;
    padding: 0.42rem 0.48rem;
    text-align: left;
    font-size: 0.82rem;
  }

  .detail-task-context-menu button:hover:not(:disabled),
  .detail-task-context-menu button:focus-visible {
    background: #eef4ff;
    color: #1f4f92;
    outline: none;
  }

  .detail-task-context-menu button.context-danger {
    color: #8a2a2a;
  }

  .detail-task-context-menu button.context-danger:hover:not(:disabled),
  .detail-task-context-menu button.context-danger:focus-visible {
    background: #fff0f0;
    color: #7f1f1f;
  }

  .detail-task-context-menu button.context-danger.strong {
    font-weight: 700;
  }

  .context-separator {
    height: 1px;
    background: #edf1f6;
    margin: 0.14rem 0;
  }

  .ghost-link {
    text-decoration: none;
    color: #2d4f7d;
    border-radius: 0.62rem;
    padding: 0.46rem 0.66rem;
    background: #eef4ff;
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
    background: rgba(255, 255, 255, 0.9);
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
  .detail-workspace,
  .detail-command {
    background: transparent;
    border-radius: 0;
    padding: 0;
  }

  .detail-top {
    background: transparent;
  }

  .detail-top:hover .detail-title {
    color: #173f70;
  }

  .detail-title {
    margin: 0;
    font-size: 1.14rem;
    font-weight: 700;
    color: #0f2f54;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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

  .detail-workspace {
    border-top: 1px solid #d8dee4;
    min-height: 0;
    overflow: hidden;
    overscroll-behavior: contain;
    padding-top: 0.72rem;
    display: grid;
    grid-template-columns: minmax(13rem, 16rem) minmax(0, 1fr);
    gap: 0.75rem;
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

  .mini-tree-row.context-open .mini-row-main {
    background: #dfeaff;
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
    border: none;
    border-radius: 0.3rem;
    background: #f1f4f8;
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
    color: #1f2937;
    background: #e7edf5;
  }

  .mini-row-quick:disabled {
    opacity: 0.5;
    pointer-events: none;
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

  @media (min-width: 1680px) and (min-aspect-ratio: 2/1) {
    .main-stack {
      grid-template-rows: minmax(18rem, 1.35fr) minmax(12rem, 0.95fr);
    }

    .session-panel {
      display: grid;
      grid-template-rows: auto auto auto minmax(0, 1fr) auto minmax(0, 1fr) auto;
      padding: clamp(1rem, 1vw, 1.35rem) 1.05rem;
      row-gap: 0.5rem;
    }

    .session-target {
      font-size: 1.14rem;
    }

    .session-meta {
      font-size: 0.92rem;
    }

    .session-clock {
      grid-row: 5;
      margin: 0;
      text-align: center;
      font-size: clamp(3.8rem, 6.4vw, 8.8rem);
      min-height: clamp(7rem, 13.5vw, 12.2rem);
      padding-block: clamp(0.55rem, 1.35vw, 1.55rem);
      display: flex;
      align-items: center;
      justify-content: center;
    }

    .session-actions {
      grid-row: 7;
      width: min(100%, 860px);
      justify-self: center;
      gap: 0.6rem;
    }

    .session-actions button {
      flex: 1 1 0;
      max-width: 420px;
      min-height: 2.7rem;
      font-size: 1rem;
    }
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
    .detail-workspace,
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

  @media (max-width: 760px) {
    .detail-workspace {
      grid-template-columns: 1fr;
      overflow: visible;
    }
  }
</style>








