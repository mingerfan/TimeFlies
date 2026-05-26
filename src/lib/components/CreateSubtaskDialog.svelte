<script lang="ts">
  import { tick } from "svelte";

  let {
    open = false,
    parentTitle = "",
    busy = false,
    oncancel,
    onsubmit,
  }: {
    open?: boolean;
    parentTitle?: string;
    busy?: boolean;
    oncancel?: () => void;
    onsubmit?: (title: string) => void | Promise<void>;
  } = $props();

  let title = $state("");
  let errorMessage = $state("");
  let titleInput = $state<HTMLInputElement | undefined>();

  const trimmedTitle = $derived(title.trim());

  $effect(() => {
    if (!open) {
      title = "";
      errorMessage = "";
      return;
    }

    void tick().then(() => titleInput?.focus());
  });

  function closeDialog() {
    if (busy) return;
    oncancel?.();
  }

  async function submitDialog(event: SubmitEvent) {
    event.preventDefault();
    if (!trimmedTitle) {
      errorMessage = "请输入子任务标题";
      titleInput?.focus();
      return;
    }

    errorMessage = "";
    await onsubmit?.(trimmedTitle);
  }

  function onDialogKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    closeDialog();
  }
</script>

{#if open}
  <div class="dialog-layer" role="presentation">
    <button
      type="button"
      class="dialog-backdrop"
      aria-label="关闭新增子任务弹窗"
      onclick={closeDialog}
      disabled={busy}
    ></button>

    <div
      class="subtask-dialog"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="create-subtask-title"
      aria-describedby={errorMessage ? "create-subtask-error" : undefined}
      onkeydown={onDialogKeydown}
    >
      <form class="dialog-form" onsubmit={submitDialog}>
        <header class="dialog-head">
          <p class="eyebrow">新增子任务</p>
          <h2 id="create-subtask-title">给当前任务添加下一步</h2>
          <p class="parent-title" title={parentTitle}>{parentTitle || "未选择任务"}</p>
        </header>

        <label class="field">
          <span>子任务标题</span>
          <input
            bind:this={titleInput}
            type="text"
            bind:value={title}
            placeholder="输入子任务标题"
            autocomplete="off"
            disabled={busy}
          />
        </label>

        {#if errorMessage}
          <p class="field-error" id="create-subtask-error">{errorMessage}</p>
        {/if}

        <div class="dialog-actions">
          <button type="button" class="secondary" onclick={closeDialog} disabled={busy}>取消</button>
          <button type="submit" class="primary" disabled={busy || !trimmedTitle}>
            {busy ? "创建中..." : "创建"}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}

<style>
  .dialog-layer {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    place-items: center;
    padding: 1rem;
  }

  .dialog-backdrop {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    border: none;
    border-radius: 0;
    background: rgba(15, 29, 49, 0.42);
    cursor: default;
    padding: 0;
  }

  .dialog-backdrop:disabled {
    opacity: 1;
  }

  .subtask-dialog {
    position: relative;
    width: min(100%, 430px);
    border: 1px solid rgba(91, 122, 165, 0.28);
    border-radius: 8px;
    background: #ffffff;
    box-shadow: 0 24px 70px rgba(17, 36, 63, 0.26);
    color: #16263c;
  }

  .dialog-form {
    display: flex;
    flex-direction: column;
    gap: 0.86rem;
    padding: 1rem;
  }

  .dialog-head {
    display: grid;
    gap: 0.22rem;
  }

  .eyebrow {
    margin: 0;
    color: #426998;
    font-size: 0.72rem;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  h2 {
    margin: 0;
    color: #102b4a;
    font-size: 1.05rem;
    line-height: 1.28;
  }

  .parent-title {
    margin: 0;
    color: #60758f;
    font-size: 0.86rem;
    line-height: 1.35;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .field {
    display: grid;
    gap: 0.38rem;
  }

  .field span {
    color: #354963;
    font-size: 0.82rem;
    font-weight: 700;
  }

  .field input {
    width: 100%;
    min-width: 0;
    border: 1px solid #b8c7da;
    border-radius: 7px;
    background: #fbfdff;
    color: #16263c;
    font: inherit;
    font-size: 0.92rem;
    padding: 0.62rem 0.7rem;
    outline: none;
    transition: border-color 120ms ease, box-shadow 120ms ease, background 120ms ease;
  }

  .field input:focus {
    border-color: #2f629f;
    background: #ffffff;
    box-shadow: 0 0 0 3px rgba(47, 98, 159, 0.16);
  }

  .field-error {
    margin: -0.4rem 0 0;
    color: #8a2a2a;
    font-size: 0.8rem;
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    padding-top: 0.1rem;
  }

  .dialog-actions button {
    min-width: 5.6rem;
    border: none;
    border-radius: 7px;
    font: inherit;
    font-size: 0.88rem;
    font-weight: 700;
    padding: 0.56rem 0.8rem;
    cursor: pointer;
    transition: background 120ms ease, color 120ms ease, opacity 120ms ease;
  }

  .dialog-actions button:disabled,
  .field input:disabled {
    cursor: not-allowed;
    opacity: 0.58;
  }

  .dialog-actions .secondary {
    background: #eef3f9;
    color: #334b68;
  }

  .dialog-actions .secondary:hover:not(:disabled),
  .dialog-actions .secondary:focus-visible {
    background: #e2eaf5;
    outline: none;
  }

  .dialog-actions .primary {
    background: #2f629f;
    color: #ffffff;
  }

  .dialog-actions .primary:hover:not(:disabled),
  .dialog-actions .primary:focus-visible {
    background: #28578f;
    outline: none;
  }

  @media (max-width: 520px) {
    .dialog-form {
      padding: 0.9rem;
    }

    .dialog-actions {
      display: grid;
      grid-template-columns: 1fr 1fr;
    }

    .dialog-actions button {
      min-width: 0;
    }
  }
</style>
