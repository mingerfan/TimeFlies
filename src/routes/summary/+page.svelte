<script lang="ts">
  import {
    getOverview,
    respondRestSuggestion,
    type OverviewRange,
    type OverviewResponse,
  } from "$lib/api";
  import {
    formatDate,
    formatDeviation,
    formatSeconds,
    normalizeError,
    restHeadline,
    restTriggerLabel,
  } from "$lib/ui";

  let overview = $state<OverviewResponse | null>(null);
  let range = $state<OverviewRange>("week");
  let loading = $state(false);
  let currentAction = $state("");
  let errorMessage = $state("");

  const restSuggestion = $derived.by(() => overview?.rest_suggestion ?? null);

  const topByExclusive = $derived.by(() =>
    [...(overview?.tasks ?? [])]
      .sort((a, b) => b.exclusive_seconds - a.exclusive_seconds)
      .slice(0, 10)
  );

  const topByInclusive = $derived.by(() =>
    [...(overview?.tasks ?? [])]
      .sort((a, b) => b.inclusive_seconds - a.inclusive_seconds)
      .slice(0, 10)
  );

  $effect(() => {
    const selectedRange = range;
    void refresh(selectedRange);
  });

  async function refresh(targetRange: OverviewRange = range) {
    loading = true;
    errorMessage = "";
    try {
      overview = await getOverview(targetRange);
    } catch (error) {
      errorMessage = normalizeError(error);
    } finally {
      loading = false;
    }
  }

  async function runAction(label: string, action: () => Promise<void>) {
    currentAction = label;
    errorMessage = "";
    try {
      await action();
      await refresh();
    } catch (error) {
      errorMessage = normalizeError(error);
    } finally {
      currentAction = "";
    }
  }

  async function onRespondRestSuggestion(accept: boolean) {
    const suggestion = restSuggestion;
    if (!suggestion) return;
    await runAction(accept ? "接受休息建议" : "忽略休息建议", () =>
      respondRestSuggestion(suggestion.id, accept)
    );
  }
</script>

<main class="summary-screen">
  <header class="page-head">
    <div>
      <p class="eyebrow">统计页面</p>
      <h1>统计摘要</h1>
      <p class="sub">二级查看面板，不干扰任务执行主流程</p>
    </div>
    <div class="range-switch">
      <button type="button" class:active={range === "all"} onclick={() => (range = "all")}>全部</button>
      <button type="button" class:active={range === "week"} onclick={() => (range = "week")}>近 7 天</button>
      <button type="button" class:active={range === "day"} onclick={() => (range = "day")}>近 24 小时</button>
    </div>
  </header>

  {#if errorMessage}
    <p class="error">{errorMessage}</p>
  {/if}

  <section class="summary-grid">
    <article class="panel">
      <div class="panel-head">
        <h2>自适应休息建议</h2>
        {#if loading}
          <span>刷新中...</span>
        {/if}
      </div>
      {#if restSuggestion}
        <p class="headline">{restHeadline(restSuggestion)}</p>
        <p class="meta">
          触发点 {restTriggerLabel(restSuggestion.trigger_type)} · 连续专注
          {formatSeconds(restSuggestion.focus_seconds)} · 30 分钟切换
          {restSuggestion.switch_count_30m} 次 · 偏差 {formatDeviation(restSuggestion.deviation_ratio)}
        </p>
        <p class="meta">创建时间 {formatDate(restSuggestion.created_at)}</p>
        <ul class="reasons">
          {#each restSuggestion.reasons as reason}
            <li>{reason}</li>
          {/each}
        </ul>
        <div class="actions">
          <button type="button" onclick={() => onRespondRestSuggestion(true)} disabled={!!currentAction}>
            接受
          </button>
          <button
            type="button"
            class="secondary"
            onclick={() => onRespondRestSuggestion(false)}
            disabled={!!currentAction}
          >
            忽略
          </button>
        </div>
      {:else}
        <p class="empty">暂无待处理建议。建议会在任务切换或子任务结束后生成。</p>
      {/if}
    </article>

    <article class="panel">
      <div class="panel-head">
        <h2>Top Exclusive</h2>
        <span>{topByExclusive.length} 项</span>
      </div>
      {#if topByExclusive.length === 0}
        <p class="empty">暂无统计数据</p>
      {:else}
        <ol class="rank-list">
          {#each topByExclusive as task}
            <li>
              <span>{task.title}</span>
              <span>{formatSeconds(task.exclusive_seconds)}</span>
            </li>
          {/each}
        </ol>
      {/if}
    </article>

    <article class="panel">
      <div class="panel-head">
        <h2>Top Inclusive</h2>
        <span>{topByInclusive.length} 项</span>
      </div>
      {#if topByInclusive.length === 0}
        <p class="empty">暂无统计数据</p>
      {:else}
        <ol class="rank-list">
          {#each topByInclusive as task}
            <li>
              <span>{task.title}</span>
              <span>{formatSeconds(task.inclusive_seconds)}</span>
            </li>
          {/each}
        </ol>
      {/if}
    </article>
  </section>
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

  .summary-grid {
    display: grid;
    grid-template-columns: 1.1fr 1fr 1fr;
    gap: 0.95rem;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .panel {
    background: rgba(255, 255, 255, 0.88);
    border: 1px solid rgba(65, 97, 143, 0.28);
    border-radius: 1rem;
    padding: 0.95rem;
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
    min-height: 230px;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.55rem;
  }

  .panel-head h2 {
    margin: 0;
    font-size: 1rem;
  }

  .panel-head span {
    font-size: 0.8rem;
    color: #4f6f95;
  }

  .headline {
    margin: 0;
    font-size: 1.04rem;
    font-weight: 700;
    color: #173a65;
  }

  .meta,
  .empty {
    margin: 0;
    color: #4e6f95;
    font-size: 0.86rem;
    line-height: 1.34;
  }

  .reasons {
    margin: 0.1rem 0 0;
    padding-left: 1.1rem;
    color: #3e618c;
    font-size: 0.84rem;
    display: flex;
    flex-direction: column;
    gap: 0.28rem;
  }

  .rank-list {
    margin: 0;
    padding-left: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .rank-list li {
    display: flex;
    justify-content: space-between;
    gap: 0.8rem;
    font-size: 0.9rem;
  }

  .actions {
    display: flex;
    gap: 0.45rem;
  }

  button {
    border: 1px solid #2f629f;
    border-radius: 0.62rem;
    background: #2f629f;
    color: #fff;
    padding: 0.44rem 0.68rem;
    cursor: pointer;
    font: inherit;
  }

  button.secondary {
    border-color: #2f629f;
    background: #f2f7ff;
    color: #2f629f;
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
    .summary-screen {
      height: auto;
      min-height: 100%;
      overflow: visible;
    }

    .summary-grid {
      flex: 0 0 auto;
      min-height: fit-content;
      overflow: visible;
    }

    .panel {
      overflow: visible;
    }
  }

  @media (max-width: 1220px) {
    .summary-grid {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 760px) {
    .page-head {
      flex-direction: column;
    }

    .range-switch {
      width: 100%;
      min-width: unset;
    }
  }
</style>

