<script lang="ts">
  import { afterNavigate } from "$app/navigation";
  import { page } from "$app/stores";
  import { getOverview, pauseTask, stopTask, type OverviewResponse, type TaskRecord } from "$lib/api";
  import { buildTaskChain, formatSeconds, normalizeError, statusLabel } from "$lib/ui";
  import { onMount } from "svelte";

  let { children } = $props();

  let sidebarOverview = $state<OverviewResponse | null>(null);
  let sidebarLoading = $state(false);
  let sidebarAction = $state("");
  let sidebarError = $state("");

  const taskMap = $derived.by(() => {
    const map = new Map<string, TaskRecord>();
    for (const task of sidebarOverview?.tasks ?? []) {
      map.set(task.id, task);
    }
    return map;
  });

  const activeTask = $derived.by(() =>
    sidebarOverview?.active_task_id ? (taskMap.get(sidebarOverview.active_task_id) ?? null) : null
  );

  const activeTaskPath = $derived.by(() =>
    buildTaskChain(activeTask?.id ?? null, taskMap)
      .map((task) => task.title)
      .join(" / ")
  );

  afterNavigate(() => {
    void refreshSidebar();
  });

  onMount(() => {
    void refreshSidebar();
    const timer = window.setInterval(() => void refreshSidebar(), 30_000);
    return () => window.clearInterval(timer);
  });

  async function refreshSidebar() {
    if (sidebarLoading || !!sidebarAction) return;
    sidebarLoading = true;
    sidebarError = "";
    try {
      sidebarOverview = await getOverview("day");
    } catch (error) {
      sidebarError = normalizeError(error);
    } finally {
      sidebarLoading = false;
    }
  }

  async function runSidebarAction(label: string, action: () => Promise<void>) {
    sidebarAction = label;
    sidebarError = "";
    try {
      await action();
      await refreshSidebar();
    } catch (error) {
      sidebarError = normalizeError(error);
    } finally {
      sidebarAction = "";
    }
  }
</script>

<main class="app-shell">
  <aside class="sidebar">
    <div class="brand">
      <p class="app-name">TimeFlies</p>
      <p class="title">任务时间台</p>
    </div>

    <nav class="nav">
      <a href="/" class:active={$page.url.pathname === "/"}>任务详情</a>
      <a href="/tree" class:active={$page.url.pathname === "/tree"}>任务树</a>
      <a href="/timer" class:active={$page.url.pathname === "/timer"}>计时器</a>
      <a href="/summary" class:active={$page.url.pathname === "/summary"}>统计摘要</a>
    </nav>

    <section class="mini-timer">
      <div class="mini-head">
        <p>当前会话</p>
        {#if sidebarLoading}
          <span>刷新中...</span>
        {/if}
      </div>

      {#if activeTask}
        <p class="active-name">{activeTask.title}</p>
        <p class="active-path">{activeTaskPath}</p>
        <p class="active-meta">
          {statusLabel(activeTask.status)} · 累计 {formatSeconds(activeTask.exclusive_seconds)}
        </p>
        <div class="mini-actions">
          <button
            type="button"
            onclick={() => runSidebarAction("暂停任务", () => pauseTask(activeTask.id))}
            disabled={!!sidebarAction || activeTask.status !== "running"}
          >
            暂停
          </button>
          <button
            type="button"
            class="danger"
            onclick={() => runSidebarAction("停止任务", () => stopTask(activeTask.id))}
            disabled={!!sidebarAction || activeTask.status !== "running"}
          >
            停止
          </button>
        </div>
      {:else}
        <p class="empty">暂无进行中的任务</p>
      {/if}

      {#if sidebarError}
        <p class="error">{sidebarError}</p>
      {/if}
    </section>
  </aside>

  <section class="content">
    {@render children()}
  </section>
</main>

<style>
  :global(html),
  :global(body) {
    height: 100%;
    margin: 0;
    font-family: "IBM Plex Sans", "Noto Sans SC", "Segoe UI", sans-serif;
    color: #11223a;
    background:
      radial-gradient(1200px 600px at 8% -5%, #faf3e7 0%, transparent 60%),
      radial-gradient(900px 500px at 110% 5%, #deecff 0%, transparent 60%),
      #eef2f8;
    overflow: hidden;
  }

  :global(body > div) {
    height: 100%;
  }

  :global(*) {
    box-sizing: border-box;
  }

  .app-shell {
    height: 100dvh;
    min-height: 0;
    display: grid;
    grid-template-columns: 220px 1fr;
    overflow: hidden;
  }

  .sidebar {
    background: linear-gradient(180deg, #f6f0e2 0%, #edf3ff 100%);
    border-right: 1px solid rgba(20, 46, 85, 0.16);
    padding: 1rem 0.85rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
    scrollbar-width: none;
    -ms-overflow-style: none;
  }

  .sidebar::-webkit-scrollbar {
    width: 0;
    height: 0;
    display: none;
  }

  .brand {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .app-name {
    margin: 0;
    font-size: 1.08rem;
    font-weight: 800;
    letter-spacing: 0.03em;
    color: #163052;
  }

  .title {
    margin: 0;
    font-size: 0.86rem;
    font-weight: 600;
    color: #32557f;
  }

  .nav {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .nav a {
    text-decoration: none;
    color: #26466d;
    border: 1px solid transparent;
    border-radius: 0.65rem;
    padding: 0.52rem 0.62rem;
    font-size: 0.92rem;
  }

  .nav a.active {
    border-color: #20497e;
    background: #dbe8ff;
    color: #15365f;
    font-weight: 600;
  }

  .mini-timer {
    margin-top: auto;
    border: 1px solid rgba(52, 88, 136, 0.35);
    background: rgba(255, 255, 255, 0.78);
    border-radius: 0.85rem;
    padding: 0.72rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .mini-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.4rem;
  }

  .mini-head p {
    margin: 0;
    font-size: 0.83rem;
    color: #2f5078;
  }

  .mini-head span {
    font-size: 0.75rem;
    color: #587aa6;
  }

  .active-name {
    margin: 0;
    font-weight: 700;
    color: #0f2c4e;
    line-height: 1.3;
  }

  .active-path,
  .active-meta {
    margin: 0;
    color: #446488;
    font-size: 0.79rem;
    line-height: 1.35;
  }

  .mini-actions {
    display: flex;
    gap: 0.4rem;
  }

  .mini-actions button {
    flex: 1;
    border-radius: 0.55rem;
    border: 1px solid #2e609a;
    background: #2e609a;
    color: #fff;
    padding: 0.44rem 0.5rem;
    font-size: 0.82rem;
    cursor: pointer;
  }

  .mini-actions button.danger {
    border-color: #8b2a2a;
    background: #8b2a2a;
  }

  .mini-actions button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .empty {
    margin: 0;
    color: #55769b;
    font-size: 0.83rem;
  }

  .error {
    margin: 0;
    font-size: 0.78rem;
    color: #8a2222;
    background: #feecec;
    border: 1px solid #d88383;
    border-radius: 0.55rem;
    padding: 0.38rem 0.45rem;
  }

  .content {
    padding: 1.1rem 1.2rem;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .content :global(main) {
    flex: 1;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }

  @media (max-width: 980px) {
    .app-shell {
      grid-template-columns: 1fr;
      grid-template-rows: auto minmax(0, 1fr);
    }

    .sidebar {
      border-right: none;
      border-bottom: 1px solid rgba(20, 46, 85, 0.16);
      margin-bottom: 0.75rem;
    }

    .nav {
      flex-direction: row;
      flex-wrap: wrap;
    }

    .nav a {
      flex: 1 1 120px;
      text-align: center;
    }
  }

  @media (max-height: 700px) {
    .content {
      overflow: auto;
    }

    .content :global(main) {
      flex: 0 0 auto;
      min-height: 100%;
      overflow: visible;
    }
  }
</style>

