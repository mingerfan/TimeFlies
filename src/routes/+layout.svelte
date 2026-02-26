<script lang="ts">
  import { afterNavigate } from "$app/navigation";
  import { page } from "$app/stores";
  import {
    APP_DATA_CHANGED_EVENT,
    getOverview,
    respondRestSuggestion,
    type OverviewResponse,
  } from "$lib/api";
  import {
    formatDeviation,
    formatSeconds,
    restHeadline,
    restTriggerLabel,
  } from "$lib/ui";
  import { onMount } from "svelte";

  let { children } = $props();

  let sidebarOverview = $state<OverviewResponse | null>(null);
  let sidebarLoading = $state(false);
  let sidebarAction = $state("");

  const restSuggestion = $derived.by(() => sidebarOverview?.rest_suggestion ?? null);

  afterNavigate(() => {
    void refreshSidebar();
  });

  onMount(() => {
    void refreshSidebar();
    const timer = window.setInterval(() => void refreshSidebar(), 30_000);
    const onDataChanged = () => void refreshSidebar(true);
    window.addEventListener(APP_DATA_CHANGED_EVENT, onDataChanged);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener(APP_DATA_CHANGED_EVENT, onDataChanged);
    };
  });

  async function refreshSidebar(force = false) {
    if (sidebarLoading || (!force && !!sidebarAction)) return;
    sidebarLoading = true;
    try {
      sidebarOverview = await getOverview("day");
    } catch {
      // Sidebar polling failure should not block the main workspace.
    } finally {
      sidebarLoading = false;
    }
  }

  async function runSidebarAction(label: string, action: () => Promise<void>) {
    sidebarAction = label;
    try {
      await action();
    } catch {
      // Rest suggestion action failures are surfaced on next successful refresh.
    } finally {
      sidebarAction = "";
      await refreshSidebar(true);
    }
  }

  async function onRespondRestSuggestion(accept: boolean) {
    const suggestion = restSuggestion;
    if (!suggestion) return;
    await runSidebarAction(accept ? "接受休息建议" : "忽略休息建议", () =>
      respondRestSuggestion(suggestion.id, accept)
    );
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
      <a href="/docs" class:active={$page.url.pathname === "/docs"}>文档</a>
    </nav>
  </aside>

  <section class="content">
    {@render children()}
  </section>

  {#if restSuggestion}
    <section class="rest-popup" role="status" aria-live="polite">
      <p class="rest-title">{restHeadline(restSuggestion)}</p>
      <p class="rest-detail">
        {restTriggerLabel(restSuggestion.trigger_type)} · 连续专注
        {formatSeconds(restSuggestion.focus_seconds)} · 30 分钟切换 {restSuggestion.switch_count_30m} 次 · 偏差
        {formatDeviation(restSuggestion.deviation_ratio)}
      </p>
      {#if restSuggestion.reasons[0]}
        <p class="rest-reason">{restSuggestion.reasons[0]}</p>
      {/if}
      <div class="rest-actions">
        <button
          type="button"
          onclick={() => onRespondRestSuggestion(true)}
          disabled={!!sidebarAction}
        >
          接受
        </button>
        <button
          type="button"
          class="secondary"
          onclick={() => onRespondRestSuggestion(false)}
          disabled={!!sidebarAction}
        >
          忽略
        </button>
      </div>
    </section>
  {/if}
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
    scrollbar-width: thin;
    scrollbar-color: #5b7faa #d7e3f2;
  }

  :global(*::-webkit-scrollbar) {
    width: 10px;
    height: 10px;
  }

  :global(*::-webkit-scrollbar-track) {
    background: #d7e3f2;
    border-radius: 999px;
  }

  :global(*::-webkit-scrollbar-thumb) {
    background: linear-gradient(180deg, #7e97bb 0%, #5576a7 100%);
    border-radius: 999px;
    border: 1px solid #edf3fc;
    background-clip: padding-box;
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: linear-gradient(180deg, #6b87b0 0%, #446794 100%);
  }

  :global(*::-webkit-scrollbar-corner) {
    background: #d7e3f2;
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

  .content {
    padding: 1.1rem 1.2rem;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .rest-popup {
    position: fixed;
    right: 1.2rem;
    bottom: 1.1rem;
    width: min(420px, calc(100vw - 2rem));
    border: 1px solid #9dbce4;
    border-radius: 0.9rem;
    background: linear-gradient(180deg, #f6faff 0%, #edf4ff 100%);
    box-shadow: 0 10px 30px rgba(31, 69, 116, 0.18);
    padding: 0.75rem 0.85rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    z-index: 50;
  }

  .rest-title {
    margin: 0;
    color: #153a65;
    font-size: 0.95rem;
    font-weight: 700;
  }

  .rest-detail {
    margin: 0;
    color: #486b92;
    font-size: 0.82rem;
    line-height: 1.35;
  }

  .rest-reason {
    margin: 0;
    color: #335981;
    font-size: 0.8rem;
  }

  .rest-actions {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
  }

  .rest-actions button {
    border: 1px solid #2f629f;
    border-radius: 0.58rem;
    background: #2f629f;
    color: #fff;
    padding: 0.42rem 0.64rem;
    cursor: pointer;
    font: inherit;
  }

  .rest-actions button.secondary {
    border-color: #2f629f;
    background: #f2f7ff;
    color: #2f629f;
  }

  .rest-actions button:disabled {
    opacity: 0.56;
    cursor: not-allowed;
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

    .rest-popup {
      right: 0.8rem;
      bottom: 0.8rem;
      width: calc(100vw - 1.6rem);
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

