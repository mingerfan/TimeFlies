<script lang="ts">
  import {
    APP_DATA_CHANGED_EVENT,
    getFocusSummary,
    type FocusSummaryRange,
    type FocusSummaryResponse,
    type FocusTimelineSegment,
  } from "$lib/api";
  import { notifyError } from "$lib/notifications";
  import { formatSeconds, formatShareRatio } from "$lib/ui";
  import { onMount } from "svelte";

  const SECONDS_PER_DAY = 24 * 60 * 60;
  const axisLabels = Array.from({ length: 12 }, (_, index) => ({
    hour: index * 2,
    line: index * 2 + 2,
  }));
  const timelinePalette = [
    "#5e88c8",
    "#4f9f84",
    "#d19a57",
    "#8771c2",
    "#cc7171",
    "#5299b1",
    "#8ea956",
    "#bd7397",
  ] as const;

  let summary = $state<FocusSummaryResponse | null>(null);
  let range = $state<FocusSummaryRange>("7d");
  let loading = $state(false);
  let hideEmptyDays = $state(false);

  const days = $derived.by(() => {
    const source = summary?.days ?? [];
    if (!hideEmptyDays) return source;
    return source.filter((day) => day.total_focus_seconds > 0);
  });

  const legendItems = $derived.by(() => {
    const totals = new Map<string, { taskId: string; title: string; totalSeconds: number }>();
    for (const day of days) {
      for (const task of day.tasks) {
        const current = totals.get(task.task_id);
        if (current) {
          current.totalSeconds += task.exclusive_seconds;
          continue;
        }
        totals.set(task.task_id, {
          taskId: task.task_id,
          title: task.title,
          totalSeconds: task.exclusive_seconds,
        });
      }
    }
    return [...totals.values()].sort((left, right) => right.totalSeconds - left.totalSeconds);
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

  function hashTaskId(taskId: string): number {
    let hash = 0;
    for (let index = 0; index < taskId.length; index += 1) {
      hash = (hash * 31 + taskId.charCodeAt(index)) | 0;
    }
    return Math.abs(hash);
  }

  function taskColor(taskId: string): string {
    return timelinePalette[hashTaskId(taskId) % timelinePalette.length];
  }

  function formatHourLabel(hour: number): string {
    return hour.toString().padStart(2, "0");
  }

  function formatDayLabel(dayStartTs: number): string {
    return new Date(dayStartTs * 1000).toLocaleDateString("zh-CN", {
      month: "numeric",
      day: "numeric",
    });
  }

  function formatWeekday(dayStartTs: number): string {
    return new Date(dayStartTs * 1000).toLocaleDateString("zh-CN", {
      weekday: "short",
    });
  }

  function buildSegmentStyle(segment: FocusTimelineSegment): string {
    const left = (segment.start_offset_seconds / SECONDS_PER_DAY) * 100;
    const width = (segment.duration_seconds / SECONDS_PER_DAY) * 100;
    const color = taskColor(segment.task_id);
    return `left: ${left}%; width: ${width}%; --segment-color: ${color};`;
  }

  function buildSegmentTitle(segment: FocusTimelineSegment): string {
    return `${segment.title} · ${formatTime(segment.start_ts)} - ${formatTime(segment.end_ts)} · ${formatSeconds(segment.duration_seconds)}`;
  }

  function formatTime(unixSeconds: number): string {
    return new Date(unixSeconds * 1000).toLocaleTimeString("zh-CN", {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    });
  }

  function buildTaskBarStyle(taskId: string, shareRatio: number): string {
    const color = taskColor(taskId);
    const width = shareRatio <= 0 ? 0 : Math.max(shareRatio * 100, 2);
    return `width: ${width}%; background: ${color};`;
  }
</script>

<main class="summary-screen scroll-hint">
  <header class="page-head">
    <div>
      <p class="eyebrow">统计页面</p>
      <h1>每日专注时间线</h1>
      <p class="sub">按自然日展示专注时段分布，不同任务使用稳定颜色区分。</p>
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
    <div class="summary-stack">
      <section class="panel timeline-panel">
        <div class="panel-head">
          <div>
            <h2>每日时间线</h2>
            <p>按小时查看每天的任务分布。</p>
          </div>
        </div>

        <div class="timeline-layout">
          <aside class="legend-panel" aria-label="任务标签表">
            <div class="legend-head">
              <span>任务标签</span>
              <span>累计时长</span>
            </div>
            <div class="legend-list scroll-hint">
              {#each legendItems as item (item.taskId)}
                <div class="legend-row">
                  <span class="legend-task">
                    <span
                      class="legend-color"
                      aria-hidden="true"
                      style={`--segment-color: ${taskColor(item.taskId)};`}
                    ></span>
                    <span class="legend-title">{item.title}</span>
                  </span>
                  <span class="legend-value">{formatSeconds(item.totalSeconds)}</span>
                </div>
              {/each}
            </div>
          </aside>

          <div class="timeline-table">
            <div class="timeline-axis-grid" aria-hidden="true">
              <div class="axis-meta"></div>
              <div class="axis-track"></div>
              {#each axisLabels as label (label.hour)}
                <span class="axis-label" style={`grid-column: ${label.line} / span 1;`}>
                  {formatHourLabel(label.hour)}
                </span>
              {/each}
            </div>

            <div class="timeline-rows scroll-hint">
              {#each days as day (day.date_key)}
                <article class="timeline-row-grid">
                  <div class="day-meta">
                    <p class="day-date">{formatDayLabel(day.day_start_ts)}</p>
                    <p class="day-weekday">{formatWeekday(day.day_start_ts)}</p>
                    <p class="day-total">{formatSeconds(day.total_focus_seconds)}</p>
                  </div>

                  <div
                    class:empty-track={day.timeline_segments.length === 0}
                    class="timeline-canvas"
                    aria-label={`${day.date_key} 专注时间线`}
                  >
                    {#if day.timeline_segments.length === 0}
                      <span class="row-empty">无记录</span>
                    {:else}
                      {#each day.timeline_segments as segment, index (`${segment.task_id}-${segment.start_ts}-${index}`)}
                        <span
                          class="timeline-segment"
                          aria-label={buildSegmentTitle(segment)}
                          style={buildSegmentStyle(segment)}
                          title={buildSegmentTitle(segment)}
                        ></span>
                      {/each}
                    {/if}
                  </div>
                </article>
              {/each}
            </div>
          </div>
        </div>
      </section>

      <section class="panel summary-panel">
        <div class="panel-head">
          <div>
            <h2>每日专注摘要</h2>
            <p>保留原先的按天任务占比展示。</p>
          </div>
        </div>

        <div class="summary-grid">
          {#each days as day (day.date_key)}
            <article class="day-card">
              <div class="day-head">
                <div>
                  <p class="day-date">{formatDayLabel(day.day_start_ts)}</p>
                  <p class="day-weekday">{formatWeekday(day.day_start_ts)}</p>
                </div>
                <div class="day-total-box">
                  <span>总专注</span>
                  <strong>{formatSeconds(day.total_focus_seconds)}</strong>
                </div>
              </div>

              {#if day.tasks.length === 0}
                <p class="empty">这一天没有专注记录。</p>
              {:else}
                <ol class="task-list scroll-hint">
                  {#each day.tasks as task (task.task_id)}
                    <li class="task-row">
                      <div class="task-row-head">
                        <span class="task-title">{task.title}</span>
                        <span class="task-meta">
                          {formatSeconds(task.exclusive_seconds)} · {formatShareRatio(task.share_ratio)}
                        </span>
                      </div>
                      <div class="task-bar" aria-hidden="true">
                        <span style={buildTaskBarStyle(task.task_id, task.share_ratio)}></span>
                      </div>
                    </li>
                  {/each}
                </ol>
              {/if}
            </article>
          {/each}
        </div>
      </section>
    </div>
  {/if}
</main>

<style>
  :global(main.summary-screen) {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    gap: 0.95rem;
    min-height: 100%;
    overflow-y: auto !important;
    overflow-x: hidden !important;
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

  .controls {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.55rem;
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

  .panel {
    background: rgba(252, 253, 255, 0.96);
    border: 1px solid rgba(65, 97, 143, 0.28);
    border-radius: 1rem;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    min-height: 0;
  }

  .empty-panel {
    justify-content: center;
    align-items: center;
  }

  .empty {
    margin: 0;
    color: #6281a6;
    font-size: 0.9rem;
  }

  .timeline-panel {
    overflow: hidden;
    min-height: 29rem;
    max-height: min(36rem, calc(100dvh - 12rem));
  }

  .summary-stack {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding-bottom: 0.1rem;
  }

  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.75rem;
  }

  .panel-head h2,
  .panel-head p {
    margin: 0;
  }

  .panel-head h2 {
    color: #183b62;
    font-size: 1.02rem;
  }

  .panel-head p {
    color: #6683a9;
    font-size: 0.84rem;
    margin-top: 0.15rem;
  }

  .timeline-layout {
    display: grid;
    grid-template-columns: 18rem minmax(0, 1fr);
    gap: 0.85rem;
    flex: 1;
    height: min(28rem, calc(100dvh - 20rem));
    min-height: 0;
    align-items: stretch;
    overflow: hidden;
  }

  .legend-panel {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    border: 1px solid rgba(116, 146, 190, 0.2);
    border-radius: 0.9rem;
    background: #f7faff;
    overflow: hidden;
  }

  .legend-head,
  .legend-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.75rem;
    align-items: center;
  }

  .legend-head {
    padding: 0.85rem 0.85rem 0.7rem;
    border-bottom: 1px solid rgba(116, 146, 190, 0.2);
    color: #5f7ea5;
    font-size: 0.78rem;
    font-weight: 600;
  }

  .legend-list {
    overflow-y: auto;
    min-height: 0;
  }

  .legend-row {
    padding: 0.7rem 0.85rem;
  }

  .legend-row + .legend-row {
    border-top: 1px solid rgba(116, 146, 190, 0.12);
  }

  .legend-task {
    display: inline-flex;
    align-items: center;
    gap: 0.55rem;
    min-width: 0;
  }

  .legend-color {
    width: 0.8rem;
    height: 0.8rem;
    border-radius: 0.18rem;
    flex-shrink: 0;
    background: var(--segment-color);
  }

  .legend-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .legend-value {
    color: #58769c;
    white-space: nowrap;
  }

  .timeline-table {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    border: 1px solid rgba(116, 146, 190, 0.2);
    border-radius: 0.9rem;
    background: #f8fbff;
    overflow: hidden;
  }

  .timeline-axis-grid,
  .timeline-row-grid {
    display: grid;
    grid-template-columns: 8rem repeat(24, minmax(0, 1fr));
  }

  .timeline-axis-grid {
    min-height: 3rem;
    align-items: stretch;
    background: rgba(248, 251, 255, 0.98);
    border-bottom: 1px solid rgba(116, 146, 190, 0.2);
  }

  .axis-meta {
    grid-column: 1;
  }

  .axis-track {
    grid-column: 2 / -1;
  }

  .axis-label {
    align-self: center;
    justify-self: start;
    width: 2ch;
    transform: translateX(-50%);
    color: #6a83a6;
    font-size: 0.78rem;
    line-height: 1;
    text-align: center;
    font-variant-numeric: tabular-nums;
    font-feature-settings: "tnum" 1;
    white-space: nowrap;
  }

  .timeline-rows {
    overflow-y: auto;
    min-height: 0;
  }

  .timeline-row-grid {
    min-height: 3.35rem;
  }

  .timeline-row-grid + .timeline-row-grid {
    border-top: 1px solid rgba(116, 146, 190, 0.16);
  }

  .day-meta {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 0.08rem;
    padding: 0.55rem 0.85rem;
    border-right: 1px solid rgba(116, 146, 190, 0.2);
    background: #f5f9ff;
    grid-column: 1;
  }

  .day-date,
  .day-weekday,
  .day-total,
  .row-empty {
    margin: 0;
  }

  .day-date {
    color: #17365e;
    font-weight: 700;
  }

  .day-weekday {
    color: #6b86ab;
    font-size: 0.76rem;
  }

  .day-total {
    color: #45688f;
    font-size: 0.8rem;
  }

  .timeline-canvas {
    grid-column: 2 / -1;
    position: relative;
    min-width: 0;
    min-height: 3.35rem;
    background:
      repeating-linear-gradient(
        90deg,
        rgba(110, 138, 180, 0.08) 0,
        rgba(110, 138, 180, 0.08) 1px,
        transparent 1px,
        transparent calc(100% / 24)
      ),
      repeating-linear-gradient(
        90deg,
        rgba(92, 124, 169, 0.12) 0,
        rgba(92, 124, 169, 0.12) 1px,
        transparent 1px,
        transparent calc(100% / 12)
      ),
      #fbfdff;
  }

  .timeline-segment {
    position: absolute;
    top: 50%;
    height: 1.4rem;
    min-width: 2px;
    transform: translateY(-50%);
    border-radius: 0.14rem;
    background: var(--segment-color);
  }

  .empty-track {
    display: flex;
    align-items: center;
    padding: 0 0.85rem;
  }

  .row-empty {
    color: #88a0c1;
    font-size: 0.84rem;
  }

  .summary-panel {
    min-height: 22rem;
    overflow: visible;
  }

  .summary-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 0.95rem;
    overflow: visible;
    min-height: 0;
  }

  .day-card {
    border: 1px solid rgba(116, 146, 190, 0.18);
    border-radius: 0.9rem;
    background: #f9fbff;
    padding: 0.95rem;
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
    height: 18rem;
    overflow: hidden;
  }

  .day-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.75rem;
  }

  .day-total-box {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.1rem;
    color: #4b6b92;
  }

  .day-total-box span {
    font-size: 0.78rem;
  }

  .day-total-box strong {
    font-size: 1rem;
    color: #17365e;
  }

  .task-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-right: 0.15rem;
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
    height: 0.46rem;
    border-radius: 0.18rem;
    background: rgba(123, 154, 198, 0.16);
    overflow: hidden;
  }

  .task-bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
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

    .timeline-layout {
      grid-template-columns: 1fr;
      height: auto;
      overflow: visible;
    }

    .legend-panel {
      height: auto;
      min-height: 12rem;
      max-height: none;
    }
  }

  @media (max-width: 640px) {
    .timeline-panel {
      max-height: none;
      overflow: visible;
    }

    .range-switch {
      grid-template-columns: repeat(2, 1fr);
    }

    .timeline-axis-grid,
    .timeline-row-grid {
      grid-template-columns: 5.8rem repeat(24, minmax(0, 1fr));
    }

    .day-meta {
      padding-left: 0.6rem;
      padding-right: 0.6rem;
    }

    .axis-label {
      font-size: 0.7rem;
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
