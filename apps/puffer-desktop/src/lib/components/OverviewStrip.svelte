<script lang="ts">
  import type { DiffSnapshot, RepoStatus, SessionListItem, TimelineItem } from "../types";

  export let session: SessionListItem | null = null;
  export let repoStatus: RepoStatus | null = null;
  export let latestDiff: DiffSnapshot | null = null;
  export let selectedItem: TimelineItem | null = null;
  export let permissionCount = 0;
  export let toolCount = 0;
  export let diffCount = 0;
  export let onOpenDiff: () => void = () => {};
  export let onOpenHistory: () => void = () => {};
  export let onOpenDetails: () => void = () => {};

  const dateFormatter = new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit"
  });

  function latestDiffSummary(diff: DiffSnapshot | null): string {
    if (!diff) {
      return "No diff snapshot has been captured in this session yet.";
    }
    return diff.status;
  }

  function repoReadiness(status: RepoStatus | null): string {
    if (!status) {
      return "Select a session to inspect repository state.";
    }
    if (!status.isGitRepo) {
      return "Session is outside a git repository.";
    }
    if (!status.ghAvailable) {
      return "GitHub CLI is unavailable.";
    }
    if (!status.ghAuthenticated) {
      return "GitHub CLI needs authentication.";
    }
    if (status.pullRequest) {
      return `PR #${status.pullRequest.number} is ${status.pullRequest.state.toLowerCase()}.`;
    }
    return status.hasUncommittedChanges ? "Working tree has local changes." : "Working tree is clean.";
  }

  function focusSummary(item: TimelineItem | null): string {
    if (!item) {
      return "Select a tool or permission item to inspect details.";
    }
    return `Focused item: ${item.title}`;
  }
</script>

<section class="overview">
  <button class="card" on:click={onOpenHistory}>
    <p class="eyebrow">Session</p>
    <strong>{session?.displayName ?? session?.title ?? "No session selected"}</strong>
    <span>
      {#if session}
        {session.eventCount} events · updated {dateFormatter.format(session.updatedAtMs)}
      {:else}
        Load a session to inspect history.
      {/if}
    </span>
    {#if session?.note}
      <span class="secondary-line">{session.note}</span>
    {/if}
  </button>

  <button class="card emphasis" on:click={onOpenDiff}>
    <p class="eyebrow">Latest Diff</p>
    <strong>{latestDiff?.title ?? "Awaiting diff snapshot"}</strong>
    <span>{latestDiffSummary(latestDiff)}</span>
    <span class="secondary-line">{diffCount} diff items recorded in this session</span>
  </button>

  <button class:alert={permissionCount > 0} class="card" on:click={onOpenDetails}>
    <p class="eyebrow">Approvals</p>
    <strong>{permissionCount > 0 ? `${permissionCount} pending approval item${permissionCount === 1 ? "" : "s"}` : "No pending approvals"}</strong>
    <span>{focusSummary(selectedItem)}</span>
    <span class="secondary-line">{toolCount} tool events in the current timeline</span>
  </button>

  <button class="card" on:click={onOpenHistory}>
    <p class="eyebrow">Repository</p>
    <strong>{repoStatus?.branch ?? "No branch"}</strong>
    <span>{repoReadiness(repoStatus)}</span>
    {#if repoStatus?.statusLines.length}
      <span class="secondary-line">{repoStatus.statusLines.length} changed path entries</span>
    {/if}
  </button>
</section>

<style>
  .overview {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0.9rem;
    padding: 1rem 1.1rem 1.05rem;
    border-bottom: 1px solid rgba(92, 73, 50, 0.12);
    background: rgba(245, 239, 230, 0.58);
  }

  .card {
    display: grid;
    gap: 0.42rem;
    text-align: left;
    border: 1px solid rgba(102, 83, 62, 0.12);
    border-radius: 18px;
    padding: 1rem 1.05rem;
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.96), rgba(251, 247, 241, 0.92));
    cursor: pointer;
    transition: transform 120ms ease, box-shadow 120ms ease, border-color 120ms ease;
    box-shadow: var(--shadow-soft), var(--shadow-edge);
  }

  .card:hover {
    transform: translateY(-1px);
    box-shadow: var(--shadow);
  }

  .card.emphasis {
    background:
      linear-gradient(180deg, rgba(220, 234, 224, 0.96), rgba(250, 253, 250, 0.9)),
      var(--surface);
    border-color: rgba(36, 105, 81, 0.16);
  }

  .card.alert {
    background:
      linear-gradient(180deg, rgba(244, 224, 218, 0.94), rgba(255, 252, 249, 0.9)),
      var(--surface);
    border-color: rgba(157, 74, 55, 0.16);
  }

  .eyebrow {
    margin: 0;
    font-size: 0.68rem;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--text-soft);
    font-weight: 600;
  }

  strong {
    font-size: 1rem;
    line-height: 1.3;
  }

  span {
    color: var(--text-muted);
    font-size: 0.84rem;
    line-height: 1.45;
  }

  .secondary-line {
    font-size: 0.78rem;
    color: var(--text-soft);
  }

  @media (max-width: 1200px) {
    .overview {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 780px) {
    .overview {
      grid-template-columns: 1fr;
    }
  }
</style>
