<script lang="ts">
  import {
    completeTaskTree,
    createTask,
    deleteTasks,
    pauseTask,
    resumeTask,
    startTask,
    type TaskRecord,
  } from "$lib/api";
  import { notifyError, pushNotification } from "$lib/notifications";
  import {
    buildSubtreeRecentActivityMap,
    compareTasksByRecentActivity,
    formatSeconds,
    statusLabel,
  } from "$lib/ui";
  import { onMount } from "svelte";

  type TodoContextMenu = {
    taskId: string;
    x: number;
    y: number;
  };

  let {
    tasks = [],
    selectedTaskId = null,
    activeTaskId = null,
    busy = false,
    onselect,
  }: {
    tasks?: TaskRecord[];
    selectedTaskId?: string | null;
    activeTaskId?: string | null;
    busy?: boolean;
    onselect?: (taskId: string) => void;
  } = $props();

  let draftTitle = $state("");
  let pendingTaskId = $state<string | null>(null);
  let todoContextMenu = $state<TodoContextMenu | null>(null);

  const subtreeRecentActivityMap = $derived.by(() => buildSubtreeRecentActivityMap(tasks));
  const taskMap = $derived.by(() => {
    const map = new Map<string, TaskRecord>();
    for (const task of tasks) {
      map.set(task.id, task);
    }
    return map;
  });
  const rootTodos = $derived.by(() =>
    tasks
      .filter((task) => !task.parent_id && task.status !== "stopped")
      .sort((a, b) => compareTasksByRecentActivity(a, b, subtreeRecentActivityMap))
  );
  const activeTask = $derived.by(() =>
    activeTaskId ? (taskMap.get(activeTaskId) ?? null) : null
  );
  const contextMenuTask = $derived.by(() =>
    todoContextMenu ? (taskMap.get(todoContextMenu.taskId) ?? null) : null
  );
  const disabled = $derived(busy || !!pendingTaskId);

  onMount(() => {
    const onDocumentClick = () => closeTodoContextMenu();
    const onDocumentKeydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeTodoContextMenu();
    };
    window.addEventListener("click", onDocumentClick);
    window.addEventListener("keydown", onDocumentKeydown);
    window.addEventListener("resize", closeTodoContextMenu);
    return () => {
      window.removeEventListener("click", onDocumentClick);
      window.removeEventListener("keydown", onDocumentKeydown);
      window.removeEventListener("resize", closeTodoContextMenu);
    };
  });

  async function onAddTodo(event: SubmitEvent) {
    event.preventDefault();
    const title = draftTitle.trim();
    if (!title || disabled) return;

    pendingTaskId = "new";
    try {
      const createdTaskId = await createTask(title, null);
      draftTitle = "";
      onselect?.(createdTaskId);
    } catch (error) {
      notifyError("新增待办失败", error, "todo-create-root-error");
    } finally {
      pendingTaskId = null;
    }
  }

  async function onCompleteTodo(event: Event, task: TaskRecord) {
    const checkbox = event.currentTarget as HTMLInputElement;
    checkbox.checked = false;
    await completeTodo(task);
  }

  async function completeTodo(task: TaskRecord) {
    if (pendingTaskId) return;

    pendingTaskId = task.id;
    try {
      await completeTaskTree(task.id);
    } catch (error) {
      notifyError("完成待办失败", error, `todo-complete-root-error:${task.id}`);
    } finally {
      pendingTaskId = null;
    }
  }

  function onSelectTask(taskId: string) {
    onselect?.(taskId);
  }

  function openTodoContextMenu(event: MouseEvent, task: TaskRecord) {
    event.preventDefault();
    event.stopPropagation();
    onselect?.(task.id);
    const menuWidth = 220;
    const menuHeight = 290;
    const margin = 8;
    todoContextMenu = {
      taskId: task.id,
      x: Math.max(margin, Math.min(event.clientX, window.innerWidth - menuWidth - margin)),
      y: Math.max(margin, Math.min(event.clientY, window.innerHeight - menuHeight - margin)),
    };
  }

  function closeTodoContextMenu() {
    todoContextMenu = null;
  }

  async function ensureSwitchFromActive(targetTaskId: string): Promise<boolean> {
    if (!activeTask || activeTask.id === targetTaskId || activeTask.status !== "running") {
      return true;
    }
    await pauseTask(activeTask.id);
    return true;
  }

  async function toggleTodoTask(task: TaskRecord) {
    if (pendingTaskId) return;
    pendingTaskId = task.id;
    try {
      onselect?.(task.id);
      if (task.status === "running") {
        await pauseTask(task.id);
        return;
      }
      await ensureSwitchFromActive(task.id);
      if (task.status === "paused") {
        await resumeTask(task.id);
        return;
      }
      await startTask(task.id);
    } catch (error) {
      notifyError("切换待办状态失败", error, `todo-toggle-error:${task.id}`);
    } finally {
      pendingTaskId = null;
    }
  }

  async function onContextPrimaryAction() {
    const task = contextMenuTask;
    if (!task) return;
    closeTodoContextMenu();
    await toggleTodoTask(task);
  }

  async function onContextCompleteTodo() {
    const task = contextMenuTask;
    if (!task) return;
    closeTodoContextMenu();
    await completeTodo(task);
  }

  async function onContextCreateSubtask() {
    const task = contextMenuTask;
    if (!task || pendingTaskId) return;
    closeTodoContextMenu();
    const title = window.prompt(`给「${task.title}」新增子任务`);
    const trimmedTitle = title?.trim();
    if (!trimmedTitle) return;

    pendingTaskId = task.id;
    try {
      const childId = await createTask(trimmedTitle, task.id);
      onselect?.(childId);
    } catch (error) {
      notifyError("新增子任务失败", error, `todo-create-child-error:${task.id}`);
    } finally {
      pendingTaskId = null;
    }
  }

  async function onContextCopyTitle() {
    const task = contextMenuTask;
    if (!task) return;
    closeTodoContextMenu();
    try {
      await navigator.clipboard.writeText(task.title);
      pushNotification({
        kind: "system",
        level: "success",
        title: "已复制待办标题",
        message: task.title,
        dedupeKey: "todo-copy-title",
      });
    } catch (error) {
      notifyError("复制待办标题失败", error, "todo-copy-title-error");
    }
  }

  function onContextOpenTree() {
    const task = contextMenuTask;
    if (!task) return;
    onselect?.(task.id);
    closeTodoContextMenu();
    window.location.href = "/tree";
  }

  async function onContextArchiveTodo() {
    const task = contextMenuTask;
    if (!task || pendingTaskId) return;
    closeTodoContextMenu();
    const confirmed = window.confirm(`确认归档待办「${task.title}」及其全部子任务吗？`);
    if (!confirmed) return;

    pendingTaskId = task.id;
    try {
      await deleteTasks([task.id], false);
    } catch (error) {
      notifyError("归档待办失败", error, `todo-archive-error:${task.id}`);
    } finally {
      pendingTaskId = null;
    }
  }

  function contextPrimaryLabel(task: TaskRecord): string {
    if (task.status === "running") return "暂停";
    if (task.status === "paused") return "恢复";
    return "开始";
  }
</script>

<section class="todo-list" aria-label="根任务待办">
  <div class="todo-head">
    <h2>待办</h2>
    <span>{rootTodos.length}</span>
  </div>

  <form class="todo-add" onsubmit={onAddTodo}>
    <input
      type="text"
      bind:value={draftTitle}
      placeholder="新增根待办"
      disabled={disabled}
      autocomplete="off"
    />
    <button type="submit" disabled={disabled || !draftTitle.trim()} title="新增待办">+</button>
  </form>

  <div class="todo-scroll scroll-hint">
    {#if rootTodos.length === 0}
      <p class="todo-empty">暂无未完成根任务</p>
    {:else}
      <ul class="todo-items">
        {#each rootTodos as task (task.id)}
          <li
            class="todo-row"
            class:selected={selectedTaskId === task.id}
            class:running={task.status === "running"}
            class:paused={task.status === "paused"}
            class:pending={pendingTaskId === task.id}
            class:context-open={todoContextMenu?.taskId === task.id}
            role="listitem"
            oncontextmenu={(event) => openTodoContextMenu(event, task)}
          >
            <input
              type="checkbox"
              checked={false}
              disabled={busy || !!pendingTaskId}
              aria-label={`完成 ${task.title}`}
              onchange={(event) => void onCompleteTodo(event, task)}
            />
            <button type="button" class="todo-main" onclick={() => onSelectTask(task.id)} title={task.title}>
              <span class="todo-title">{task.title}</span>
              <span class="todo-meta">
                {statusLabel(task.status)} · Ex {formatSeconds(task.exclusive_seconds)}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  {#if todoContextMenu && contextMenuTask}
    <div
      class="todo-context-menu"
      role="menu"
      tabindex="-1"
      aria-label={`待办操作：${contextMenuTask.title}`}
      style={`left:${todoContextMenu.x}px;top:${todoContextMenu.y}px`}
      oncontextmenu={(event) => event.preventDefault()}
    >
      <div class="context-menu-head">
        <span class="context-menu-title" title={contextMenuTask.title}>{contextMenuTask.title}</span>
        <span>{statusLabel(contextMenuTask.status)}</span>
      </div>
      <button type="button" role="menuitem" onclick={() => void onContextPrimaryAction()} disabled={disabled}>
        {contextPrimaryLabel(contextMenuTask)}
      </button>
      <button type="button" role="menuitem" onclick={() => void onContextCompleteTodo()} disabled={disabled}>
        完成待办
      </button>
      <div class="context-separator"></div>
      <button type="button" role="menuitem" onclick={() => void onContextCreateSubtask()} disabled={disabled}>
        新增子任务
      </button>
      <button
        type="button"
        role="menuitem"
        onclick={() => {
          onSelectTask(contextMenuTask.id);
          closeTodoContextMenu();
        }}
      >
        设为操控目标
      </button>
      <div class="context-separator"></div>
      <button type="button" role="menuitem" onclick={() => void onContextCopyTitle()}>
        复制标题
      </button>
      <button type="button" role="menuitem" onclick={onContextOpenTree}>
        打开完整任务树
      </button>
      <div class="context-separator"></div>
      <button type="button" role="menuitem" class="context-danger" onclick={() => void onContextArchiveTodo()} disabled={disabled}>
        删除（归档）
      </button>
    </div>
  {/if}
</section>

<style>
  .todo-list {
    display: flex;
    flex-direction: column;
    gap: 0.52rem;
    min-height: 0;
    overflow: hidden;
    border-right: 1px solid #d8dee4;
    padding-right: 0.72rem;
  }

  .todo-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
  }

  h2 {
    margin: 0;
    font-size: 0.96rem;
    color: #102b4a;
  }

  .todo-head span {
    color: #5b7496;
    font-size: 0.78rem;
  }

  .todo-add {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 2rem;
    gap: 0.34rem;
  }

  .todo-add input {
    min-width: 0;
    border: 1px solid #b6c7dc;
    border-radius: 0.52rem;
    background: #fff;
    color: #1f344d;
    font: inherit;
    font-size: 0.84rem;
    padding: 0.42rem 0.5rem;
  }

  .todo-add button,
  .todo-main {
    font: inherit;
  }

  .todo-add button {
    border: none;
    border-radius: 0.52rem;
    background: #2f629f;
    color: #fff;
    cursor: pointer;
    display: grid;
    place-items: center;
    min-width: 0;
    padding: 0;
    font-weight: 700;
    line-height: 1;
  }

  .todo-scroll {
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
    padding-right: 0.12rem;
  }

  .todo-empty {
    margin: 0;
    color: #5a7190;
    font-size: 0.84rem;
    line-height: 1.35;
  }

  .todo-items {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
  }

  .todo-row {
    display: grid;
    grid-template-columns: 1.05rem minmax(0, 1fr);
    gap: 0.34rem;
    align-items: center;
    min-height: 2.4rem;
    border-radius: 0.5rem;
    padding: 0.16rem 0.18rem;
  }

  .todo-row:hover,
  .todo-row:focus-within {
    background: #f4f7fb;
  }

  .todo-row.selected {
    background: #e8eef6;
  }

  .todo-row.context-open {
    background: #dfeaff;
  }

  .todo-row.running .todo-title {
    color: #123e6e;
  }

  .todo-row.paused .todo-title {
    color: #3f5d82;
  }

  .todo-row.pending {
    opacity: 0.58;
  }

  .todo-row input[type="checkbox"] {
    width: 0.95rem;
    height: 0.95rem;
    margin: 0;
    accent-color: #2f629f;
  }

  .todo-main {
    min-width: 0;
    border: none;
    border-radius: 0.45rem;
    background: transparent;
    color: #20364f;
    cursor: pointer;
    padding: 0.3rem 0.22rem;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 0.12rem;
  }

  .todo-title,
  .todo-meta {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .todo-title {
    font-size: 0.86rem;
    font-weight: 700;
    line-height: 1.25;
  }

  .todo-meta {
    color: #657a95;
    font-size: 0.74rem;
    line-height: 1.3;
  }

  .todo-context-menu {
    position: fixed;
    z-index: 30;
    width: 220px;
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

  .todo-context-menu button {
    width: 100%;
    display: flex;
    justify-content: flex-start;
    align-items: center;
    border: none;
    border-radius: 0.42rem;
    background: transparent;
    color: #26364a;
    padding: 0.42rem 0.48rem;
    text-align: left;
    font: inherit;
    font-size: 0.82rem;
    cursor: pointer;
  }

  .todo-context-menu button:hover:not(:disabled),
  .todo-context-menu button:focus-visible {
    background: #eef4ff;
    color: #1f4f92;
    outline: none;
  }

  .todo-context-menu button.context-danger {
    color: #8a2a2a;
  }

  .todo-context-menu button.context-danger:hover:not(:disabled),
  .todo-context-menu button.context-danger:focus-visible {
    background: #fff0f0;
    color: #7f1f1f;
  }

  .context-separator {
    height: 1px;
    background: #edf1f6;
    margin: 0.14rem 0;
  }

  button:disabled,
  input:disabled {
    cursor: not-allowed;
    opacity: 0.56;
  }

  @media (max-width: 760px) {
    .todo-list {
      border-right: none;
      border-bottom: 1px solid #d8dee4;
      padding-right: 0;
      padding-bottom: 0.72rem;
      max-height: 18rem;
    }
  }
</style>
