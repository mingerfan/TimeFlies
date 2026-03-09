<script lang="ts">
  import {
    APP_DATA_CHANGED_EVENT,
    getFocusSummary,
    type FocusSummaryRange,
    type FocusSummaryResponse,
  } from "$lib/api";
  import { notifyError } from "$lib/notifications";
  import { formatDateOnly, formatSeconds, formatShareRatio } from "$lib/ui";
  import { onMount } from "svelte";

  let summary = $state<FocusSummaryResponse | null>(null);
  let range = $state<FocusSummaryRange>("7d");
  let loading = $state(false);
  let hideEmptyDays = $state(false);

  const days = $derived.by(() => {
    const source = summary?.days ?? [];
    if (!hideEmptyDays) return source;
    return source.filter((day) => day.total_focus_seconds > 0);
  });
  const rangeOptions: Array<{ value: FocusSummaryRange; label: string }> = [
    { value: "today", label: "今天" },
    { value: "7d", label: "近 7 天" },
    { value: "30d", label: "近 30 天" },
    { value: "all", label: "全部" },
  ];

  onMount(() => {
    const onDataChanged = () => {
      if (loading) return;
      void refresh();
    };
    window.addEventListener(APP_DATA_CHANGED_EVENT, onDataChanged);
    return () => {
      window.removeEventListener(APP_DATA_CHANGED_EVENT, onDataChanged);
    };
  });

  $effect(() => {
    const selectedRange = range;
    void refresh(selectedRange);
  });

  async function refresh(targetRange: FocusSummaryRange = range) {
    loading = true;
    try {
      summary = await getFocusSummary(targetRange);
    } catch (error) {
      notifyError("刷新统计摘要失败", error, "focus-summary-refresh-error");
    } finally {
      loading = false;
    }
  }

  function taskBarWidth(shareRatio: number): string {
    if (shareRatio <= 0) return "0%";
    return `${Math.max(shareRatio * 100, 2)}%`;
  }
</script>

<main class="summary-screen">
  <header class="page-head">
    <div>
      <p class="eyebrow">统计页面</p>
      <h1>每日专注摘要</h1>
      <p class="sub">按自然日统计专注时长，并拆平展示每个任务的占比。</p>
    </div>
    <div class="controls">
      <div class="range-switch" role="tablist" aria-label="统计范围">
        {#each rangeOptions as option}
          <button
            type="button"
            class:active={range === option.value}
            onclick={() => (range = option.value)}
          >
            {option.label}
          </button>
        {/each}
      </div>
      <label class="filter-toggle">
        <input type="checkbox" bind:checked={hideEmptyDays} />
        <span>隐藏空白日</span>
      </label>
    </div>
  </header>

  {#if loading && !summary}
    <section class="panel empty-panel">
      <p class="empty">正在生成统计摘要...</p>
    </section>
  {:else if days.length === 0}
    <section class="panel empty-panel">
      <p class="empty">当前范围内还没有专注记录。</p>
    </section>
  {:else}
    <section class="summary-timeline">
      {#each days as day (day.date_key)}
        <article class="panel day-card">
          <div class="day-head">
            <div>
              <p class="day-label">{formatDateOnly(day.day_start_ts)}</p>
              <p class="day-key">{day.date_key}</p>
            </div>
            <div class="day-total">
              <span>总专注</span>
              <strong>{formatSeconds(day.total_focus_seconds)}</strong>
            </div>
          </div>

          {#if day.tasks.length === 0}
            <p class="empty">这一天没有专注记录。</p>
          {:else}
            <ol class="task-list">
              {#each day.tasks as task (task.task_id)}
                <li class="task-row">
                  <div class="task-row-head">
                    <span class="task-title">{task.title}</span>
                    <span class="task-meta">
                      {formatSeconds(task.exclusive_seconds)} · {formatShareRatio(task.share_ratio)}
                    </span>
                  </div>
                  <div class="task-bar" aria-hidden="true">
                    <span style={`width: ${taskBarWidth(task.share_ratio)};`}></span>
                  </div>
                </li>
              {/each}
            </ol>
          {/if}
        </article>
      {/each}
    </section>
  {/if}
</main>

<style>
  .summary-screen {
    display: flex;
    flex-direction: column;
    gap: 0.95rem;
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
    margin: 0.15rem 0;
    font-size: clamp(1.6rem, 2.1vw, 2.05rem);
  }

  .sub {
    margin: 0;
    color: #3f5f85;
    font-size: 0.9rem;
  }

  .range-switch {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    border: 1px solid #89a9d4;
    border-radius: 0.78rem;
    overflow: hidden;
    min-width: 360px;
    background: #deebff;
  }

  .controls {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.55rem;
  }

  .filter-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    color: #2a4e7e;
    font-size: 0.88rem;
    user-select: none;
  }

  .filter-toggle input {
    margin: 0;
    accent-color: #1f4f92;
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

  .summary-timeline {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 0.95rem;
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-right: 0.1rem;
  }

  .panel {
    background: rgba(255, 255, 255, 0.88);
    border: 1px solid rgba(65, 97, 143, 0.28);
    border-radius: 1rem;
    padding: 0.95rem;
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
    min-height: 220px;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .empty-panel {
    justify-content: center;
    align-items: center;
  }

  .day-card {
    min-height: 260px;
  }

  .day-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.75rem;
  }

  .day-label,
  .day-key,
  .day-total span,
  .empty {
    margin: 0;
  }

  .day-label {
    font-size: 1.02rem;
    font-weight: 700;
    color: #17365e;
  }

  .day-key {
    color: #58779d;
    font-size: 0.82rem;
  }

  .day-total {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.1rem;
    color: #4b6b92;
  }

  .day-total strong {
    font-size: 1.02rem;
    color: #17365e;
  }

  .task-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }

  .task-row {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .task-row-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.8rem;
  }

  .task-title {
    color: #143457;
    font-weight: 600;
    line-height: 1.35;
  }

  .task-meta {
    color: #52739a;
    font-size: 0.86rem;
    white-space: nowrap;
  }

  .task-bar {
    height: 0.5rem;
    border-radius: 999px;
    background: rgba(123, 154, 198, 0.2);
    overflow: hidden;
  }

  .task-bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, #2d6fbd 0%, #6aa3de 100%);
  }

  .empty {
    color: #6281a6;
    font-size: 0.9rem;
  }

  @media (max-width: 900px) {
    .page-head {
      flex-direction: column;
      align-items: stretch;
    }

    .controls {
      align-items: stretch;
    }

    .range-switch {
      min-width: 0;
    }
  }

  @media (max-width: 640px) {
    .summary-screen {
      overflow: auto;
    }

    .summary-timeline {
      overflow: visible;
    }

    .range-switch {
      grid-template-columns: repeat(2, 1fr);
    }

    .task-row-head {
      flex-direction: column;
      align-items: flex-start;
      gap: 0.2rem;
    }

    .task-meta {
      white-space: normal;
    }
  }
</style>
