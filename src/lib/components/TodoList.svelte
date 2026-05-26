<script lang="ts">
  import { completeTaskTree, createTask, type TaskRecord } from "$lib/api";
  import { notifyError } from "$lib/notifications";
  import {
    buildSubtreeRecentActivityMap,
    compareTasksByRecentActivity,
    formatSeconds,
    statusLabel,
  } from "$lib/ui";

  let {
    tasks = [],
    selectedTaskId = null,
    busy = false,
    onselect,
  }: {
    tasks?: TaskRecord[];
    selectedTaskId?: string | null;
    busy?: boolean;
    onselect?: (taskId: string) => void;
  } = $props();

  let draftTitle = $state("");
  let pendingTaskId = $state<string | null>(null);

  const subtreeRecentActivityMap = $derived.by(() => buildSubtreeRecentActivityMap(tasks));
  const rootTodos = $derived.by(() =>
    tasks
      .filter((task) => !task.parent_id && task.status !== "stopped")
      .sort((a, b) => compareTasksByRecentActivity(a, b, subtreeRecentActivityMap))
  );
  const disabled = $derived(busy || !!pendingTaskId);

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
