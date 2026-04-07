<script lang="ts">
  import type { AppView, RepoStatus, SessionListItem } from "../types";

  export let session: SessionListItem | null = null;
  export let repoStatus: RepoStatus | null = null;
  export let view: AppView = "workspace";
  export let remoteLabel: string | null = null;
  export let busy = false;
  export let statusMessage = "";
  export let onRefresh: () => void = () => {};
  export let onCreatePr: () => void = () => {};
  export let onMergePr: () => void = () => {};
  export let onOpenSettings: () => void = () => {};
  export let onBackToWorkspace: () => void = () => {};

  const timeFormatter = new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit"
  });

  function createReason(status: RepoStatus | null): string {
    if (!status) {
      return "Select a session to inspect repository actions.";
    }
    if (!status.canCreatePr) {
      return status.createPrReason ?? "Create PR is not available for this repository.";
    }
    return "Create a pull request from the current branch.";
  }

  function mergeReason(status: RepoStatus | null): string {
    if (!status) {
      return "Select a session to inspect repository actions.";
    }
    if (!status.canMergePr) {
      return status.mergePrReason ?? "Merge is not available for this repository.";
    }
    return "Merge the active pull request.";
  }

  function cleanPath(path: string | null | undefined): string {
    if (view === "login") {
      return "Authentication required";
    }
    if (!path) {
      return "No session selected";
    }
    return path;
  }

  function sessionSummary(session: SessionListItem | null): string {
    if (view === "login") {
      return "Sign in with a provider to unlock sessions, agents, and review workflows.";
    }
    if (!session) {
      return "Select a session to inspect conversation history and repository state.";
    }
    return `${session.eventCount} events · updated ${timeFormatter.format(session.updatedAtMs)}`;
  }

  function repoSummary(status: RepoStatus | null): string {
    if (view === "login") {
      return "Once signed in, the desktop app will open your workspace sessions automatically.";
    }
    if (!status) {
      return "No repository selected.";
    }
    if (!status.isGitRepo) {
      return "This session is not in a git repository.";
    }
    if (status.warnings.length) {
      return status.warnings[0];
    }
    if (status.pullRequest) {
      return `PR #${status.pullRequest.number} is ${status.pullRequest.state.toLowerCase()}.`;
    }
    if (!status.ghAvailable) {
      return "Install GitHub CLI to enable one-click pull request actions.";
    }
    if (!status.ghAuthenticated) {
      return "Authenticate GitHub CLI to enable pull request and merge actions.";
    }
    return status.hasUncommittedChanges
      ? `${status.statusLines.length} changed file entries in the current working tree.`
      : "Working tree is clean and ready for review.";
  }
</script>

<header class="header">
  <div class="identity">
    <div class="brand-block">
      <p class="eyebrow">Puffer Desktop</p>
      <h1>{view === "login" ? "Sign in" : session?.displayName ?? session?.title ?? "Workspace sessions"}</h1>
      <p class="path">{cleanPath(session?.cwd)}</p>
      <p class="session-summary">{sessionSummary(session)}</p>
    </div>

    {#if view !== "login"}
      <div class="repo-pills">
        {#if remoteLabel}
          <span class="pill remote">Remote {remoteLabel}</span>
        {/if}
        <span class="pill neutral">{repoStatus?.branch ?? "No branch"}</span>
        <span class:warning={repoStatus?.hasUncommittedChanges} class="pill neutral">
          {repoStatus?.hasUncommittedChanges ? "Uncommitted changes" : "Working tree clean"}
        </span>
        <span class:ok={repoStatus?.ghAuthenticated} class="pill neutral">
          {repoStatus?.ghAuthenticated ? "GitHub ready" : "GitHub auth needed"}
        </span>
        {#if repoStatus?.pullRequest}
          <a class="pill link" href={repoStatus.pullRequest.url} target="_blank" rel="noreferrer">
            PR #{repoStatus.pullRequest.number}
          </a>
        {/if}
        {#if session?.tags.length}
          {#each session.tags.slice(0, 3) as tag}
            <span class="pill tag">{tag}</span>
          {/each}
        {/if}
      </div>
    {/if}
  </div>

  <div class="actions">
    <div class="action-group">
      <button class="ghost" on:click={onRefresh} disabled={busy}>
        {view === "workspace" ? "Refresh" : "Refresh Snapshot"}
      </button>
      {#if view === "workspace"}
        <button class="ghost" on:click={onOpenSettings}>Settings</button>
        <button
          class="primary"
          on:click={onCreatePr}
          disabled={busy || !repoStatus?.canCreatePr}
          title={createReason(repoStatus)}
        >
          Create PR
        </button>
        {#if repoStatus?.pullRequest}
          <a class="secondary link-button" href={repoStatus.pullRequest.url} target="_blank" rel="noreferrer">
            Open PR
          </a>
        {/if}
        <button
          class="secondary"
          on:click={onMergePr}
          disabled={busy || !repoStatus?.canMergePr}
          title={mergeReason(repoStatus)}
        >
          Merge
        </button>
      {:else if view === "settings"}
        <button class="primary" on:click={onBackToWorkspace}>Back to Workspace</button>
      {/if}
    </div>

    <div class="status-copy">
      <p class="status-line">{statusMessage}</p>
      <p class="repo-summary">{repoSummary(repoStatus)}</p>
    </div>
  </div>
</header>

<style>
  .header {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 1.4rem;
    padding: 1.4rem 1.5rem 1.2rem;
    border-bottom: 1px solid rgba(92, 73, 50, 0.12);
    background:
      radial-gradient(circle at top right, rgba(36, 105, 81, 0.08), transparent 26%),
      linear-gradient(180deg, rgba(255, 252, 247, 0.98), rgba(247, 241, 233, 0.92));
  }

  .identity {
    display: grid;
    gap: 1rem;
    min-width: 0;
  }

  .brand-block {
    min-width: 0;
  }

  .eyebrow {
    margin: 0 0 0.3rem;
    color: var(--text-soft);
    font-size: 0.7rem;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    font-weight: 600;
  }

  h1 {
    margin: 0;
    font-size: 1.6rem;
    line-height: 1.05;
    letter-spacing: -0.035em;
  }

  .path {
    margin: 0.4rem 0 0;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.94rem;
  }

  .session-summary {
    margin: 0.55rem 0 0;
    color: var(--text-muted);
    font-size: 0.9rem;
    max-width: 56rem;
    line-height: 1.5;
  }

  .repo-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 0.55rem;
  }

  .pill {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.46rem 0.74rem;
    border-radius: 999px;
    border: 1px solid rgba(102, 83, 62, 0.14);
    background: rgba(255, 255, 255, 0.64);
    color: var(--text-soft);
    text-decoration: none;
    font-size: 0.77rem;
    box-shadow: var(--shadow-edge);
  }

  .pill.ok {
    background: rgba(220, 234, 224, 0.88);
    color: var(--accent-strong);
    border-color: rgba(36, 105, 81, 0.14);
  }

  .pill.warning {
    background: rgba(244, 230, 208, 0.88);
    color: var(--warning);
    border-color: rgba(141, 97, 48, 0.14);
  }

  .pill.link:hover {
    color: var(--text);
  }

  .pill.tag {
    background: rgba(246, 240, 230, 0.92);
  }

  .pill.remote {
    background: rgba(226, 235, 244, 0.92);
    color: #34526d;
    border-color: rgba(52, 82, 109, 0.14);
  }

  .actions {
    display: grid;
    gap: 0.9rem;
    align-content: start;
    justify-items: end;
  }

  .action-group {
    display: flex;
    justify-content: flex-end;
    gap: 0.6rem;
    flex-wrap: wrap;
    padding: 0.35rem;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.55);
    border: 1px solid rgba(102, 83, 62, 0.1);
    box-shadow: var(--shadow-edge);
  }

  button {
    border: 1px solid rgba(102, 83, 62, 0.12);
    border-radius: 999px;
    padding: 0.62rem 0.96rem;
    background: rgba(255, 255, 255, 0.76);
    color: var(--text);
    cursor: pointer;
    font-weight: 600;
    transition: transform 120ms ease, box-shadow 120ms ease, opacity 120ms ease,
      border-color 120ms ease, background 120ms ease;
    box-shadow: 0 8px 18px rgba(22, 26, 32, 0.08);
  }

  button:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: var(--shadow);
  }

  button:disabled {
    opacity: 0.42;
    cursor: not-allowed;
    box-shadow: none;
  }

  .primary {
    background: linear-gradient(180deg, #2e7a61, #225d49);
    color: #fcfffd;
    border-color: rgba(34, 93, 73, 0.28);
    box-shadow: 0 12px 24px rgba(35, 93, 73, 0.2);
  }

  .secondary {
    background: rgba(245, 237, 225, 0.96);
    color: var(--warning);
    border-color: rgba(141, 97, 48, 0.12);
  }

  .link-button {
    display: inline-flex;
    align-items: center;
    text-decoration: none;
    border: 1px solid rgba(141, 97, 48, 0.12);
    border-radius: 999px;
    padding: 0.62rem 0.96rem;
    font-weight: 600;
    background: rgba(245, 237, 225, 0.96);
    color: var(--warning);
    box-shadow: 0 8px 18px rgba(22, 26, 32, 0.08);
    transition: transform 120ms ease, box-shadow 120ms ease, opacity 120ms ease;
  }

  .link-button:hover {
    transform: translateY(-1px);
    box-shadow: var(--shadow);
  }

  .ghost {
    background: rgba(255, 255, 255, 0.76);
  }

  .status-copy {
    display: grid;
    gap: 0.28rem;
    justify-items: end;
  }

  .status-line {
    margin: 0;
    color: var(--text);
    text-align: right;
    max-width: 24rem;
    justify-self: end;
    font-size: 0.86rem;
    font-weight: 600;
  }

  .repo-summary {
    margin: 0;
    color: var(--text-soft);
    text-align: right;
    max-width: 28rem;
    font-size: 0.8rem;
    line-height: 1.45;
  }

  @media (max-width: 1200px) {
    .header {
      grid-template-columns: 1fr;
    }

    .actions {
      justify-items: start;
    }

    .action-group,
    .status-copy,
    .status-line,
    .repo-summary {
      justify-content: flex-start;
      justify-self: start;
      text-align: left;
    }

    .action-group {
      border-radius: 18px;
    }
  }
</style>
