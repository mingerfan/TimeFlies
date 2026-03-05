<script lang="ts">
  import {
    dismissNotification,
    notifications,
    type NotificationAction,
    type NotificationActionVariant,
  } from "$lib/notifications";

  let busyActionKeys = $state<Set<string>>(new Set());

  async function onAction(id: string, actionIndex: number, action: NotificationAction) {
    const busyKey = `${id}:${actionIndex}`;
    if (busyActionKeys.has(busyKey)) return;
    const nextBusy = new Set(busyActionKeys);
    nextBusy.add(busyKey);
    busyActionKeys = nextBusy;

    try {
      await action.run();
      if (action.closeOnClick ?? false) {
        dismissNotification(id);
      }
    } finally {
      const done = new Set(busyActionKeys);
      done.delete(busyKey);
      busyActionKeys = done;
    }
  }

  function actionClass(variant: NotificationActionVariant | undefined) {
    return variant === "secondary" ? "secondary" : "primary";
  }
</script>

<section class="notification-hub" aria-live="polite" aria-atomic="false">
  {#each $notifications as item (item.id)}
    <article class="notice" class:error={item.level === "error"} class:warning={item.level === "warning"} class:success={item.level === "success"}>
      <div class="notice-head">
        <p class="notice-title">{item.title}</p>
        <button type="button" class="notice-close" onclick={() => dismissNotification(item.id)} aria-label="关闭通知">
          ×
        </button>
      </div>
      {#if item.message}
        <p class="notice-message">{item.message}</p>
      {/if}
      {#if item.detail}
        <p class="notice-detail">{item.detail}</p>
      {/if}
      {#if item.actions && item.actions.length > 0}
        <div class="notice-actions">
          {#each item.actions as action, actionIndex (`${item.id}-${action.label}-${actionIndex}`)}
            {@const busyKey = `${item.id}:${actionIndex}`}
            <button
              type="button"
              class={actionClass(action.variant)}
              onclick={() => onAction(item.id, actionIndex, action)}
              disabled={busyActionKeys.has(busyKey)}
            >
              {busyActionKeys.has(busyKey) ? "处理中..." : action.label}
            </button>
          {/each}
        </div>
      {/if}
    </article>
  {/each}
</section>

<style>
  .notification-hub {
    position: fixed;
    right: 1.2rem;
    bottom: 1.1rem;
    width: min(420px, calc(100vw - 2rem));
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    z-index: 60;
    pointer-events: none;
  }

  .notice {
    pointer-events: auto;
    border: 1px solid #9dbce4;
    border-radius: 0.9rem;
    background: linear-gradient(180deg, #f6faff 0%, #edf4ff 100%);
    box-shadow: 0 10px 30px rgba(31, 69, 116, 0.18);
    padding: 0.68rem 0.78rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .notice.warning {
    border-color: #d9b46f;
    background: #fff8eb;
  }

  .notice.error {
    border-color: #cb7474;
    background: #ffeded;
  }

  .notice.success {
    border-color: #8bbf97;
    background: #ecf8ef;
  }

  .notice-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.6rem;
  }

  .notice-title {
    margin: 0;
    color: #153a65;
    font-size: 0.9rem;
    font-weight: 700;
    line-height: 1.35;
  }

  .notice-message,
  .notice-detail {
    margin: 0;
    color: #2f547e;
    font-size: 0.8rem;
    line-height: 1.35;
    white-space: pre-wrap;
  }

  .notice-actions {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
  }

  .notice-actions button {
    border-radius: 0.58rem;
    padding: 0.42rem 0.64rem;
    cursor: pointer;
    font: inherit;
    border: 1px solid #2f629f;
  }

  .notice-actions button.primary {
    background: #2f629f;
    color: #fff;
  }

  .notice-actions button.secondary {
    background: #f2f7ff;
    color: #2f629f;
  }

  .notice-actions button:disabled {
    opacity: 0.56;
    cursor: not-allowed;
  }

  .notice-close {
    border: none;
    background: transparent;
    color: #4a6b91;
    font-size: 1rem;
    line-height: 1;
    cursor: pointer;
    padding: 0.1rem 0.2rem;
  }

  @media (max-width: 980px) {
    .notification-hub {
      right: 0.8rem;
      bottom: 0.8rem;
      width: calc(100vw - 1.6rem);
    }
  }
</style>
