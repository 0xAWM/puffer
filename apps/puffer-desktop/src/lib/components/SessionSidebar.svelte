<script lang="ts">
  import type { FolderGroup, SessionListItem } from "../types";

  export let groups: FolderGroup[] = [];
  export let activeSessionId: string | null = null;
  export let loading = false;
  export let onSelect: (session: SessionListItem) => void = () => {};

  const timeFormatter = new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit"
  });

  let query = "";
  let collapsedGroupIds = new Set<string>();

  function toggleGroup(groupId: string) {
    const next = new Set(collapsedGroupIds);
    if (next.has(groupId)) {
      next.delete(groupId);
    } else {
      next.add(groupId);
    }
    collapsedGroupIds = next;
  }

  function setCollapsedState(collapsed: boolean) {
    collapsedGroupIds = collapsed ? new Set(groups.map((group) => group.id)) : new Set<string>();
  }

  function matchesSession(session: SessionListItem, filter: string): boolean {
    if (!filter) {
      return true;
    }
    const haystack = [
      session.title,
      session.displayName ?? "",
      session.cwd,
      session.slug ?? "",
      session.note ?? "",
      ...session.tags
    ]
      .join(" ")
      .toLowerCase();
    return haystack.includes(filter);
  }

  function clearQuery() {
    query = "";
  }

  function groupContainsActiveSession(group: FolderGroup): boolean {
    return activeSessionId !== null && group.sessions.some((session) => session.id === activeSessionId);
  }

  $: trimmedQuery = query.trim().toLowerCase();
  $: visibleGroups = groups
    .map((group) => ({
      ...group,
      sessions: group.sessions.filter((session) => matchesSession(session, trimmedQuery))
    }))
    .sort((left, right) => {
      const leftActive = groupContainsActiveSession(left);
      const rightActive = groupContainsActiveSession(right);
      if (leftActive !== rightActive) {
        return leftActive ? -1 : 1;
      }
      return left.label.localeCompare(right.label);
    })
    .filter((group) => group.sessions.length > 0);
  $: totalSessions = groups.reduce((count, group) => count + group.sessions.length, 0);
  $: visibleSessionCount = visibleGroups.reduce((count, group) => count + group.sessions.length, 0);
  $: {
    if (activeSessionId) {
      const activeGroup = groups.find((group) => groupContainsActiveSession(group));
      if (activeGroup && collapsedGroupIds.has(activeGroup.id)) {
        const next = new Set(collapsedGroupIds);
        next.delete(activeGroup.id);
        collapsedGroupIds = next;
      }
    }
  }
</script>

<aside class="sidebar">
  <div class="sidebar-header">
    <div class="header-copy">
      <p class="eyebrow">Workspaces</p>
      <h2>Folders and sessions</h2>
      <p class="summary">{totalSessions} sessions indexed</p>
    </div>
    <div class="header-actions">
      <button class="mini" on:click={() => setCollapsedState(false)}>Expand</button>
      <button class="mini" on:click={() => setCollapsedState(true)}>Collapse</button>
    </div>
  </div>

  <div class="search-box">
    <div class="search-row">
      <input bind:value={query} placeholder="Search sessions, tags, notes" spellcheck={false} />
      {#if query}
        <button class="clear" on:click={clearQuery}>Clear</button>
      {/if}
    </div>
    <p class="search-summary">
      {#if trimmedQuery}
        {visibleSessionCount} matches across {visibleGroups.length} folders
      {:else}
        Browse by folder or search by session context
      {/if}
    </p>
  </div>

  <div class="group-list">
    {#if loading}
      <div class="empty">Loading sessions...</div>
    {:else if !visibleGroups.length}
      <div class="empty">No sessions matched this filter.</div>
    {:else}
      {#each visibleGroups as group}
        <section class="group">
          <button class="group-header" on:click={() => toggleGroup(group.id)}>
            <div>
              <h3>{group.label}</h3>
              <p>{group.path}</p>
            </div>
            <div class="group-meta">
              {#if groupContainsActiveSession(group)}
                <span class="group-pill">Active</span>
              {/if}
              <span>{collapsedGroupIds.has(group.id) ? "+" : `${group.sessions.length}`}</span>
            </div>
          </button>

          {#if !collapsedGroupIds.has(group.id)}
            <div class="sessions">
              {#each group.sessions as session}
                <button
                  class:selected={session.id === activeSessionId}
                  class="session-card"
                  on:click={() => onSelect(session)}
                >
                  <div class="session-topline">
                    <strong>
                      {#if session.id === activeSessionId}
                        <span class="active-dot"></span>
                      {/if}
                      {session.displayName ?? session.title}
                    </strong>
                    <small>{session.id === activeSessionId ? "Active" : timeFormatter.format(session.updatedAtMs)}</small>
                  </div>
                  <p class="session-path">{session.cwd}</p>
                  {#if session.note}
                    <p class="session-note">{session.note}</p>
                  {/if}
                  <div class="session-footer">
                    <span>{session.eventCount} events</span>
                    {#if session.tags.length}
                      <span>{session.tags.join(" · ")}</span>
                    {/if}
                  </div>
                </button>
              {/each}
            </div>
          {/if}
        </section>
      {/each}
    {/if}
  </div>
</aside>

<style>
  .sidebar {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    border-right: 1px solid var(--border);
    background:
      linear-gradient(180deg, rgba(254, 250, 243, 0.94), rgba(245, 239, 229, 0.86)),
      rgba(250, 245, 237, 0.92);
    min-width: 300px;
  }

  .sidebar-header {
    padding: 1.25rem 1.1rem 0.85rem;
    border-bottom: 1px solid var(--border);
    display: flex;
    justify-content: space-between;
    gap: 0.8rem;
    align-items: start;
  }

  .header-copy {
    min-width: 0;
  }

  .eyebrow {
    margin: 0 0 0.35rem;
    font-size: 0.72rem;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .sidebar-header h2 {
    margin: 0;
    font-size: 1.08rem;
    line-height: 1.2;
  }

  .summary {
    margin: 0.35rem 0 0;
    color: var(--text-muted);
  }

  .header-actions {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .mini {
    border: 1px solid rgba(111, 101, 89, 0.18);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.72);
    color: var(--text-muted);
    padding: 0.38rem 0.62rem;
    cursor: pointer;
  }

  .search-box {
    padding: 0.9rem 1rem;
    border-bottom: 1px solid var(--border);
  }

  input {
    width: 100%;
    border: 1px solid rgba(111, 101, 89, 0.22);
    border-radius: 14px;
    padding: 0.8rem 0.95rem;
    background: rgba(255, 255, 255, 0.78);
    color: var(--text);
    outline: none;
  }

  .search-row {
    display: flex;
    gap: 0.55rem;
    align-items: center;
  }

  input:focus {
    border-color: rgba(20, 99, 86, 0.36);
    box-shadow: 0 0 0 3px rgba(20, 99, 86, 0.12);
  }

  .clear {
    border: 1px solid rgba(111, 101, 89, 0.18);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.72);
    color: var(--text-muted);
    padding: 0.42rem 0.68rem;
    cursor: pointer;
    flex: 0 0 auto;
  }

  .search-summary {
    margin: 0.55rem 0 0;
    color: var(--text-muted);
    font-size: 0.82rem;
    line-height: 1.45;
  }

  .group-list {
    overflow: auto;
    padding: 0.8rem;
    display: grid;
    gap: 0.8rem;
  }

  .group {
    border: 1px solid rgba(111, 101, 89, 0.12);
    border-radius: 22px;
    background: rgba(255, 252, 246, 0.72);
    overflow: hidden;
  }

  .group-header {
    width: 100%;
    border: 0;
    background: transparent;
    padding: 0.95rem 1rem;
    display: flex;
    justify-content: space-between;
    align-items: start;
    cursor: pointer;
    text-align: left;
  }

  .group-header h3 {
    margin: 0;
    font-size: 0.95rem;
  }

  .group-header p {
    margin: 0.22rem 0 0;
    color: var(--text-muted);
    font-size: 0.82rem;
    line-height: 1.4;
  }

  .group-header span {
    color: var(--text-muted);
    font-size: 0.82rem;
  }

  .group-meta {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }

  .group-pill {
    padding: 0.28rem 0.5rem;
    border-radius: 999px;
    background: rgba(222, 238, 232, 0.88);
    color: var(--accent);
    border: 1px solid rgba(20, 99, 86, 0.14);
    font-size: 0.72rem;
  }

  .sessions {
    padding: 0 0.7rem 0.7rem;
    display: grid;
    gap: 0.55rem;
  }

  .session-card {
    width: 100%;
    border: 1px solid transparent;
    border-radius: 18px;
    padding: 0.85rem 0.9rem;
    text-align: left;
    background: rgba(255, 255, 255, 0.62);
    cursor: pointer;
    display: grid;
    gap: 0.4rem;
    transition: transform 120ms ease, border-color 120ms ease, box-shadow 120ms ease;
  }

  .session-card:hover,
  .session-card.selected {
    transform: translateY(-1px);
    border-color: rgba(20, 99, 86, 0.24);
    box-shadow: var(--shadow-soft);
  }

  .session-card.selected {
    background: linear-gradient(180deg, rgba(222, 238, 232, 0.78), rgba(255, 255, 255, 0.74));
  }

  .session-topline {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    align-items: baseline;
  }

  strong {
    font-size: 0.95rem;
    line-height: 1.35;
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
  }

  .active-dot {
    width: 0.52rem;
    height: 0.52rem;
    border-radius: 999px;
    background: var(--accent);
    box-shadow: 0 0 0 4px rgba(20, 99, 86, 0.12);
    flex: 0 0 auto;
  }

  small,
  .session-path,
  .session-note,
  .session-footer {
    color: var(--text-muted);
  }

  .session-path,
  .session-note {
    margin: 0;
    line-height: 1.45;
  }

  .session-footer {
    display: flex;
    gap: 0.7rem;
    flex-wrap: wrap;
    font-size: 0.8rem;
  }

  .empty {
    padding: 1rem;
    border-radius: 18px;
    color: var(--text-muted);
    background: rgba(255, 252, 246, 0.72);
    border: 1px dashed rgba(111, 101, 89, 0.24);
  }

  @media (max-width: 980px) {
    .sidebar-header {
      flex-direction: column;
    }

    .header-actions {
      justify-content: flex-start;
    }

    .search-row {
      align-items: stretch;
    }
  }
</style>
