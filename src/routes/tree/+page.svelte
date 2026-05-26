<script lang="ts">
  import {
    APP_DATA_CHANGED_EVENT,
    completeTaskTree,
    createTask,
    deleteTasks,
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
  import { notifyError } from "$lib/notifications";
  import {
    buildSubtreeRecentActivityMap,
    buildTaskChain,
    compareTasksByRecentActivity,
    compactTaskPath,
    formatSeconds,
    statusLabel,
  } from "$lib/ui";
  import { onMount } from "svelte";

  type VisibleTaskRow = {
    task: TaskRecord;
    depth: number;
    hasChildren: boolean;
    visibleChildCount: number;
    isMatch: boolean;
    isPathOnly: boolean;
  };

  type DeleteMode = "archive" | "hard";
  type TreeCompletionFilter = "open" | "all" | "done";

  let overview = $state<OverviewResponse | null>(null);
  let range = $state<OverviewRange>("week");
  let selectedTaskId = $state<string | null>(null);
  let expandedTaskIds = $state<Set<string>>(new Set());
  let hasInitializedTreeExpansion = $state(false);
  let loading = $state(false);
  let currentAction = $state("");
  let nowTs = $state(Math.floor(Date.now() / 1000));
  let treeQuery = $state("");
  let treeCompletionFilter = $state<TreeCompletionFilter>("open");
  let quickAddTitle = $state("");
  let quickAddAsChild = $state(true);
  let batchMode = $state(false);
  let batchDeleteMode = $state<DeleteMode>("archive");
  let batchSelectedTaskIds = $state<Set<string>>(new Set());

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

  const rootTasks = $derived.by(() =>
    (overview?.tasks ?? [])
      .filter((task) => !task.parent_id)
      .sort((a, b) => compareTasksByRecentActivity(a, b, subtreeRecentActivityMap))
  );

  const selectedTask = $derived.by(() =>
    selectedTaskId ? (taskMap.get(selectedTaskId) ?? null) : null
  );

  const activeTaskId = $derived.by(() => overview?.active_task_id ?? null);

  const activePathIds = $derived.by(() => {
    const chain = buildTaskChain(activeTaskId, taskMap);
    return new Set(chain.map((task) => task.id));
  });

  const selectedTaskPathTitles = $derived.by(() =>
    buildTaskChain(selectedTask?.id ?? null, taskMap).map((task) => task.title)
  );

  const selectedTaskPath = $derived.by(() => selectedTaskPathTitles.join(" / "));
  const selectedTaskPathCompact = $derived.by(() => compactTaskPath(selectedTaskPathTitles));

  const normalizedTreeQuery = $derived.by(() => treeQuery.trim().toLowerCase());

  const scopedTreeIds = $derived.by(() =>
    buildScopedTreeIds(overview?.tasks ?? [], treeCompletionFilter, taskMap)
  );

  const scopedRootTasks = $derived.by(() =>
    rootTasks.filter((task) => scopedTreeIds.includedIds.has(task.id))
  );

  const treeSearchResult = $derived.by(() => {
    const query = normalizedTreeQuery;
    if (!query) return null;

    const matchedIds = new Set<string>();
    const visibleIds = new Set<string>();
    for (const taskId of scopedTreeIds.primaryIds) {
      const task = taskMap.get(taskId);
      if (!task || !task.title.toLowerCase().includes(query)) continue;
      matchedIds.add(task.id);
      addTaskAndAncestors(task.id, visibleIds, taskMap);
    }

    return { matchedIds, visibleIds };
  });

  const visibleRows = $derived.by(() =>
    flattenTaskRows(
      scopedRootTasks,
      childrenByParent,
      expandedTaskIds,
      true,
      scopedTreeIds.includedIds,
      scopedTreeIds.primaryIds,
      null
    )
  );

  const displayRows = $derived.by(() =>
    treeSearchResult
      ? flattenTaskRows(
          scopedRootTasks,
          childrenByParent,
          expandedTaskIds,
          false,
          treeSearchResult.visibleIds,
          treeSearchResult.matchedIds,
          treeSearchResult.matchedIds
        )
      : visibleRows
  );

  const filteredTaskCount = $derived.by(() => scopedTreeIds.primaryIds.size);
  const treeSearchMatchCount = $derived.by(() => treeSearchResult?.matchedIds.size ?? 0);
  const selectedTaskIsVisible = $derived.by(() =>
    selectedTaskId ? displayRows.some((row) => row.task.id === selectedTaskId) : true
  );

  const batchSelectedCount = $derived.by(() => batchSelectedTaskIds.size);

  const batchRootTaskIds = $derived.by(() => {
    if (batchSelectedTaskIds.size === 0) return [];
    const roots: string[] = [];

    for (const taskId of batchSelectedTaskIds) {
      if (!hasSelectedAncestor(taskId, batchSelectedTaskIds, taskMap)) {
        roots.push(taskId);
      }
    }

    return roots;
  });

  const batchAffectedCount = $derived.by(() => batchSelectedTaskIds.size);

  onMount(() => {
    const onDataChanged = () => {
      if (loading || !!currentAction) return;
      void refresh();
    };
    window.addEventListener(APP_DATA_CHANGED_EVENT, onDataChanged);
    const ticker = window.setInterval(() => {
      nowTs = Math.floor(Date.now() / 1000);
    }, 1_000);
    return () => {
      window.removeEventListener(APP_DATA_CHANGED_EVENT, onDataChanged);
      window.clearInterval(ticker);
    };
  });

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

    const prunedBatch = new Set([...batchSelectedTaskIds].filter((id) => validIds.has(id)));
    if (!areSetsEqual(prunedBatch, batchSelectedTaskIds)) {
      batchSelectedTaskIds = prunedBatch;
    }
  });

  async function refresh(targetRange: OverviewRange = range) {
    loading = true;
    try {
      const snapshot = await getOverview(targetRange);
      overview = snapshot;
      if (selectedTaskId && !snapshot.tasks.some((task) => task.id === selectedTaskId)) {
        selectedTaskId = null;
      }
      if (!selectedTaskId && snapshot.tasks.length > 0) {
        selectedTaskId =
          snapshot.active_task_id ?? snapshot.last_used_task_id ?? snapshot.tasks[0].id;
      }
    } catch (error) {
      notifyError("刷新任务树失败", error, "tree-refresh-error");
    } finally {
      loading = false;
    }
  }

  async function runAction<T>(label: string, action: () => Promise<T>): Promise<T | null> {
    currentAction = label;
    try {
      const result = await action();
      await refresh();
      return result;
    } catch (error) {
      notifyError(`${label}失败`, error, `tree-action-error:${label}`);
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
    expandedTaskIds = new Set(scopedTreeIds.includedIds);
  }

  function collapseAll() {
    expandedTaskIds = new Set();
  }

  function revealSelectedTask() {
    if (!selectedTaskId) return;
    treeCompletionFilter = "all";
    treeQuery = "";
    expandedTaskIds = expandAncestors(selectedTaskId, expandedTaskIds);
  }

  function expandAncestors(taskId: string, expandedIds: Set<string>): Set<string> {
    const next = new Set(expandedIds);
    const chain = buildTaskChain(taskId, taskMap);
    for (const task of chain.slice(0, -1)) {
      next.add(task.id);
    }
    return next;
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

  async function onPrimarySelectedToggle() {
    if (!selectedTask) return;
    if (selectedTask.status === "running") {
      await runAction("暂停任务", () => pauseTask(selectedTask.id));
      return;
    }
    if (!(await ensureSwitchFromActive(selectedTask.id))) return;
    if (selectedTask.status === "paused") {
      await runAction("恢复任务", () => resumeTask(selectedTask.id));
      return;
    }
    await runAction("开始任务", () => startTask(selectedTask.id));
  }

  async function onStopSelected() {
    if (!selectedTask) return;
    if (selectedTask.status !== "running" && selectedTask.status !== "paused") return;
    await runAction("停止任务", () => stopTask(selectedTask.id));
  }

  async function onCompleteSelected() {
    if (!selectedTask || selectedTask.status === "stopped") return;
    await runAction(selectedTask.parent_id ? "完成任务分支" : "完成待办", () =>
      completeTaskTree(selectedTask.id)
    );
  }

  async function onArchiveSelected() {
    await onDeleteSelected(false);
  }

  async function onHardDeleteSelected() {
    await onDeleteSelected(true);
  }

  async function onDeleteSelected(hardDelete: boolean) {
    if (!selectedTask) return;
    const modeLabel = hardDelete ? "硬删除" : "软删除（归档）";
    const warning = hardDelete
      ? "该操作不可恢复，将彻底移除任务、其子任务及相关事件记录。"
      : "该操作会归档任务子树，可视为软删除。";
    const confirmed = window.confirm(
      `确认${modeLabel}任务「${selectedTask.title}」及其全部子任务吗？\n${warning}`
    );
    if (!confirmed) return;
    if (!(await ensureDeleteReady([selectedTask.id], hardDelete))) return;
    await runAction(hardDelete ? "硬删除任务" : "删除任务", () =>
      deleteTasks([selectedTask.id], hardDelete)
    );
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

  function primaryActionLabel(task: TaskRecord): string {
    if (task.status === "running") return "暂停";
    if (task.status === "paused") return "恢复";
    if (task.status === "stopped") return "重新开始";
    return "开始";
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

  function toggleBatchMode() {
    if (batchMode) {
      batchMode = false;
      batchSelectedTaskIds = new Set();
      return;
    }

    batchMode = true;
    if (selectedTaskId) {
      const initial = new Set<string>();
      collectSubtreeTaskIds(selectedTaskId, childrenByParent, initial);
      batchSelectedTaskIds = initial;
    }
  }

  function clearBatchSelection() {
    batchSelectedTaskIds = new Set();
  }

  function onBatchRowChecked(taskId: string, checked: boolean) {
    const next = new Set(batchSelectedTaskIds);
    if (checked) {
      addSubtreeSelections(taskId, next);
    } else {
      removeSubtreeSelections(taskId, next);
      removeAncestorSelections(taskId, next, taskMap);
    }
    batchSelectedTaskIds = next;
  }

  async function onBatchDelete() {
    if (batchRootTaskIds.length === 0) return;
    const hardDelete = batchDeleteMode === "hard";
    const modeLabel = hardDelete ? "硬删除" : "软删除（归档）";
    const warning = hardDelete
      ? "不可恢复，将彻底移除选中任务子树和关联事件。"
      : "会归档选中任务子树。";
    const confirmed = window.confirm(
      `确认批量${modeLabel}吗？\n已选 ${batchSelectedCount} 个任务，预计影响 ${batchAffectedCount} 个节点。\n${warning}`
    );
    if (!confirmed) return;
    if (!(await ensureDeleteReady(batchRootTaskIds, hardDelete))) return;

    const done = await runAction(hardDelete ? "批量硬删除" : "批量删除", () =>
      deleteTasks(batchRootTaskIds, hardDelete)
    );
    if (done === null) return;
    batchSelectedTaskIds = new Set();
    batchMode = false;
  }

  function taskQuickActionLabel(task: TaskRecord): string {
    if (task.status === "running") return "暂停任务";
    if (task.status === "paused") return "恢复任务";
    if (task.status === "stopped") return "重新开始任务";
    return "开始任务";
  }

  function taskQuickActionSymbol(task: TaskRecord): string {
    if (task.status === "running") return "⏸";
    if (task.status === "stopped") return "↻";
    return "▶";
  }

  function resetTreeQuery() {
    treeQuery = "";
  }

  function taskMatchesCompletionFilter(task: TaskRecord, filter: TreeCompletionFilter): boolean {
    if (filter === "all") return true;
    if (filter === "done") return task.status === "stopped";
    return task.status !== "stopped";
  }

  function addTaskAndAncestors(
    taskId: string,
    collector: Set<string>,
    map: Map<string, TaskRecord>
  ) {
    let cursor: string | null = taskId;
    while (cursor) {
      if (collector.has(cursor)) break;
      collector.add(cursor);
      cursor = map.get(cursor)?.parent_id ?? null;
    }
  }

  function buildScopedTreeIds(
    tasks: TaskRecord[],
    filter: TreeCompletionFilter,
    map: Map<string, TaskRecord>
  ): { includedIds: Set<string>; primaryIds: Set<string> } {
    const includedIds = new Set<string>();
    const primaryIds = new Set<string>();

    for (const task of tasks) {
      if (!taskMatchesCompletionFilter(task, filter)) continue;
      primaryIds.add(task.id);
      addTaskAndAncestors(task.id, includedIds, map);
    }

    return { includedIds, primaryIds };
  }

  function treeCompletionFilterLabel(filter: TreeCompletionFilter): string {
    if (filter === "all") return "全部";
    if (filter === "done") return "已完成";
    return "未完成";
  }

  function treeSummaryText(): string {
    const total = overview?.tasks.length ?? 0;
    const scope = treeCompletionFilterLabel(treeCompletionFilter);
    if (normalizedTreeQuery) {
      return `${scope} ${filteredTaskCount} 项 · 匹配 ${treeSearchMatchCount} 项`;
    }
    return `${scope} ${filteredTaskCount} 项 / 全部 ${total} 项`;
  }

  function emptyTreeMessage(): string {
    const query = treeQuery.trim();
    if (query) {
      return `没有在${treeCompletionFilterLabel(treeCompletionFilter)}任务中匹配“${query}”的结果。`;
    }
    if (treeCompletionFilter === "open") return "当前没有未完成任务。";
    if (treeCompletionFilter === "done") return "当前没有已完成任务。";
    return "当前暂无任务。";
  }

  function rowMetaText(row: VisibleTaskRow): string {
    const parts = [
      statusLabel(row.task.status),
      `In ${formatSeconds(taskLiveInclusiveSeconds(row.task))}`,
    ];
    if (row.visibleChildCount > 0) {
      parts.push(`${row.visibleChildCount} 子任务`);
    }
    return parts.join(" · ");
  }

  function areSetsEqual(left: Set<string>, right: Set<string>) {
    if (left.size !== right.size) return false;
    for (const id of left) {
      if (!right.has(id)) return false;
    }
    return true;
  }

  function collectSubtreeTaskIds(
    rootTaskId: string,
    childrenMap: Map<string, TaskRecord[]>,
    collector: Set<string>
  ) {
    const stack = [rootTaskId];
    while (stack.length > 0) {
      const taskId = stack.pop();
      if (!taskId || collector.has(taskId)) continue;
      collector.add(taskId);
      for (const child of childrenMap.get(taskId) ?? []) {
        stack.push(child.id);
      }
    }
  }

  function addSubtreeSelections(taskId: string, selectedIds: Set<string>) {
    const descendants = new Set<string>();
    collectSubtreeTaskIds(taskId, childrenByParent, descendants);
    for (const id of descendants) {
      selectedIds.add(id);
    }
  }

  function removeSubtreeSelections(taskId: string, selectedIds: Set<string>) {
    const descendants = new Set<string>();
    collectSubtreeTaskIds(taskId, childrenByParent, descendants);
    for (const id of descendants) {
      selectedIds.delete(id);
    }
  }

  function removeAncestorSelections(
    taskId: string,
    selectedIds: Set<string>,
    map: Map<string, TaskRecord>
  ) {
    let parentId = map.get(taskId)?.parent_id ?? null;
    while (parentId) {
      selectedIds.delete(parentId);
      parentId = map.get(parentId)?.parent_id ?? null;
    }
  }

  function collectSubtreeIdsForRoots(rootIds: string[]): Set<string> {
    const collector = new Set<string>();
    for (const rootId of rootIds) {
      collectSubtreeTaskIds(rootId, childrenByParent, collector);
    }
    return collector;
  }

  function listActiveTasksForRoots(rootIds: string[]): TaskRecord[] {
    const active: TaskRecord[] = [];
    const subtreeIds = collectSubtreeIdsForRoots(rootIds);
    for (const taskId of subtreeIds) {
      const task = taskMap.get(taskId);
      if (!task) continue;
      if (task.status === "running" || task.status === "paused") {
        active.push(task);
      }
    }
    return active;
  }

  async function stopTasks(taskIds: string[]): Promise<boolean> {
    if (taskIds.length === 0) return true;
    currentAction = "停止任务";
    try {
      for (const taskId of taskIds) {
        await stopTask(taskId);
      }
      await refresh();
      return true;
    } catch (error) {
      notifyError("停止任务失败", error, "tree-stop-tasks-error");
      return false;
    } finally {
      currentAction = "";
    }
  }

  function buildActiveTaskPreview(tasks: TaskRecord[]): string {
    const titles = tasks.slice(0, 3).map((task) => `「${task.title}」`);
    const preview = titles.join("、");
    if (tasks.length > 3) {
      return `如 ${preview} 等`;
    }
    return preview;
  }

  async function ensureDeleteReady(rootIds: string[], hardDelete: boolean): Promise<boolean> {
    if (rootIds.length === 0) return false;
    const activeTasks = listActiveTasksForRoots(rootIds);
    if (activeTasks.length === 0) return true;

    const modeLabel = hardDelete ? "硬删除" : "删除（归档）";
    const preview = buildActiveTaskPreview(activeTasks);
    const confirmed = window.confirm(
      `检测到 ${activeTasks.length} 个任务仍在进行中或已暂停（${preview}）。` +
        `删除前需要先停止这些任务。是否先停止并继续${modeLabel}？`
    );
    if (!confirmed) return false;
    return await stopTasks(activeTasks.map((task) => task.id));
  }

  function hasSelectedAncestor(
    taskId: string,
    selectedIds: Set<string>,
    map: Map<string, TaskRecord>
  ): boolean {
    let parentId = map.get(taskId)?.parent_id ?? null;
    while (parentId) {
      if (selectedIds.has(parentId)) return true;
      parentId = map.get(parentId)?.parent_id ?? null;
    }
    return false;
  }

  function flattenTaskRows(
    roots: TaskRecord[],
    childrenMap: Map<string, TaskRecord[]>,
    expandedIds: Set<string>,
    respectExpanded: boolean,
    includeIds: Set<string> | null,
    primaryIds: Set<string> | null,
    matchIds: Set<string> | null
  ): VisibleTaskRow[] {
    const rows: VisibleTaskRow[] = [];
    const visited = new Set<string>();

    const visit = (task: TaskRecord, depth: number) => {
      if (visited.has(task.id)) return;
      visited.add(task.id);
      const children = childrenMap.get(task.id) ?? [];
      const visibleChildren = includeIds
        ? children.filter((child) => includeIds.has(child.id))
        : children;
      if (!includeIds || includeIds.has(task.id)) {
        rows.push({
          task,
          depth,
          hasChildren: visibleChildren.length > 0,
          visibleChildCount: visibleChildren.length,
          isMatch: matchIds?.has(task.id) ?? false,
          isPathOnly: !!primaryIds && !primaryIds.has(task.id),
        });
      }
      if (respectExpanded && !expandedIds.has(task.id)) return;
      for (const child of visibleChildren) {
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

  <section class="panel selection-strip">
    <div>
      {#if selectedTask}
        <p class="selected-title" title={selectedTask.title}>{selectedTask.title}</p>
        <p class="selected-meta">
          <span>{statusLabel(selectedTask.status)}</span>
          <span class="selected-path" title={selectedTaskPath || "-"}>路径 {selectedTaskPathCompact || "-"}</span>
          <span>Ex {formatSeconds(taskLiveExclusiveSeconds(selectedTask))}</span>
        </p>
      {:else}
        <p class="selected-title">未选中任务</p>
        <p class="selected-meta">在下方任务树选择一个任务</p>
      {/if}
    </div>
    <div class="selection-actions">
      <button type="button" class="action-btn action-primary" onclick={onPrimarySelectedToggle} disabled={!selectedTask || !!currentAction}>
        {selectedTask ? primaryActionLabel(selectedTask) : "开始"}
      </button>
      <button
        type="button"
        class="action-btn action-secondary"
        onclick={onCompleteSelected}
        disabled={!selectedTask || selectedTask.status === "stopped" || !!currentAction}
      >
        {selectedTask?.parent_id ? "完成分支" : "完成待办"}
      </button>
      <details class="more-actions-wrapper">
        <summary class="action-btn action-subtle">
          更多操作
        </summary>
        <div class="more-actions-menu">
          <button type="button" class="secondary" onclick={() => window.location.href = '/'}>
            打开详情页
          </button>
          <button type="button" class="secondary" onclick={onStopSelected} disabled={!selectedTask || !!currentAction}>
            停止
          </button>
          <button
            type="button"
            class={batchMode ? "danger active" : "subtle-danger"}
            onclick={toggleBatchMode}
            disabled={!overview?.tasks.length || !!currentAction}
          >
            {batchMode ? "退出批量删除" : "批量删除"}
          </button>
          <button
            type="button"
            class="subtle-danger"
            onclick={onArchiveSelected}
            disabled={!selectedTask || !!currentAction}
          >
            删除（归档）
          </button>
          <button
            type="button"
            class="danger"
            onclick={onHardDeleteSelected}
            disabled={!selectedTask || !!currentAction}
          >
            删除（硬）
          </button>
        </div>
      </details>
    </div>
  </section>

  <section class="panel tree-panel">
    <div class="panel-head">
      <h2>任务树</h2>
      <span>{treeSummaryText()}</span>
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
        placeholder="搜索当前视图任务..."
        bind:value={treeQuery}
        disabled={!overview?.tasks.length}
        aria-label="搜索任务"
      />
      <div class="tree-filter" aria-label="任务完成状态筛选">
        <button
          type="button"
          class:active={treeCompletionFilter === "open"}
          onclick={() => (treeCompletionFilter = "open")}
        >
          未完成
        </button>
        <button
          type="button"
          class:active={treeCompletionFilter === "all"}
          onclick={() => (treeCompletionFilter = "all")}
        >
          全部
        </button>
        <button
          type="button"
          class:active={treeCompletionFilter === "done"}
          onclick={() => (treeCompletionFilter = "done")}
        >
          已完成
        </button>
      </div>
      <div class="tree-tools">
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
    </div>

    {#if selectedTask && !selectedTaskIsVisible}
      <div class="tree-notice">
        <span>选中任务不在当前视图中</span>
        <button type="button" class="subtle" onclick={revealSelectedTask}>定位</button>
      </div>
    {/if}

    {#if batchMode}
      <div class="batch-toolbar">
        <p>已选 {batchSelectedCount} 项，预计影响 {batchAffectedCount} 个节点</p>
        <select bind:value={batchDeleteMode} disabled={!!currentAction}>
          <option value="archive">软删除（归档）</option>
          <option value="hard">硬删除（彻底移除）</option>
        </select>
        <button
          type="button"
          class={batchDeleteMode === "hard" ? "danger" : "subtle-danger"}
          onclick={onBatchDelete}
          disabled={!!currentAction || batchSelectedCount === 0}
        >
          执行批量删除
        </button>
        <button type="button" class="secondary" onclick={clearBatchSelection} disabled={!!currentAction}>
          清空选择
        </button>
      </div>
    {/if}

    {#if !overview || overview.tasks.length === 0}
      <p class="empty">当前暂无任务。</p>
    {:else if displayRows.length === 0}
      <p class="empty">{emptyTreeMessage()}</p>
    {:else}
      <div class="tree-frame scroll-hint">
        <ul class="tree-list" role="tree" aria-label="任务树">
          {#each displayRows as row (row.task.id)}
            <li class="tree-item">
              <div
                class="tree-row"
                class:batch-mode={batchMode}
                class:selected={selectedTaskId === row.task.id}
                class:active-ancestor={activePathIds.has(row.task.id) && activeTaskId !== row.task.id}
                class:active-leaf={activeTaskId === row.task.id}
                class:completed={row.task.status === "stopped"}
                class:match={row.isMatch}
                class:path-only={row.isPathOnly}
                style={`--depth:${row.depth}`}
              >
                {#if batchMode}
                  <label class="row-check" aria-label="加入批量删除">
                    <input
                      type="checkbox"
                      checked={batchSelectedTaskIds.has(row.task.id)}
                      onchange={(event) =>
                        onBatchRowChecked(
                          row.task.id,
                          (event.currentTarget as HTMLInputElement).checked
                        )}
                      onclick={(event) => event.stopPropagation()}
                      disabled={!!currentAction}
                    />
                  </label>
                {/if}

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
                  title={`${row.task.title}\n${statusLabel(row.task.status)} · Ex ${formatSeconds(taskLiveExclusiveSeconds(row.task))} · In ${formatSeconds(taskLiveInclusiveSeconds(row.task))}`}
                >
                  <span
                    class="status-dot"
                    class:running={row.task.status === "running"}
                    class:paused={row.task.status === "paused"}
                    class:stopped={row.task.status === "stopped"}
                    aria-hidden="true"
                  ></span>
                  <span class="row-copy">
                    <span class="title-line">
                      <span class="title">{row.task.title}</span>
                      {#if row.isPathOnly}
                        <span class="path-note">路径</span>
                      {/if}
                    </span>
                    <span class="row-meta">{rowMetaText(row)}</span>
                  </span>
                </button>

                {#if !batchMode && !row.isPathOnly}
                  <button
                    type="button"
                    class="row-quick"
                    onclick={(event) => onTaskQuickToggle(event, row.task)}
                    disabled={!!currentAction}
                    title={taskQuickActionLabel(row.task)}
                  >
                    {taskQuickActionSymbol(row.task)}
                  </button>
                {/if}
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

  .selection-strip > div:first-child {
    min-width: 0;
    flex: 1;
  }

  .selected-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 700;
    color: #112d4e;
    line-height: 1.3;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .selected-meta {
    margin: 0.2rem 0 0;
    color: #4f6f95;
    font-size: 0.84rem;
    line-height: 1.35;
    display: flex;
    align-items: baseline;
    gap: 0.38rem;
    min-width: 0;
    flex-wrap: wrap;
  }

  .selected-path {
    max-width: min(100%, 64ch);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .selection-actions {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
  }

  .selection-actions > button,
  .selection-actions > details > summary.action-btn {
    min-width: 6.8rem;
  }

  .more-actions-wrapper {
    position: relative;
  }

  .more-actions-wrapper summary {
    list-style: none;
  }

  .more-actions-wrapper summary.action-btn {
    border-radius: 0.62rem;
    cursor: pointer;
    padding: 0.5rem 0.72rem;
    user-select: none;
  }

  .more-actions-wrapper summary::-webkit-details-marker {
    display: none;
  }

  .more-actions-wrapper[open] summary {
    background: #e7edf5;
  }

  .more-actions-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 0.35rem);
    min-width: 160px;
    background: #fff;
    border: 1px solid #ced9e8;
    border-radius: 0.66rem;
    box-shadow: 0 10px 24px rgba(17, 36, 63, 0.16);
    padding: 0.35rem;
    display: flex;
    flex-direction: column;
    gap: 0.28rem;
    z-index: 5;
  }

  .more-actions-menu button {
    width: 100%;
    display: flex;
    justify-content: flex-start;
    text-align: left;
  }

  .more-actions-menu button.active {
    background: #8b2a2a;
    color: #fff;
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

  .quick-row button {
    min-width: 6.8rem;
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
    grid-template-columns: minmax(0, 1fr) auto auto;
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

  .tree-filter {
    display: grid;
    grid-template-columns: repeat(3, auto);
    border: 1px solid #d0d7de;
    border-radius: 0.5rem;
    overflow: hidden;
    background: #ffffff;
  }

  .tree-filter button {
    border: none;
    border-right: 1px solid #d0d7de;
    border-radius: 0;
    background: #ffffff;
    color: #374151;
    padding: 0.4rem 0.58rem;
    min-width: 4.1rem;
    font-size: 0.8rem;
  }

  .tree-filter button:last-child {
    border-right: none;
  }

  .tree-filter button.active {
    background: #e7f0ff;
    color: #204f85;
    font-weight: 700;
  }

  .tree-tools {
    display: flex;
    justify-content: flex-end;
    gap: 0.35rem;
  }

  .tree-toolbar .subtle {
    border-color: #d0d7de;
    background: #ffffff;
    color: #374151;
    padding: 0.4rem 0.52rem;
    min-width: 4rem;
    font-size: 0.8rem;
  }

  .tree-notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.6rem;
    border: 1px solid #d5dde7;
    border-radius: 0.56rem;
    background: #f8fafc;
    color: #536273;
    padding: 0.46rem 0.55rem;
    font-size: 0.82rem;
  }

  .tree-notice .subtle {
    flex: 0 0 auto;
    border: 1px solid #d0d7de;
    background: #ffffff;
    color: #374151;
    padding: 0.32rem 0.56rem;
    min-width: 3.4rem;
  }

  .batch-toolbar {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto auto;
    gap: 0.4rem;
    align-items: center;
    border: 1px dashed #c7d6ea;
    border-radius: 0.62rem;
    background: #f7fbff;
    padding: 0.5rem 0.55rem;
  }

  .batch-toolbar p {
    margin: 0;
    color: #3f6087;
    font-size: 0.82rem;
    min-width: 0;
  }

  .batch-toolbar select {
    border: 1px solid #d0d7de;
    border-radius: 0.5rem;
    background: #fff;
    color: #2f3437;
    padding: 0.39rem 0.48rem;
    min-width: 0;
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

  .tree-row.batch-mode {
    grid-template-columns: auto 1.25rem minmax(0, 1fr);
  }

  .row-check {
    display: grid;
    place-items: center;
    width: 1.2rem;
    height: 1.2rem;
    margin: 0;
  }

  .row-check input {
    margin: 0;
    min-width: unset;
    width: 0.86rem;
    height: 0.86rem;
    cursor: pointer;
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
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    column-gap: 0.42rem;
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

  .tree-row.match .row-main {
    background: #fff5cc;
    color: #1f2937;
  }

  .tree-row.selected.match .row-main {
    background: #dfeaff;
  }

  .tree-row.completed .row-main {
    color: #6d7d90;
  }

  .tree-row.completed .title {
    text-decoration: line-through;
  }

  .tree-row.path-only:not(.selected) .row-main {
    color: #7b8794;
  }

  .tree-row.path-only .title {
    font-weight: 400;
    text-decoration: none;
  }

  .tree-row.path-only .row-meta {
    display: none;
  }

  .status-dot {
    width: 0.46rem;
    height: 0.46rem;
    border-radius: 999px;
    background: #94a3b8;
    box-shadow: 0 0 0 3px rgba(148, 163, 184, 0.12);
  }

  .status-dot.running {
    background: #2f855a;
    box-shadow: 0 0 0 3px rgba(47, 133, 90, 0.14);
  }

  .status-dot.paused {
    background: #b7791f;
    box-shadow: 0 0 0 3px rgba(183, 121, 31, 0.14);
  }

  .status-dot.stopped {
    background: #9ca3af;
    box-shadow: none;
  }

  .row-copy {
    display: grid;
    min-width: 0;
    gap: 0.08rem;
  }

  .title-line {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    min-width: 0;
  }

  .title-line .title {
    display: block;
    flex: 1;
    min-width: 0;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row-meta {
    min-width: 0;
    color: #6b7280;
    font-size: 0.72rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .path-note {
    flex: 0 0 auto;
    border: 1px solid #d5dde7;
    border-radius: 999px;
    color: #6b7280;
    background: #f6f8fb;
    padding: 0.02rem 0.34rem;
    font-size: 0.68rem;
    line-height: 1.35;
  }

  .row-quick {
    border: none;
    background: #f1f4f8;
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
    color: #1f2937;
    background: #e7edf5;
    opacity: 1;
    pointer-events: auto;
  }

  .row-quick:disabled {
    opacity: 0.5;
    cursor: not-allowed;
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

  input[type="checkbox"] {
    min-width: unset;
    width: 0.92rem;
    height: 0.92rem;
    padding: 0;
    margin: 0;
    accent-color: #2f629f;
    border-radius: 0.22rem;
  }

  button {
    border: none;
    border-radius: 0.62rem;
    background: #2f629f;
    color: #fff;
    padding: 0.5rem 0.72rem;
    font-size: 0.86rem;
    cursor: pointer;
    transition: background 120ms ease, color 120ms ease, opacity 120ms ease;
  }

  .action-btn {
    display: inline-flex;
    justify-content: center;
    align-items: center;
    text-align: center;
    min-width: 7.2rem;
  }

  .action-primary {
    background: #2f629f;
    color: #fff;
  }

  .action-primary:hover:not(:disabled) {
    background: #28578f;
  }

  .action-secondary {
    background: #edf3ff;
    color: #2f5688;
  }

  .action-secondary:hover:not(:disabled) {
    background: #e2ecff;
  }

  .action-subtle {
    background: #f1f4f8;
    color: #374151;
  }

  .action-subtle:hover:not(:disabled) {
    background: #e7edf5;
  }

  button.secondary {
    background: #edf3ff;
    color: #2f629f;
  }

  button.danger {
    background: #8b2a2a;
  }

  button.subtle-danger {
    background: #ffe9e9;
    color: #7f1f1f;
  }

  button:disabled,
  input:disabled,
  select:disabled {
    opacity: 0.56;
    cursor: not-allowed;
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
      grid-template-columns: 1fr;
    }

    .tree-filter {
      grid-template-columns: repeat(3, 1fr);
      width: 100%;
    }

    .tree-filter button {
      min-width: 0;
    }

    .tree-tools {
      justify-content: stretch;
      width: 100%;
    }

    .tree-tools .subtle {
      flex: 1;
      min-width: 0;
    }

    .batch-toolbar {
      grid-template-columns: 1fr 1fr;
    }

    .tree-frame {
      max-height: 60vh;
      min-height: 260px;
    }
  }
</style>



