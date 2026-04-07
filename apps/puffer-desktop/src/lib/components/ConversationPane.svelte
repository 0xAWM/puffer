<script lang="ts">
  import type { TimelineItem } from "../types";
  import TimelineItemCard from "./TimelineItemCard.svelte";

  export let timeline: TimelineItem[] = [];
  export let selectedId: string | null = null;
  export let loading = false;
  export let onSelect: (item: TimelineItem) => void = () => {};

  type FilterMode = "all" | "messages" | "tools" | "permissions" | "diffs";

  let filterMode: FilterMode = "all";
  let query = "";

  function matchesFilter(item: TimelineItem, mode: FilterMode): boolean {
    if (mode === "all") {
      return true;
    }
    if (mode === "messages") {
      return item.kind === "user" || item.kind === "assistant" || item.kind === "system" || item.kind === "command";
    }
    if (mode === "tools") {
      return item.kind === "tool";
    }
    if (mode === "permissions") {
      return item.kind === "permission";
    }
    return item.kind === "diff";
  }

  function matchesQuery(item: TimelineItem, search: string): boolean {
    if (!search) {
      return true;
    }
    return [item.title, item.summary, item.body, item.meta.join(" ")]
      .join(" ")
      .toLowerCase()
      .includes(search);
  }

  function filterCount(mode: FilterMode): number {
    return timeline.filter((item) => matchesFilter(item, mode)).length;
  }

  function resetFilters() {
    filterMode = "all";
    query = "";
  }

  $: toolCount = timeline.filter((item) => item.kind === "tool").length;
  $: permissionCount = timeline.filter((item) => item.kind === "permission").length;
  $: diffCount = timeline.filter((item) => item.kind === "diff").length;
  $: trimmedQuery = query.trim().toLowerCase();
  $: visibleTimeline = timeline.filter((item) => matchesFilter(item, filterMode) && matchesQuery(item, trimmedQuery));
  $: selectedVisible = selectedId ? visibleTimeline.some((item) => item.id === selectedId) : true;
</script>

<section class="conversation">
  <div class="section-header">
    <div>
      <p class="eyebrow">Conversation</p>
      <h2>Transcript and tool activity</h2>
    </div>
    <div class="counters">
      <span>{timeline.length} items</span>
      <span>{toolCount} tools</span>
      <span>{permissionCount} approvals</span>
      <span>{diffCount} diffs</span>
    </div>
  </div>

  <div class="controls">
    <div class="filters">
      <button class:active={filterMode === "all"} on:click={() => (filterMode = "all")}>
        All {filterCount("all")}
      </button>
      <button class:active={filterMode === "messages"} on:click={() => (filterMode = "messages")}>
        Messages {filterCount("messages")}
      </button>
      <button class:active={filterMode === "tools"} on:click={() => (filterMode = "tools")}>
        Tools {filterCount("tools")}
      </button>
      <button class:active={filterMode === "permissions"} on:click={() => (filterMode = "permissions")}>
        Approvals {filterCount("permissions")}
      </button>
      <button class:active={filterMode === "diffs"} on:click={() => (filterMode = "diffs")}>
        Diffs {filterCount("diffs")}
      </button>
    </div>

    <div class="search">
      <input bind:value={query} placeholder="Filter title, summary, body, metadata" spellcheck={false} />
      <div class="search-actions">
        <span>{visibleTimeline.length} shown</span>
        {#if query || filterMode !== "all"}
          <button class="clear" on:click={resetFilters}>Reset</button>
        {/if}
      </div>
    </div>
  </div>

  <div class="items">
    {#if loading}
      <div class="empty-card">Loading conversation...</div>
    {:else if !timeline.length}
      <div class="empty-card">No conversation items are available for this session yet.</div>
    {:else if !visibleTimeline.length}
      <div class="empty-card">No conversation items match the current filter.</div>
    {:else}
      {#if !selectedVisible}
        <div class="notice-card">
          <span>The focused item is hidden by the current filter.</span>
          <button on:click={resetFilters}>Show everything</button>
        </div>
      {/if}
      {#each visibleTimeline as item}
        <TimelineItemCard
          item={item}
          selected={item.id === selectedId}
          onSelect={() => onSelect(item)}
        />
      {/each}
    {/if}
  </div>
</section>

<style>
  .conversation {
    min-width: 0;
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    background:
      linear-gradient(180deg, rgba(252, 248, 242, 0.92), rgba(248, 242, 234, 0.78)),
      var(--canvas);
  }

  .section-header {
    position: sticky;
    top: 0;
    z-index: 2;
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: end;
    padding: 1.1rem 1.15rem 0.95rem;
    border-bottom: 1px solid rgba(92, 73, 50, 0.12);
    background: rgba(253, 249, 243, 0.94);
  }

  .eyebrow {
    margin: 0 0 0.28rem;
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.18em;
    color: var(--text-soft);
    font-weight: 600;
  }

  h2 {
    margin: 0;
    font-size: 1.06rem;
    line-height: 1.2;
  }

  .counters {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .counters span {
    padding: 0.34rem 0.6rem;
    border-radius: 999px;
    border: 1px solid rgba(102, 83, 62, 0.1);
    background: rgba(255, 255, 255, 0.62);
    color: var(--text-soft);
    font-size: 0.75rem;
  }

  .items {
    min-height: 0;
    overflow: auto;
    padding: 1rem 1.05rem 1.1rem;
    display: grid;
    gap: 0.9rem;
    align-content: start;
  }

  .controls {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.9rem;
    padding: 0.9rem 1.05rem;
    border-bottom: 1px solid rgba(92, 73, 50, 0.1);
    background: rgba(247, 240, 231, 0.72);
    align-items: center;
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
    align-items: center;
  }

  .filters button {
    border: 1px solid rgba(102, 83, 62, 0.12);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.78);
    color: var(--text-muted);
    padding: 0.48rem 0.74rem;
    cursor: pointer;
    box-shadow: var(--shadow-edge);
  }

  .filters button.active {
    border-color: rgba(36, 105, 81, 0.14);
    background: rgba(220, 234, 224, 0.88);
    color: var(--accent-strong);
  }

  .search {
    display: flex;
    justify-content: space-between;
    gap: 0.8rem;
    align-items: center;
    min-width: min(26rem, 100%);
  }

  .search input {
    flex: 1;
    border: 1px solid rgba(102, 83, 62, 0.12);
    border-radius: 14px;
    background: rgba(255, 255, 255, 0.86);
    color: var(--text);
    padding: 0.78rem 0.94rem;
    outline: none;
  }

  .search input:focus {
    border-color: rgba(36, 105, 81, 0.26);
    box-shadow: 0 0 0 3px rgba(36, 105, 81, 0.1);
  }

  .search span {
    white-space: nowrap;
    color: var(--text-soft);
    font-size: 0.8rem;
  }

  .search-actions {
    display: flex;
    align-items: center;
    gap: 0.55rem;
  }

  .clear,
  .notice-card button {
    border: 1px solid rgba(102, 83, 62, 0.12);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.78);
    color: var(--text-muted);
    padding: 0.4rem 0.66rem;
    cursor: pointer;
  }

  .empty-card {
    padding: 1rem 1.05rem;
    border-radius: 18px;
    background: rgba(255, 251, 246, 0.72);
    border: 1px dashed rgba(102, 83, 62, 0.18);
    color: var(--text-muted);
    min-height: min(18rem, 42vh);
    display: grid;
    align-content: center;
  }

  .notice-card {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.8rem;
    padding: 0.9rem 0.98rem;
    border-radius: 18px;
    background: rgba(244, 230, 208, 0.74);
    border: 1px solid rgba(141, 97, 48, 0.14);
    color: var(--text-muted);
  }

  @media (max-width: 900px) {
    .controls {
      grid-template-columns: 1fr;
    }

    .search {
      flex-direction: column;
      align-items: stretch;
    }

    .search-actions,
    .notice-card {
      justify-content: space-between;
    }
  }
</style>
