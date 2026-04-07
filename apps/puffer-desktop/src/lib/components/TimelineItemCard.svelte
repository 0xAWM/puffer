<script lang="ts">
  import type {
    DiffTimelineItem,
    PermissionTimelineItem,
    TimelineItem,
    ToolTimelineItem
  } from "../types";
  import DiffView from "./DiffView.svelte";
  import MessageBody from "./MessageBody.svelte";

  export let item: TimelineItem;
  export let selected = false;
  export let onSelect: () => void = () => {};

  function isToolItem(value: TimelineItem): value is ToolTimelineItem {
    return value.kind === "tool";
  }

  function isPermissionItem(value: TimelineItem): value is PermissionTimelineItem {
    return value.kind === "permission";
  }

  function isDiffItem(value: TimelineItem): value is DiffTimelineItem {
    return value.kind === "diff";
  }

  function isMessageLike(value: TimelineItem): boolean {
    return value.kind === "user" || value.kind === "assistant" || value.kind === "system";
  }

  function preview(value: string, maxLength = 220): string {
    return value.length > maxLength ? `${value.slice(0, maxLength).trimEnd()}...` : value;
  }
</script>

<button class:selected class={"card " + item.kind} on:click={onSelect}>
  <div class="header">
    <div class="title-block">
      <span class={"kind " + item.kind}>{item.kind}</span>
      <strong>{item.title}</strong>
    </div>
    {#if isToolItem(item) || isPermissionItem(item)}
      <span class={"status " + item.status}>{item.status}</span>
    {/if}
  </div>

  {#if item.meta.length}
    <div class="meta-row">
      {#each item.meta as meta}
        <span>{meta}</span>
      {/each}
    </div>
  {/if}

  {#if isDiffItem(item)}
    <DiffView diff={item.diff} compact={true} />
  {:else}
    <p class="summary">{item.summary}</p>
    {#if item.kind === "command"}
      <pre class="body command-body">{item.body}</pre>
    {:else if isMessageLike(item)}
      <div class="body message-body">
        <MessageBody body={item.body} />
      </div>
    {:else}
      <div class="body">{item.body}</div>
    {/if}
  {/if}

  {#if isToolItem(item)}
    <div class="tool-grid">
      <div>
        <p class="label">Input</p>
        <pre>{preview(item.input)}</pre>
      </div>
      <div>
        <p class="label">Output</p>
        <pre>{preview(item.output)}</pre>
      </div>
    </div>
    {#if item.status === "error" || item.status === "ask"}
      <div class="inline-banner">
        <strong>{item.status === "ask" ? "Approval needed" : "Tool failed"}</strong>
        <span>{preview(item.output)}</span>
      </div>
    {/if}
  {/if}

  {#if isPermissionItem(item)}
    <div class="inline-banner permission-banner">
      <strong>{item.permissionDialog.summary ?? "Approval request"}</strong>
      <span>{item.permissionDialog.reason}</span>
    </div>
    <div class="permission-actions">
      {#each item.choices as choice}
        <span>{choice}</span>
      {/each}
    </div>
  {/if}
</button>

<style>
  .card {
    width: 100%;
    text-align: left;
    border: 1px solid rgba(111, 101, 89, 0.18);
    background: rgba(255, 255, 255, 0.74);
    border-radius: 24px;
    padding: 1rem;
    display: grid;
    gap: 0.8rem;
    cursor: pointer;
    transition: transform 120ms ease, box-shadow 120ms ease, border-color 120ms ease;
    box-shadow: var(--shadow-soft);
  }

  .card:hover,
  .card.selected {
    transform: translateY(-1px);
    box-shadow: var(--shadow);
  }

  .card.selected {
    border-color: rgba(20, 99, 86, 0.34);
    box-shadow: 0 0 0 2px rgba(20, 99, 86, 0.12), var(--shadow);
  }

  .card.user {
    background: linear-gradient(180deg, rgba(255, 252, 246, 0.92), rgba(255, 255, 255, 0.76));
  }

  .card.assistant {
    background: linear-gradient(180deg, rgba(245, 249, 247, 0.92), rgba(255, 255, 255, 0.8));
  }

  .card.tool {
    background: linear-gradient(180deg, rgba(247, 245, 240, 0.94), rgba(255, 255, 255, 0.78));
  }

  .card.permission {
    background: linear-gradient(180deg, rgba(252, 241, 238, 0.94), rgba(255, 255, 255, 0.78));
  }

  .card.diff {
    padding: 0;
    overflow: hidden;
  }

  .header {
    display: flex;
    justify-content: space-between;
    gap: 0.8rem;
    align-items: start;
  }

  .title-block {
    display: grid;
    gap: 0.25rem;
  }

  .kind {
    color: var(--text-muted);
    font-size: 0.72rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .kind.assistant,
  .kind.tool,
  .kind.diff {
    color: var(--accent);
  }

  .kind.command {
    color: #7d4f25;
  }

  .kind.permission {
    color: var(--danger);
  }

  strong {
    font-size: 1rem;
    line-height: 1.32;
  }

  .status {
    padding: 0.38rem 0.62rem;
    border-radius: 999px;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    background: rgba(228, 221, 208, 0.42);
    color: var(--text-muted);
  }

  .status.ok {
    background: rgba(220, 238, 232, 0.8);
    color: var(--accent);
  }

  .status.ask,
  .status.required {
    background: rgba(247, 225, 220, 0.82);
    color: var(--danger);
  }

  .meta-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }

  .meta-row span,
  .permission-actions span {
    padding: 0.35rem 0.58rem;
    border-radius: 999px;
    border: 1px solid rgba(111, 101, 89, 0.15);
    background: rgba(255, 255, 255, 0.72);
    color: var(--text-muted);
    font-size: 0.76rem;
  }

  .summary,
  .body {
    margin: 0;
  }

  .summary {
    font-weight: 600;
    line-height: 1.45;
  }

  .body {
    line-height: 1.62;
    white-space: pre-wrap;
    color: var(--text);
  }

  .body.message-body {
    display: grid;
    gap: 0.7rem;
  }

  .command-body {
    font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
    padding: 0.8rem;
    border-radius: 16px;
    background: rgba(247, 243, 235, 0.82);
    border: 1px solid rgba(111, 101, 89, 0.14);
    overflow: auto;
  }

  .tool-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.75rem;
  }

  .label {
    margin: 0 0 0.4rem;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }

  pre {
    margin: 0;
    padding: 0.8rem;
    border-radius: 16px;
    background: rgba(247, 243, 235, 0.82);
    border: 1px solid rgba(111, 101, 89, 0.14);
    font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
    font-size: 0.8rem;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow: auto;
  }

  .permission-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }

  .inline-banner {
    display: grid;
    gap: 0.2rem;
    padding: 0.8rem 0.85rem;
    border-radius: 16px;
    background: rgba(255, 248, 234, 0.8);
    border: 1px solid rgba(138, 91, 42, 0.14);
  }

  .inline-banner strong {
    font-size: 0.82rem;
  }

  .inline-banner span {
    color: var(--text-muted);
    line-height: 1.5;
  }

  .permission-banner {
    background: rgba(247, 225, 220, 0.64);
    border-color: rgba(157, 58, 43, 0.16);
  }

  @media (max-width: 900px) {
    .tool-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
