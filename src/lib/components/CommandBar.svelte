<script lang="ts">
  import type { TaskRecord } from "$lib/api";
  import type { CommandFeedbackTone } from "$lib/command/executor";
  import { COMMAND_NAMES } from "$lib/command/parser";
  import { buildTaskChain } from "$lib/ui";
  import { onMount, tick } from "svelte";

  type SuggestionKind = "command" | "parent" | "tag";

  type Suggestion = {
    id: string;
    kind: SuggestionKind;
    label: string;
    meta?: string;
    insertText: string;
    replaceStart: number;
    replaceEnd: number;
  };

  let {
    value = $bindable(""),
    busy = false,
    feedback = "",
    tone = "success",
    tasks = [],
    historyKey = "timeflies:command-history",
    onexecute,
  }: {
    value?: string;
    busy?: boolean;
    feedback?: string;
    tone?: CommandFeedbackTone;
    tasks?: TaskRecord[];
    historyKey?: string;
    onexecute?: (input: string) => void | Promise<void>;
  } = $props();

  let inputEl = $state<HTMLInputElement | null>(null);
  let caretIndex = $state(0);
  let suggestions = $state<Suggestion[]>([]);
  let activeSuggestionIndex = $state(0);
  let suggestionSignature = $state("");
  let history = $state<string[]>([]);
  let historyCursor = $state<number | null>(null);
  let historyDraft = $state("");

  const taskMap = $derived.by(() => {
    const map = new Map<string, TaskRecord>();
    for (const task of tasks) {
      map.set(task.id, task);
    }
    return map;
  });

  const titleCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const task of tasks) {
      const key = task.title.toLowerCase();
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return counts;
  });

  const tagStats = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const task of tasks) {
      for (const tag of task.tags) {
        counts.set(tag, (counts.get(tag) ?? 0) + 1);
      }
    }
    const tags = [...counts.entries()]
      .sort((left, right) => {
        if (right[1] !== left[1]) return right[1] - left[1];
        return left[0].localeCompare(right[0]);
      })
      .map(([tag]) => tag);
    return { counts, tags };
  });

  const suggestionsOpen = $derived.by(() => suggestions.length > 0);

  onMount(() => {
    history = loadHistory();
  });

  $effect(() => {
    if (busy) {
      suggestions = [];
      return;
    }
    const { items, signature } = buildSuggestions(value, caretIndex);
    suggestions = items;
    if (signature !== suggestionSignature) {
      suggestionSignature = signature;
      activeSuggestionIndex = 0;
    } else if (activeSuggestionIndex >= items.length) {
      activeSuggestionIndex = 0;
    }
  });

  function onSubmit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    const trimmed = value.trim();
    if (trimmed) {
      pushHistory(trimmed);
      historyCursor = null;
      historyDraft = "";
    }
    void onexecute?.(value);
  }

  function onInput(event: Event) {
    if (event.currentTarget !== inputEl) return;
    syncCaret();
    historyCursor = null;
  }

  function onInputClick() {
    syncCaret();
  }

  function onInputKeyup() {
    syncCaret();
  }

  function onInputKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      if (suggestionsOpen) {
        event.preventDefault();
        suggestions = [];
        return;
      }
      value = "";
      historyCursor = null;
      return;
    }

    if (suggestionsOpen) {
      if (event.key === "Tab" && event.shiftKey) {
        event.preventDefault();
        activeSuggestionIndex =
          (activeSuggestionIndex - 1 + suggestions.length) % suggestions.length;
        return;
      }
      if (event.key === "Tab") {
        event.preventDefault();
        void applySuggestion(activeSuggestionIndex);
        return;
      }
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      recallHistory(-1);
      return;
    }

    if (event.key === "ArrowDown") {
      event.preventDefault();
      recallHistory(1);
    }
  }

  function syncCaret() {
    caretIndex = inputEl?.selectionStart ?? value.length;
  }

  async function applySuggestion(index: number) {
    const item = suggestions[index];
    if (!item) return;
    const nextValue = replaceRange(value, item.replaceStart, item.replaceEnd, item.insertText);
    const caretPos = item.replaceStart + item.insertText.length;
    await updateValue(nextValue, caretPos);
  }

  async function updateValue(nextValue: string, caretPos: number) {
    value = nextValue;
    await tick();
    if (!inputEl) return;
    inputEl.focus();
    inputEl.setSelectionRange(caretPos, caretPos);
    caretIndex = caretPos;
  }

  function recallHistory(direction: number) {
    if (history.length === 0) return;

    if (direction < 0) {
      if (historyCursor === null) {
        historyDraft = value;
        historyCursor = history.length - 1;
      } else {
        historyCursor = Math.max(0, historyCursor - 1);
      }
      const nextValue = history[historyCursor];
      void updateValue(nextValue, nextValue.length);
      return;
    }

    if (historyCursor === null) return;
    if (historyCursor < history.length - 1) {
      historyCursor += 1;
      const nextValue = history[historyCursor];
      void updateValue(nextValue, nextValue.length);
      return;
    }

    historyCursor = null;
    if (historyDraft) {
      void updateValue(historyDraft, historyDraft.length);
    } else {
      void updateValue("", 0);
    }
  }

  function loadHistory(): string[] {
    if (typeof window === "undefined") return [];
    try {
      const raw = window.localStorage.getItem(historyKey);
      if (!raw) return [];
      const parsed = JSON.parse(raw);
      if (!Array.isArray(parsed)) return [];
      return parsed.filter((item) => typeof item === "string");
    } catch {
      return [];
    }
  }

  function pushHistory(entry: string) {
    const current = history[history.length - 1];
    if (current === entry) return;
    const next = [...history, entry];
    if (next.length > 50) {
      next.splice(0, next.length - 50);
    }
    history = next;
    if (typeof window === "undefined") return;
    window.localStorage.setItem(historyKey, JSON.stringify(next));
  }

  function buildSuggestions(raw: string, caret: number): { items: Suggestion[]; signature: string } {
    const tagContext = findTagContext(raw, caret);
    if (tagContext) {
      const items = buildTagSuggestions(tagContext, raw, caret);
      return { items, signature: `tag:${items.map((item) => item.id).join("|")}` };
    }

    const commandContext = findCommandContext(raw, caret);
    if (!commandContext) {
      return { items: [], signature: "" };
    }

    if (commandContext.mode === "command") {
      const items = buildCommandSuggestions(commandContext);
      return { items, signature: `command:${items.map((item) => item.id).join("|")}` };
    }

    const items = buildParentSuggestions(commandContext);
    return { items, signature: `parent:${items.map((item) => item.id).join("|")}` };
  }

  function findTagContext(raw: string, caret: number): { start: number; query: string } | null {
    const before = raw.slice(0, caret);
    const hashIndex = before.lastIndexOf("#");
    if (hashIndex < 0) return null;
    if (hashIndex > 0 && !/\s/.test(before[hashIndex - 1])) return null;
    const afterHash = before.slice(hashIndex + 1);
    if (/\s/.test(afterHash)) return null;
    return { start: hashIndex + 1, query: afterHash };
  }

  function findCommandContext(
    raw: string,
    caret: number
  ):
    | { mode: "command"; query: string; replaceStart: number; replaceEnd: number }
    | { mode: "parent"; query: string; replaceStart: number; replaceEnd: number }
    | null {
    const leading = raw.match(/^\s*/)?.[0].length ?? 0;
    if (raw.slice(leading, leading + 1) !== "/") return null;

    const afterSlashPos = leading + 1;
    const afterSlash = raw.slice(afterSlashPos);
    const spaceIndex = afterSlash.search(/\s/);
    const nameEndPos = spaceIndex === -1 ? raw.length : afterSlashPos + spaceIndex;
    const namePart = raw.slice(afterSlashPos, nameEndPos).toLowerCase();

    if (caret <= nameEndPos) {
      const query = raw.slice(afterSlashPos, caret).toLowerCase();
      return {
        mode: "command",
        query,
        replaceStart: afterSlashPos,
        replaceEnd: nameEndPos,
      };
    }

    if (namePart !== "parent") return null;
    let argStart = nameEndPos;
    while (argStart < raw.length && /\s/.test(raw[argStart])) {
      argStart += 1;
    }
    const query = raw.slice(argStart, caret).trim();
    return {
      mode: "parent",
      query,
      replaceStart: argStart,
      replaceEnd: caret,
    };
  }

  function buildCommandSuggestions(context: {
    query: string;
    replaceStart: number;
    replaceEnd: number;
  }): Suggestion[] {
    const query = context.query.trim().toLowerCase();
    const items = COMMAND_NAMES.filter((name) => name.startsWith(query)).map((name) => {
      const insertText = commandInsertText(name);
      return {
        id: `cmd:${name}`,
        kind: "command",
        label: `/${name}`,
        insertText,
        replaceStart: context.replaceStart,
        replaceEnd: context.replaceEnd,
      };
    });
    return items.slice(0, 8);
  }

  function buildParentSuggestions(context: {
    query: string;
    replaceStart: number;
    replaceEnd: number;
  }): Suggestion[] {
    const query = context.query.trim().toLowerCase();
    const items: Suggestion[] = [];

    if (!query || "root".startsWith(query)) {
      items.push({
        id: "parent:root",
        kind: "parent",
        label: "root",
        meta: "设为根任务",
        insertText: "root",
        replaceStart: context.replaceStart,
        replaceEnd: context.replaceEnd,
      });
    }

    for (const task of tasks) {
      if (query) {
        const titleMatch = task.title.toLowerCase().includes(query);
        const idMatch = task.id.toLowerCase().includes(query);
        if (!titleMatch && !idMatch) continue;
      }
      const path = buildTaskChain(task.id, taskMap)
        .map((item) => item.title)
        .join(" / ");
      const titleKey = task.title.toLowerCase();
      const insertText = (titleCounts.get(titleKey) ?? 0) > 1 ? task.id : task.title;
      items.push({
        id: `parent:${task.id}`,
        kind: "parent",
        label: task.title,
        meta: path,
        insertText,
        replaceStart: context.replaceStart,
        replaceEnd: context.replaceEnd,
      });
    }

    return items.slice(0, 8);
  }

  function buildTagSuggestions(
    context: { start: number; query: string },
    raw: string,
    caret: number
  ): Suggestion[] {
    const query = context.query.trim().toLowerCase();
    const addSpace = caret >= raw.length;
    const items = tagStats.tags
      .filter((tag) => tag.toLowerCase().startsWith(query))
      .map((tag) => ({
        id: `tag:${tag}`,
        kind: "tag",
        label: `#${tag}`,
        meta: tagStats.counts.get(tag) ? `使用 ${tagStats.counts.get(tag)} 次` : undefined,
        insertText: `${tag}${addSpace ? " " : ""}`,
        replaceStart: context.start,
        replaceEnd: caret,
      }));
    return items.slice(0, 8);
  }

  function commandInsertText(name: (typeof COMMAND_NAMES)[number]): string {
    if (name === "rename" || name === "parent" || name === "sub") {
      return `${name} `;
    }
    return name;
  }

  function replaceRange(raw: string, start: number, end: number, insert: string): string {
    return `${raw.slice(0, start)}${insert}${raw.slice(end)}`;
  }
</script>

<section class="command-bar">
  <form class="command-form" onsubmit={onSubmit}>
    <input
      type="text"
      bind:value={value}
      bind:this={inputEl}
      oninput={onInput}
      onkeyup={onInputKeyup}
      onclick={onInputClick}
      onkeydown={onInputKeydown}
      placeholder="输入命令（/rename /parent /start /pause /resume /stop /sub）或直接输入任务标题"
      autocomplete="off"
      spellcheck="false"
      disabled={busy}
      aria-label="命令输入"
    />
    <button type="submit" disabled={busy || !value.trim()}>{busy ? "执行中..." : "执行"}</button>
  </form>
  {#if suggestionsOpen}
    <div class="command-suggestions" role="listbox" aria-label="命令补全">
      {#each suggestions as item, index (item.id)}
        <button
          type="button"
          class="suggestion-item"
          class:active={index === activeSuggestionIndex}
          onclick={() => applySuggestion(index)}
        >
          <span class="suggestion-label">{item.label}</span>
          {#if item.meta}
            <span class="suggestion-meta">{item.meta}</span>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
  <p class="command-hint">Tab 补全，Shift+Tab 反向切换，↑/↓ 历史。Enter 执行，Esc 关闭/清空。支持 `#tag` 自动补全。</p>
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

  .command-suggestions {
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
    padding: 0.35rem;
    border: 1px solid #c9d6e6;
    border-radius: 0.6rem;
    background: #fff;
    box-shadow: 0 10px 20px rgba(18, 35, 59, 0.12);
  }

  .suggestion-item {
    border: 1px solid transparent;
    border-radius: 0.46rem;
    background: transparent;
    color: #23364d;
    padding: 0.38rem 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.18rem;
    text-align: left;
    cursor: pointer;
  }

  .suggestion-item:hover {
    background: #f3f6fb;
    border-color: #d1dbe8;
  }

  .suggestion-item.active {
    background: #e7f0ff;
    border-color: #a9c3e8;
    color: #1d3f6f;
  }

  .suggestion-label {
    font-weight: 600;
    font-size: 0.86rem;
  }

  .suggestion-meta {
    font-size: 0.74rem;
    color: #56708e;
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
