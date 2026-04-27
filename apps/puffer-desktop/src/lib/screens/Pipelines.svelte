<script lang="ts">
  import "../design/pipeline.css";

  import { onMount } from "svelte";
  import { loadWorkflowSnapshot } from "../api/desktop";
  import Icon, { type IconName } from "../design/Icon.svelte";
  import Puffer from "../design/Puffer.svelte";
  import type {
    WorkflowDefinition,
    WorkflowPipelineNode,
    WorkflowRun,
    WorkflowRunNode,
    WorkflowRunStatus,
    WorkflowSnapshot
  } from "../types";

  type GraphNode = {
    id: string;
    type: "trigger" | "agent";
    title: string;
    subtitle: string;
    node?: WorkflowPipelineNode;
  };

  type GraphEdge = {
    from: string;
    to: string;
    label?: string;
  };

  const COL_W = 150;
  const NODE_W = 118;
  const NODE_H = 64;
  const PAD_L = 14;
  const PAD_T = 18;

  let snapshot = $state<WorkflowSnapshot>({ workflows: [], runs: [] });
  let workflowSlug = $state("");
  let runIdx = $state<number | null>(null);
  let stepIdx = $state<number | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let workflows = $derived(snapshot.workflows);
  let workflow = $derived(
    workflows.find((item) => item.slug === workflowSlug) ?? workflows[0] ?? null
  );
  let runs = $derived(
    workflow ? snapshot.runs.filter((run) => run.workflow_slug === workflow.slug) : []
  );
  let run = $derived(
    runs.find((item) => item.idx === runIdx) ?? runs[0] ?? null
  );
  let graphNodes = $derived(workflow ? nodesFor(workflow) : []);
  let graphEdges = $derived(workflow ? edgesFor(workflow) : []);
  let graphWidth = $derived(PAD_L * 2 + Math.max(0, graphNodes.length - 1) * COL_W + NODE_W);
  let graphHeight = $derived(PAD_T * 2 + NODE_H);
  let currentStepIndex = $derived(
    run ? (stepIdx === null ? Math.max(0, run.nodes.length - 1) : Math.min(stepIdx, run.nodes.length - 1)) : 0
  );
  let currentNode = $derived(run?.nodes[currentStepIndex] ?? null);
  let visited = $derived(new Set((run?.nodes ?? []).slice(0, currentStepIndex + 1).map((node) => node.id)));
  let activeNode = $derived(currentNode?.id ?? "");
  let isLive = $derived(run?.status === "running" && stepIdx === null);

  let wrapEl = $state<HTMLDivElement | undefined>();
  let scale = $state(0.8);

  function measure() {
    if (!wrapEl || graphWidth <= 0) return;
    const cw = wrapEl.clientWidth;
    if (!cw) return;
    scale = Math.min(1, cw / graphWidth);
  }

  onMount(() => {
    void refresh();
    measure();
    const ro = new ResizeObserver(measure);
    if (wrapEl) ro.observe(wrapEl);
    window.addEventListener("resize", measure);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", measure);
    };
  });

  $effect(() => {
    workflowSlug;
    runIdx = null;
    stepIdx = null;
  });

  async function refresh() {
    loading = true;
    error = null;
    try {
      const next = await loadWorkflowSnapshot();
      snapshot = {
        workflows: next.workflows,
        runs: [...next.runs].sort((a, b) => b.idx - a.idx)
      };
      if (!workflowSlug && next.workflows.length > 0) workflowSlug = next.workflows[0].slug;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
      setTimeout(measure, 0);
    }
  }

  function selectWorkflow(slug: string) {
    workflowSlug = slug;
  }

  function selectRun(idx: number) {
    runIdx = idx;
    stepIdx = null;
  }

  function stepToIdx(i: number) {
    stepIdx = i;
  }

  function nodesFor(item: WorkflowDefinition): GraphNode[] {
    return [
      {
        id: "trigger",
        type: "trigger",
        title: triggerTitle(item),
        subtitle: item.enabled ? "enabled" : "disabled"
      },
      ...item.pipeline.nodes.map((node) => ({
        id: node.id,
        type: "agent" as const,
        title: node.agent ?? node.id,
        subtitle: node.model ?? "default",
        node
      }))
    ];
  }

  function edgesFor(item: WorkflowDefinition): GraphEdge[] {
    const edges: GraphEdge[] = [];
    for (const node of item.pipeline.nodes) {
      const deps = node.depends_on ?? [];
      if (deps.length === 0) {
        edges.push({ from: "trigger", to: node.id });
      } else {
        for (const dep of deps) edges.push({ from: dep, to: node.id, label: "after" });
      }
    }
    return edges;
  }

  function triggerTitle(item: WorkflowDefinition): string {
    if (item.trigger.type === "cron") return item.trigger.cron;
    return item.trigger.source_topic;
  }

  function workflowLatestRun(slug: string): WorkflowRun | undefined {
    return snapshot.runs.find((item) => item.workflow_slug === slug);
  }

  function nodeXY(id: string) {
    const idx = Math.max(0, graphNodes.findIndex((node) => node.id === id));
    const x = PAD_L + idx * COL_W;
    return { x, y: PAD_T, cx: x + NODE_W / 2, cy: PAD_T + NODE_H / 2 };
  }

  function pathFor(fromId: string, toId: string) {
    const pa = nodeXY(fromId);
    const pb = nodeXY(toId);
    const x1 = pa.x + NODE_W - 4;
    const y1 = pa.cy;
    const x2 = pb.x + 4;
    const y2 = pb.cy;
    const mx = (x1 + x2) / 2;
    return `M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`;
  }

  function edgeMidY(from: string, to: string) {
    const pa = nodeXY(from);
    const pb = nodeXY(to);
    return (pa.cy + pb.cy) / 2 - 16;
  }

  function edgeMidX(from: string, to: string) {
    const pa = nodeXY(from);
    const pb = nodeXY(to);
    const x1 = pa.x + NODE_W - 4;
    const x2 = pb.x + 4;
    return (x1 + x2) / 2;
  }

  function edgeClass(from: string, to: string) {
    const visitedEdge = visited.has(from) && visited.has(to);
    return "pf-pipe-edge" + (visitedEdge ? " on-path visited" : "");
  }

  function nodeState(node: GraphNode): string {
    if (node.id === "trigger") return run ? "visited" : "idle";
    const record = run?.nodes.find((item) => item.id === node.id);
    if (!record) return "idle";
    if (record.status === "running") return "active";
    if (record.status === "failed") return "failed";
    if (record.status === "skipped") return "blocked";
    if (record.status === "completed") return "visited";
    return visited.has(node.id) ? "queued" : "idle";
  }

  function statusColor(status: WorkflowRunStatus): string {
    return (
      {
        pending: "var(--pf-run-skipped)",
        running: "var(--pf-run-running)",
        completed: "var(--pf-run-done)",
        failed: "var(--pf-run-failed)",
        skipped: "var(--pf-run-skipped)"
      } as const
    )[status];
  }

  function runElapsed(item: WorkflowRun): string {
    const end = item.ended_at_ms ?? Date.now();
    const seconds = Math.max(0, Math.round((end - item.started_at_ms) / 1000));
    if (seconds < 60) return `${seconds}s`;
    return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  }

  function runWhen(item: WorkflowRun): string {
    return new Date(item.started_at_ms).toLocaleString([], {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit"
    });
  }

  function nodeIcon(node: GraphNode): IconName {
    if (node.type === "trigger") {
      return workflow?.trigger.type === "cron" ? "clock" : "bolt";
    }
    return "panel";
  }

  function stepLabel(node: WorkflowRunNode): string {
    if (node.status === "completed") return "completed";
    if (node.status === "failed") return "failed";
    if (node.status === "skipped") return "skipped";
    if (node.status === "running") return "running";
    return "pending";
  }
</script>

<div class="pf-pipe">
  <div class="pf-pipe-top">
    <div class="pf-pipe-top-id">
      <span class="pf-pipe-chip">Workflow</span>
      <strong>{workflow?.slug ?? "No workflows"}</strong>
      {#if workflow}
        <span class="pf-pipe-hash">{workflow.pipeline.name}</span>
      {/if}
    </div>
    <div class="pf-pipe-top-right">
      <button type="button" class="sc-btn" data-variant="ghost" data-size="sm" onclick={refresh}>
        <Icon name="refresh" size={12} />Refresh
      </button>
    </div>
  </div>

  <div class="pf-pipe-body">
    <div class="pf-pipe-runs">
      <div class="pf-pipe-runs-head">
        <span>Workflows</span>
        <span class="count">{workflows.length}</span>
      </div>
      {#if loading}
        <div class="pf-pipe-empty">Loading workflows...</div>
      {:else if error}
        <div class="pf-pipe-empty">{error}</div>
      {:else if workflows.length === 0}
        <div class="pf-pipe-empty">No workflows registered.</div>
      {:else}
        {#each workflows as item (item.slug)}
          {@const latest = workflowLatestRun(item.slug)}
          <button
            type="button"
            class="pf-run-row"
            data-selected={item.slug === workflow?.slug}
            data-state={latest?.status ?? "pending"}
            onclick={() => selectWorkflow(item.slug)}
          >
            <div class="pf-run-head">
              <span class="pf-run-pip {latest?.status ?? 'pending'}"></span>
              <span class="pf-run-label">{item.slug}</span>
              <span class="pf-run-when">{item.enabled ? "enabled" : "disabled"}</span>
            </div>
            <div class="pf-run-title">{item.pipeline.name}</div>
            <div class="pf-run-meta">
              <span>{triggerTitle(item)}</span>
              <span class="sep">·</span>
              <span class="mono">{item.pipeline.nodes.length} nodes</span>
            </div>
          </button>
        {/each}
      {/if}
    </div>

    <div class="pf-pipe-main">
      {#if workflow}
        <div class="pf-run-header">
          <span class="pf-run-header-pip" style="background: {run ? statusColor(run.status) : 'var(--pf-run-skipped)'};"></span>
          <span class="pf-run-header-label">{workflow.pipeline.name}</span>
          <span class="pf-run-header-state" data-state={run?.status ?? "pending"}>{run?.status ?? "no runs"}</span>
          <span class="pf-run-header-title">{triggerTitle(workflow)}</span>
          <span class="pf-run-header-meta-group">
            <span class="pf-run-header-dim"><Icon name="wrench" size={11} /><span class="mono">{workflow.pipeline.nodes.length} nodes</span></span>
            {#if run}
              <span class="pf-run-header-dim"><Icon name="clock" size={11} /><span class="mono">{runElapsed(run)}</span></span>
            {/if}
          </span>
        </div>

        <div class="pf-pipe-graph-wrap">
          <div bind:this={wrapEl} class="pf-pipe-graph-scaler" style="height: {graphHeight * scale}px;">
            <div
              class="pf-pipe-graph"
              style="width: {graphWidth}px; height: {graphHeight}px; transform: scale({scale}); transform-origin: top left;"
            >
              <svg class="pf-pipe-graph-svg" viewBox="0 0 {graphWidth} {graphHeight}" width={graphWidth} height={graphHeight}>
                <defs>
                  <marker id="pf-pipe-arr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
                    <path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
                  </marker>
                </defs>
                {#each graphEdges as edge, i (i)}
                  {@const d = pathFor(edge.from, edge.to)}
                  {@const midY = edgeMidY(edge.from, edge.to)}
                  {@const midX = edgeMidX(edge.from, edge.to)}
                  {@const visEdge = visited.has(edge.from) && visited.has(edge.to)}
                  <g class={edgeClass(edge.from, edge.to)}>
                    <path d={d} fill="none" stroke-width={visEdge ? 2 : 1.2} />
                    <path d={d} fill="none" stroke-width="1.2" class="arr-head" marker-end="url(#pf-pipe-arr)" />
                    {#if isLive && visEdge && activeNode === edge.to}
                      <circle r="3.5" class="pf-pipe-edge-dot">
                        <animateMotion dur="1.8s" repeatCount="indefinite" path={d} />
                      </circle>
                    {/if}
                    {#if edge.label}
                      <g transform="translate({midX}, {midY})">
                        <rect x="-32" y="-9" width="64" height="18" rx="9" fill="var(--background)" class="pf-pipe-edge-pill"></rect>
                        <text x="0" y="4" text-anchor="middle" font-size="10.5" font-family="var(--font-mono)" class="pf-pipe-edge-text">
                          {edge.label}
                        </text>
                      </g>
                    {/if}
                  </g>
                {/each}
              </svg>

              {#each graphNodes as node (node.id)}
                {@const p = nodeXY(node.id)}
                {@const st = nodeState(node)}
                <div
                  class="pf-pipe-node"
                  style="left: {p.x}px; top: {p.y}px; width: {NODE_W}px; height: {NODE_H}px;"
                  data-type={node.type}
                  data-state={st}
                >
                  <div class="pf-pipe-node-head">
                    {#if node.type === "agent"}
                      <Puffer size={20} state={activeNode === node.id && isLive ? "running" : "idle"} />
                    {:else}
                      <span class="pf-pipe-node-ico">
                        <Icon name={nodeIcon(node)} size={12} />
                      </span>
                    {/if}
                    <div class="pf-pipe-node-meta">
                      <span class="name">{node.title}</span>
                      <span class="sub">{node.subtitle}</span>
                    </div>
                    {#if activeNode === node.id && isLive}
                      <span class="pf-pipe-node-halo"></span>
                    {/if}
                  </div>
                  <span class="pf-pipe-node-stub left"></span>
                  <span class="pf-pipe-node-stub right"></span>
                </div>
              {/each}
            </div>
          </div>
        </div>

        <div class="pf-pipe-traj">
          <div class="pf-pipe-traj-head">
            <Icon name="terminal" size={12} />
            <span>Runs</span>
            <span class="pf-pipe-traj-count">{runs.length}</span>
          </div>

          <div class="pf-pipe-run-list">
            {#each runs as item (item.idx)}
              <button
                type="button"
                class="pf-run-row"
                data-selected={item.idx === run?.idx}
                data-state={item.status}
                onclick={() => selectRun(item.idx)}
              >
                <div class="pf-run-head">
                  <span class="pf-run-pip {item.status}"></span>
                  <span class="pf-run-label">#{item.idx}</span>
                  <span class="pf-run-when">{runWhen(item)}</span>
                </div>
                <div class="pf-run-title">{item.status}</div>
                <div class="pf-run-meta">
                  <span class="mono">{runElapsed(item)}</span>
                  <span class="sep">·</span>
                  <span>{item.nodes.length} steps</span>
                </div>
              </button>
            {:else}
              <div class="pf-pipe-empty">No runs recorded.</div>
            {/each}
          </div>

          {#if run}
            <div class="pf-pipe-traj-head">
              <Icon name="terminal" size={12} />
              <span>Trajectory</span>
              <span class="pf-pipe-traj-count">{run.nodes.length} steps</span>
              <span style="flex: 1;"></span>
              <button
                type="button"
                class="sc-btn"
                data-variant="ghost"
                data-size="sm"
                disabled={currentStepIndex <= 0}
                onclick={() => stepToIdx(Math.max(0, currentStepIndex - 1))}
                aria-label="Previous step"
              >
                <Icon name="chevL" size={12} />
              </button>
              <button
                type="button"
                class="sc-btn"
                data-variant="ghost"
                data-size="sm"
                disabled={currentStepIndex >= run.nodes.length - 1}
                onclick={() => stepToIdx(Math.min(run.nodes.length - 1, currentStepIndex + 1))}
                aria-label="Next step"
              >
                <Icon name="chevR" size={12} />
              </button>
            </div>

            <div class="pf-pipe-traj-list">
              {#each run.nodes as node, i (i)}
                <button
                  type="button"
                  class="pf-pipe-traj-row"
                  data-step={i}
                  data-current={i === currentStepIndex}
                  data-past={i < currentStepIndex}
                  data-status={node.status}
                  onclick={() => stepToIdx(i)}
                >
                  <span class="t">#{i + 1}</span>
                  <span class="rail">
                    <span class="dot" data-kind="agent" data-status={node.status}></span>
                  </span>
                  <span><span class="lane-chip agent">{node.id}</span></span>
                  <span class="body">
                    <span class="body-title">{stepLabel(node)}</span>
                    {#if node.output}
                      <span class="body-arg">{node.output}</span>
                    {:else if node.error}
                      <span class="body-arg">{node.error}</span>
                    {/if}
                  </span>
                  <span class="status {node.status}">
                    {#if node.status === "running"}
                      <span class="dot-live"></span>running
                    {:else}
                      {node.status}
                    {/if}
                  </span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {:else}
        <div class="pf-pipe-empty">Register workflows with the CLI or Puffer agent.</div>
      {/if}
    </div>
  </div>
</div>

<style>
  .pf-pipe-empty {
    color: var(--muted-foreground);
    font-size: 12px;
    padding: 14px;
  }
  .pf-pipe-run-list {
    display: grid;
    gap: 8px;
    padding: 10px;
  }
  .pf-pipe-traj-row {
    all: unset;
    display: grid;
    grid-template-columns: 54px 22px 88px 1fr auto;
    align-items: center;
    gap: 10px;
    padding: 5px 16px;
    font-size: 12.5px;
    cursor: pointer;
    border-left: 2px solid transparent;
    transition: background 100ms;
    min-height: 30px;
  }
  .pf-run-pip.completed,
  .pf-run-pip.pending {
    background: var(--pf-run-done);
  }
  .pf-run-header-state[data-state="completed"] {
    background: color-mix(in oklab, var(--pf-run-done) 20%, var(--background));
    color: var(--pf-run-done);
  }
  .pf-run-header-state[data-state="pending"] {
    background: var(--muted);
    color: var(--muted-foreground);
  }
  .pf-pipe-traj-row .rail .dot[data-status="completed"] {
    background: var(--pf-run-done);
    border-color: var(--pf-run-done);
  }
  .pf-pipe-traj-row .rail .dot[data-status="failed"] {
    background: var(--pf-run-failed);
    border-color: var(--pf-run-failed);
  }
  .pf-pipe-traj-row .status.completed {
    color: var(--pf-run-done);
  }
  .pf-pipe-traj-row .status.failed {
    color: var(--pf-run-failed);
  }
</style>
