<script lang="ts">
  import type { DiffSnapshot, InspectorTab, TimelineItem } from "../types";
  import DiffView from "./DiffView.svelte";
  import MessageBody from "./MessageBody.svelte";

  export let open = true;
  export let tab: InspectorTab = "latest-diff";
  export let latestDiff: DiffSnapshot | null = null;
  export let diffHistory: DiffSnapshot[] = [];
  export let timeline: TimelineItem[] = [];
  export let selectedId: string | null = null;
  export let selectedItem: TimelineItem | null = null;
  export let onTabChange: (tab: InspectorTab) => void = () => {};
  export let onToggle: () => void = () => {};
  export let onSelectItem: (item: TimelineItem) => void = () => {};

  function excerpt(value: string, length = 140): string {
    return value.length > length ? `${value.slice(0, length).trimEnd()}...` : value;
  }

  function isMessageLike(item: TimelineItem | null): boolean {
    return item?.kind === "user" || item?.kind === "assistant" || item?.kind === "system";
  }

  $: diffCount = diffHistory.length;
  $: toolDetailCount = timeline.filter((item) => item.kind === "tool" || item.kind === "permission").length;
</script>

<aside class:closed={!open} class="inspector">
  <div class="header">
    <div class="tabs">
      <button class:active={tab === "latest-diff"} on:click={() => onTabChange("latest-diff")}>
        Latest Diff {latestDiff ? 1 : 0}
      </button>
      <button class:active={tab === "history"} on:click={() => onTabChange("history")}>
        History {timeline.length}
      </button>
      <button class:active={tab === "tool-details"} on:click={() => onTabChange("tool-details")}>
        Tool Details {toolDetailCount}
      </button>
    </div>
    <button class="toggle" on:click={onToggle}>{open ? "Collapse" : "Expand"}</button>
  </div>

  {#if open}
    <div class="panel">
      {#if tab === "latest-diff"}
        {#if latestDiff}
          <DiffView diff={latestDiff} />
        {:else}
          <div class="empty">No diff snapshot has been recorded for this session yet.</div>
        {/if}
      {:else if tab === "history"}
        <div class="history-stack">
          <section>
            <div class="section-title">
              <p class="eyebrow">History</p>
              <h3>Recent diff checkpoints</h3>
            </div>
            {#if diffHistory.length}
              <div class="history-diffs">
                {#each diffHistory as diff}
                  <DiffView {diff} compact={true} />
                {/each}
              </div>
            {:else}
              <div class="empty">No diff history is available.</div>
            {/if}
            {#if diffCount}
              <p class="section-summary">{diffCount} diff checkpoints retained for this session.</p>
            {/if}
          </section>

          <section>
            <div class="section-title">
              <p class="eyebrow">Timeline</p>
              <h3>Conversation history</h3>
            </div>
            <div class="history-items">
              {#each timeline as item}
                <button
                  class:selected={item.id === selectedId}
                  class="history-item"
                  on:click={() => onSelectItem(item)}
                >
                  <div>
                    <strong>{item.title}</strong>
                    <p>{excerpt(item.summary)}</p>
                  </div>
                  <span>{item.kind}</span>
                </button>
              {/each}
            </div>
            <p class="section-summary">{timeline.length} timeline items indexed in this view.</p>
          </section>
        </div>
      {:else if selectedItem}
        <section class="detail-stack">
          <div class="section-title">
            <p class="eyebrow">Selected item</p>
            <h3>{selectedItem.title}</h3>
          </div>

          <div class="detail-meta">
            <span>{selectedItem.kind}</span>
            {#each selectedItem.meta as meta}
              <span>{meta}</span>
            {/each}
          </div>

          <div class="detail-block">
            <p class="label">Summary</p>
            <p>{selectedItem.summary}</p>
          </div>

          <div class="detail-block">
            <p class="label">Body</p>
            {#if isMessageLike(selectedItem)}
              <MessageBody body={selectedItem.body} />
            {:else}
              <pre>{selectedItem.body}</pre>
            {/if}
          </div>

          {#if selectedItem.kind === "tool"}
            <div class="detail-block">
              <p class="label">Input</p>
              <pre>{selectedItem.input}</pre>
            </div>
            <div class="detail-block">
              <p class="label">Output</p>
              <pre>{selectedItem.output}</pre>
            </div>
            {#if selectedItem.status === "ask" || selectedItem.status === "error"}
              <div class="detail-block emphasis">
                <p class="label">Execution state</p>
                <p>{selectedItem.status === "ask" ? "This tool call paused for approval." : "This tool call finished with an error."}</p>
              </div>
            {/if}
          {/if}

          {#if selectedItem.kind === "permission"}
            <div class="detail-block emphasis">
              <p class="label">Reason</p>
              <p>{selectedItem.permissionDialog.reason}</p>
            </div>
            {#if selectedItem.permissionDialog.inputText}
              <div class="detail-block">
                <p class="label">Requested input</p>
                <pre>{selectedItem.permissionDialog.inputText}</pre>
              </div>
            {/if}
            <div class="detail-block">
              <p class="label">Available actions</p>
              <div class="choice-row">
                {#each selectedItem.choices as choice}
                  <span>{choice}</span>
                {/each}
              </div>
            </div>
          {/if}

          {#if selectedItem.kind === "diff"}
            <DiffView diff={selectedItem.diff} compact={true} />
          {/if}
        </section>
      {:else}
        <div class="empty">Select a conversation item to inspect its details.</div>
      {/if}
    </div>
  {:else}
    <button class="collapsed-panel" on:click={onToggle}>
      <div class="collapsed-chip">{tab}</div>
      <div class="collapsed-copy">
        <strong>{selectedItem?.title ?? "Inspector"}</strong>
        <span>
          {#if selectedItem}
            {selectedItem.kind} selected
          {:else if tab === "latest-diff"}
            {latestDiff ? "1 latest diff" : "No diff"}
          {:else if tab === "history"}
            {timeline.length} history items
          {:else}
            {toolDetailCount} tool details
          {/if}
        </span>
      </div>
    </button>
  {/if}
</aside>

<style>
  .inspector {
    min-width: 420px;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    border-left: 1px solid rgba(92, 73, 50, 0.1);
    background:
      linear-gradient(180deg, rgba(239, 233, 224, 0.94), rgba(233, 225, 214, 0.86)),
      var(--canvas-muted);
  }

  .inspector.closed {
    min-width: 108px;
  }

  .header {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    align-items: center;
    padding: 0.95rem 1rem;
    border-bottom: 1px solid rgba(92, 73, 50, 0.12);
    background: rgba(249, 244, 237, 0.8);
  }

  .tabs {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
    padding: 0.3rem;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.5);
    border: 1px solid rgba(102, 83, 62, 0.08);
  }

  .tabs button,
  .toggle {
    border: 1px solid rgba(102, 83, 62, 0.12);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.76);
    padding: 0.48rem 0.8rem;
    cursor: pointer;
    color: var(--text);
  }

  .tabs button.active {
    background: rgba(220, 234, 224, 0.88);
    border-color: rgba(36, 105, 81, 0.14);
    color: var(--accent-strong);
  }

  .panel {
    min-width: 0;
    overflow: auto;
    padding: 1rem 1.05rem 1.1rem;
  }

  .collapsed-panel {
    display: grid;
    gap: 0.8rem;
    align-content: center;
    justify-items: center;
    padding: 1rem 0.6rem;
    min-height: 0;
    border: 0;
    background: transparent;
    cursor: pointer;
    text-align: center;
  }

  .collapsed-panel:hover {
    background: rgba(255, 255, 255, 0.14);
  }

  .collapsed-chip {
    padding: 0.38rem 0.56rem;
    border-radius: 999px;
    border: 1px solid rgba(102, 83, 62, 0.12);
    background: rgba(255, 255, 255, 0.74);
    color: var(--text-soft);
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    writing-mode: vertical-rl;
    text-orientation: mixed;
  }

  .collapsed-copy {
    display: grid;
    gap: 0.35rem;
    justify-items: center;
    text-align: center;
  }

  .collapsed-copy strong {
    font-size: 0.86rem;
    writing-mode: vertical-rl;
    text-orientation: mixed;
    line-height: 1.2;
  }

  .collapsed-copy span {
    font-size: 0.76rem;
    color: var(--text-muted);
    writing-mode: vertical-rl;
    text-orientation: mixed;
    line-height: 1.2;
  }

  .history-stack,
  .detail-stack {
    display: grid;
    gap: 1rem;
  }

  .section-title {
    display: grid;
    gap: 0.2rem;
    margin-bottom: 0.7rem;
  }

  .eyebrow {
    margin: 0;
    color: var(--text-soft);
    font-size: 0.68rem;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    font-weight: 600;
  }

  h3 {
    margin: 0;
    font-size: 1rem;
  }

  .section-summary {
    margin: 0.7rem 0 0;
    color: var(--text-soft);
    font-size: 0.82rem;
    line-height: 1.45;
  }

  .history-diffs,
  .history-items {
    display: grid;
    gap: 0.8rem;
  }

  .history-item {
    width: 100%;
    text-align: left;
    border: 1px solid rgba(102, 83, 62, 0.12);
    background: rgba(255, 255, 255, 0.72);
    border-radius: 18px;
    padding: 0.9rem 0.98rem;
    display: flex;
    justify-content: space-between;
    gap: 0.8rem;
    cursor: pointer;
    box-shadow: var(--shadow-edge);
  }

  .history-item.selected {
    border-color: rgba(36, 105, 81, 0.22);
    box-shadow: 0 0 0 2px rgba(36, 105, 81, 0.08);
  }

  .history-item strong {
    display: block;
    margin-bottom: 0.28rem;
  }

  .history-item p {
    margin: 0;
    color: var(--text-muted);
    line-height: 1.45;
  }

  .history-item span,
  .detail-meta span,
  .choice-row span {
    padding: 0.32rem 0.54rem;
    border-radius: 999px;
    border: 1px solid rgba(102, 83, 62, 0.12);
    background: rgba(255, 255, 255, 0.72);
    color: var(--text-soft);
    font-size: 0.76rem;
  }

  .detail-meta,
  .choice-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }

  .detail-block {
    display: grid;
    gap: 0.45rem;
    padding: 0.98rem;
    border-radius: 18px;
    background: rgba(255, 251, 246, 0.8);
    border: 1px solid rgba(102, 83, 62, 0.12);
    box-shadow: var(--shadow-edge);
  }

  .detail-block.emphasis {
    background: rgba(244, 230, 208, 0.8);
    border-color: rgba(141, 97, 48, 0.14);
  }

  .label {
    margin: 0;
    color: var(--text-soft);
    font-size: 0.68rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    font-weight: 600;
  }

  .detail-block p {
    margin: 0;
    line-height: 1.6;
  }

  pre {
    margin: 0;
    font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
    font-size: 0.81rem;
    line-height: 1.58;
    white-space: pre-wrap;
    overflow: auto;
  }

  .empty {
    padding: 1rem;
    border-radius: 18px;
    border: 1px dashed rgba(102, 83, 62, 0.18);
    background: rgba(255, 251, 246, 0.66);
    color: var(--text-muted);
  }
</style>
