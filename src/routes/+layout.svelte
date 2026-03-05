<script lang="ts">
  import { afterNavigate } from "$app/navigation";
  import { page } from "$app/stores";
  import {
    APP_DATA_CHANGED_EVENT,
    getOverview,
    respondRestSuggestion,
    type OverviewResponse,
  } from "$lib/api";
  import NotificationHub from "$lib/components/NotificationHub.svelte";
  import {
    dismissByDedupeKey,
    notifyError,
    pushNotification,
  } from "$lib/notifications";
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
  const REST_SUGGESTION_KEY = "rest-suggestion:pending";

  const restSuggestion = $derived.by(() => sidebarOverview?.rest_suggestion ?? null);

  $effect(() => {
    const suggestion = restSuggestion;
    if (!suggestion) {
      dismissByDedupeKey(REST_SUGGESTION_KEY);
      return;
    }

    pushNotification({
      kind: "rest-suggestion",
      level: "info",
      dedupeKey: REST_SUGGESTION_KEY,
      autoCloseMs: null,
      title: restHeadline(suggestion),
      message:
        `${restTriggerLabel(suggestion.trigger_type)} · 连续专注 ${formatSeconds(suggestion.focus_seconds)}` +
        ` · 30 分钟切换 ${suggestion.switch_count_30m} 次 · 偏差 ${formatDeviation(suggestion.deviation_ratio)}`,
      detail: suggestion.reasons[0],
      actions: [
        {
          label: sidebarAction === "接受休息建议" ? "处理中..." : "接受",
          run: () => onRespondRestSuggestion(true),
          variant: "primary",
        },
        {
          label: sidebarAction === "忽略休息建议" ? "处理中..." : "忽略",
          run: () => onRespondRestSuggestion(false),
          variant: "secondary",
        },
      ],
    });
  });

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
    } catch (error) {
      notifyError(`${label}失败`, error, "rest-suggestion-action-error");
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

  <NotificationHub />
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

