<script lang="ts">
  import type { CommandFeedbackTone } from "$lib/command/executor";

  let {
    value = $bindable(""),
    busy = false,
    feedback = "",
    tone = "success",
    onexecute,
  }: {
    value?: string;
    busy?: boolean;
    feedback?: string;
    tone?: CommandFeedbackTone;
    onexecute?: (input: string) => void | Promise<void>;
  } = $props();

  function onSubmit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    void onexecute?.(value);
  }

  function onInputKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    value = "";
  }
</script>

<section class="command-bar">
  <form class="command-form" onsubmit={onSubmit}>
    <input
      type="text"
      bind:value={value}
      onkeydown={onInputKeydown}
      placeholder="输入命令（/rename /parent /start /pause /resume /stop /sub）或直接输入任务标题"
      autocomplete="off"
      spellcheck="false"
      disabled={busy}
      aria-label="命令输入"
    />
    <button type="submit" disabled={busy || !value.trim()}>{busy ? "执行中..." : "执行"}</button>
  </form>
  <p class="command-hint">Enter 执行，Esc 清空。支持 `#tag` 语法。</p>
  {#if feedback}
    <p class="command-feedback" class:success={tone === "success"} class:error={tone === "error"} class:warning={tone === "warning"}>
      {feedback}
    </p>
  {/if}
</section>

<style>
  .command-bar {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  .command-form {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.45rem;
    align-items: center;
  }

  .command-form input,
  .command-form button {
    font: inherit;
  }

  .command-form input {
    min-width: 0;
    border-radius: 0.64rem;
    border: 1px solid #8cafd7;
    padding: 0.52rem 0.64rem;
    background: #fff;
  }

  .command-form button {
    border: 1px solid #2f629f;
    border-radius: 0.62rem;
    background: #2f629f;
    color: #fff;
    padding: 0.5rem 0.74rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .command-form button:disabled,
  .command-form input:disabled {
    opacity: 0.56;
    cursor: not-allowed;
  }

  .command-hint {
    margin: 0;
    font-size: 0.78rem;
    color: #4c6f96;
  }

  .command-feedback {
    margin: 0;
    border-radius: 0.62rem;
    border: 1px solid #95b1d5;
    background: #edf4ff;
    color: #1f436d;
    padding: 0.45rem 0.6rem;
    font-size: 0.85rem;
    line-height: 1.35;
  }

  .command-feedback.success {
    border-color: #8bbf97;
    background: #ecf8ef;
    color: #215730;
  }

  .command-feedback.warning {
    border-color: #d9b46f;
    background: #fff8eb;
    color: #765515;
  }

  .command-feedback.error {
    border-color: #cb7474;
    background: #ffeded;
    color: #7f1a1a;
  }
</style>
