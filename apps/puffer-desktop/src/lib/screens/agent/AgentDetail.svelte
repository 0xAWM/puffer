<script lang="ts">
  import Puffer from "../../design/Puffer.svelte";
  import Icon from "../../design/Icon.svelte";
  import ConversationView from "./ConversationView.svelte";
  import DiffView from "../../components/DiffView.svelte";
  import FilesPane from "./FilesPane.svelte";
  import TerminalPane from "./TerminalPane.svelte";
  import {
    AGENT_STATE_LABELS,
    agentPufferState,
    type AgentStatus,
    type MockAgent
  } from "../../data/mockProjects";
  import type {
    PermissionTimelineItem,
    SessionDetail,
    SessionListItem,
    TimelineItem
  } from "../../types";
  import type { AgentState } from "../../shell/tweaks";

  type Props = {
    // Mock agent (from workspace board click) — provides display identity.
    agent?: MockAgent | null;
    // Live session data from the backend.
    session: SessionListItem | null;
    sessionDetail: SessionDetail | null;
    timeline: TimelineItem[];
    pendingPermissions: PermissionTimelineItem[];
    loading: boolean;
    turnRunning?: boolean;
    onBack: () => void;
    onSubmitMessage: (message: string) => void;
    onResolvePermission: (permissionId: string, choice: string) => void;
    onCancelTurn?: () => void;
  };

  let {
    agent = null,
    session,
    sessionDetail,
    timeline,
    pendingPermissions,
    loading,
    turnRunning = false,
    onBack,
    onSubmitMessage,
    onResolvePermission,
    onCancelTurn
  }: Props = $props();

  type Tab = "chat" | "diff" | "terminal" | "files";
  let tab = $state<Tab>("chat");

  // Identity for the header: prefer the mock agent, fall back to real session.
  let displayName = $derived(
    agent?.name ?? (session?.displayName ?? session?.title ?? "Session")
  );
  let displayTitle = $derived(
    agent?.title ?? session?.title ?? (session?.note ?? "")
  );
  let displayBranch = $derived(
    agent?.branch ?? sessionDetail?.repoStatus?.branch ?? ""
  );
  let displayProject = $derived(
    agent?.project ?? (session?.folderPath?.split("/").pop() ?? "")
  );
  let displayWorktree = $derived(agent?.worktree ?? "");
  let status = $derived<AgentStatus>(
    agent?.status ?? inferStatusFromSession(sessionDetail)
  );

  function inferStatusFromSession(d: SessionDetail | null): AgentStatus {
    if (!d) return "idle";
    const hasPending = d.timeline.some((t) => t.kind === "permission");
    if (hasPending) return "awaiting";
    if (d.repoStatus?.pullRequest) return "review";
    if (d.repoStatus?.hasUncommittedChanges) return "running";
    return "idle";
  }

  let pufferState = $derived<AgentState>(agentPufferState(status));
  let diffCount = $derived(timeline.filter((t) => t.kind === "diff").length);
</script>

<div class="pf-agent-detail">
  <div class="pf-agent-detail-head">
    <button type="button" class="pf-agent-back" onclick={onBack} title="Back to workspace" aria-label="Back">
      <Icon name="chevL" size={13} />
    </button>
    <Puffer size={20} state={pufferState} />
    <div class="pf-agent-identity">
      <div class="name">
        {displayName}
        {#if displayTitle}
          <span class="sep">·</span>
          <span class="title">{displayTitle}</span>
        {/if}
      </div>
      <div class="meta">
        {#if displayProject}
          <span class="mono">{displayProject}</span>
          <span class="sep">·</span>
        {/if}
        {#if displayBranch}
          <span class="branch mono"><Icon name="branch" size={10} />{displayBranch}</span>
          {#if displayWorktree}
            <span class="sep">·</span>
          {/if}
        {/if}
        {#if displayWorktree}
          <span class="mono">{displayWorktree}</span>
        {/if}
      </div>
    </div>
    <span class="pf-agent-status-pill" data-status={status}>
      {#if status === "running"}
        <span class="pip"></span>
      {/if}
      {AGENT_STATE_LABELS[status] ?? status}
    </span>
    <div class="pf-agent-tabs">
      <button class="pf-agent-tab" class:on={tab === "chat"} onclick={() => (tab = "chat")}>
        <Icon name="sparkles" size={12} />Chat
      </button>
      <button class="pf-agent-tab" class:on={tab === "diff"} onclick={() => (tab = "diff")}>
        <Icon name="git" size={12} />Diff
        {#if diffCount > 0}
          <span class="pf-agent-tab-badge">{diffCount}</span>
        {/if}
      </button>
      <button class="pf-agent-tab" class:on={tab === "terminal"} onclick={() => (tab = "terminal")}>
        <Icon name="terminal" size={12} />Terminal
      </button>
      <button class="pf-agent-tab" class:on={tab === "files"} onclick={() => (tab = "files")}>
        <Icon name="folder" size={12} />Files
      </button>
    </div>
  </div>

  <div class="pf-agent-detail-body">
    {#if tab === "chat"}
      <ConversationView
        session={session}
        agentName={displayName}
        agentState={pufferState}
        timeline={timeline}
        pendingPermissions={pendingPermissions}
        loading={loading}
        turnRunning={turnRunning}
        onSubmitMessage={onSubmitMessage}
        onResolvePermission={onResolvePermission}
        onCancelTurn={onCancelTurn}
      />
    {:else if tab === "diff"}
      {#if sessionDetail?.latestDiff}
        <div class="diff-wrap">
          <DiffView diff={sessionDetail.latestDiff} />
        </div>
      {:else}
        <div class="pane-empty">
          <Icon name="git" size={20} color="var(--muted-foreground)" />
          <div class="title">No diff yet</div>
          <div class="sub">When the agent edits files, the most-recent patch will land here.</div>
        </div>
      {/if}
    {:else if tab === "terminal"}
      <TerminalPane cwd={session?.cwd ?? displayProject} />
    {:else if tab === "files"}
      <FilesPane cwd={session?.cwd ?? displayProject} />
    {/if}
  </div>
</div>

<style>
  .pf-agent-detail {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--background);
  }
  .pf-agent-detail-head {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    background: color-mix(in oklab, var(--background) 96%, var(--muted));
    border-bottom: 1px solid var(--border);
    min-height: 52px;
  }
  .pf-agent-back {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--background);
    color: var(--muted-foreground);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex-shrink: 0;
    transition: background 120ms, color 120ms;
  }
  .pf-agent-back:hover { background: var(--accent); color: var(--foreground); }
  .pf-agent-identity {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 0 1 auto;
    max-width: 420px;
  }
  .pf-agent-identity .name {
    font-size: 14px;
    font-weight: 600;
    letter-spacing: -0.01em;
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }
  .pf-agent-identity .name .sep { color: var(--muted-foreground); opacity: 0.5; }
  .pf-agent-identity .name .title {
    font-weight: 500;
    color: var(--foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pf-agent-identity .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--muted-foreground);
  }
  .pf-agent-identity .meta .mono { font-family: var(--font-mono); }
  .pf-agent-identity .meta .sep { opacity: 0.4; }
  .pf-agent-identity .meta .branch {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--muted);
  }

  .pf-agent-status-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 10.5px;
    font-weight: 600;
    font-family: var(--font-mono);
    padding: 3px 8px;
    border-radius: 999px;
    background: var(--muted);
    color: var(--muted-foreground);
    text-transform: lowercase;
    flex-shrink: 0;
    margin-left: auto;
  }
  .pf-agent-status-pill[data-status="running"]  { background: color-mix(in oklab, oklch(0.7 0.17 70) 15%, var(--background)); color: oklch(0.55 0.17 70); }
  .pf-agent-status-pill[data-status="awaiting"] { background: color-mix(in oklab, oklch(0.72 0.18 30) 16%, var(--background)); color: oklch(0.55 0.2 30); }
  .pf-agent-status-pill[data-status="review"]   { background: color-mix(in oklab, oklch(0.7 0.16 40) 15%, var(--background));  color: oklch(0.55 0.17 40); }
  .pf-agent-status-pill .pip {
    width: 6px; height: 6px; border-radius: 50%;
    background: oklch(0.7 0.17 70);
    animation: pf-pulse-dot 1.6s infinite;
  }
  @keyframes pf-pulse-dot {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }

  .pf-agent-tabs {
    display: flex;
    gap: 1px;
    background: var(--muted);
    padding: 3px;
    border-radius: 8px;
    flex-shrink: 0;
  }
  .pf-agent-tab {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    font-size: 12px;
    font-weight: 500;
    color: var(--muted-foreground);
    border: 0;
    background: transparent;
    border-radius: 5px;
    cursor: pointer;
    transition: background 120ms, color 120ms;
    font: inherit;
  }
  .pf-agent-tab:hover { color: var(--foreground); }
  .pf-agent-tab.on {
    background: var(--background);
    color: var(--foreground);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.06);
  }
  .pf-agent-tab-badge {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 1px 5px;
    border-radius: 3px;
    background: oklch(0.7 0.16 40);
    color: white;
    margin-left: 2px;
  }

  .pf-agent-detail-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .diff-wrap {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .pane-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 40px;
    color: var(--muted-foreground);
    text-align: center;
  }
  .pane-empty .title { font-size: 14px; font-weight: 600; color: var(--foreground); }
  .pane-empty .sub { font-size: 12.5px; max-width: 360px; line-height: 1.55; }

  @media (max-width: 720px) {
    .pf-agent-detail-head { flex-wrap: wrap; row-gap: 6px; padding: 8px 10px; }
    .pf-agent-tabs { order: 3; width: 100%; overflow-x: auto; }
    .pf-agent-status-pill { order: 2; margin-left: 0; }
  }
</style>
