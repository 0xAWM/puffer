<script lang="ts">
  import "../design/pipeline.css";

  import { onMount } from "svelte";
  import Icon, { type IconName } from "../design/Icon.svelte";
  import Puffer from "../design/Puffer.svelte";
  import {
    PIPE_NODES,
    PIPE_EDGES,
    RUNS,
    SINK_ICONS,
    TRIGGER_ICONS,
    type PipeNode,
    type PipeRun,
    type RunState
  } from "../data/mockPipeline";

  // Layout constants
  const COL_W = 130;
  const NODE_W = 110;
  const NODE_H = 64;
  const PAD_L = 14;
  const PAD_T = 18;
  const NODE_X: Record<string, number> = { trg: 0, a1: 1, a2: 2, g: 3, a3: 4, s: 5 };
  const GRAPH_W = PAD_L * 2 + (PIPE_NODES.length - 1) * COL_W + NODE_W;
  const GRAPH_H = PAD_T * 2 + NODE_H;

  function nodeXY(id: string) {
    const col = NODE_X[id];
    return { x: PAD_L + col * COL_W, y: PAD_T, cx: PAD_L + col * COL_W + NODE_W / 2, cy: PAD_T + NODE_H / 2 };
  }

  let runId = $state("r8");
  let stepIdx = $state<number | null>(null);

  let run = $derived(RUNS.find((r) => r.id === runId) ?? RUNS[0]);
  let effectiveStep = $derived(
    stepIdx === null ? Math.max(0, run.steps.length - 1) : Math.min(stepIdx, run.steps.length - 1)
  );
  let currentStep = $derived(run.steps[effectiveStep]);
  let visited = $derived(new Set(run.steps.slice(0, effectiveStep + 1).map((s) => s.node)));
  let activeNode = $derived(currentStep?.node ?? "");
  let pathSet = $derived(new Set(run.path));
  let isLive = $derived(stepIdx === null && run.state === "running");
  let stats = $derived(statsFor(run));

  // Scale graph to container
  let wrapEl: HTMLDivElement | undefined;
  let scale = $state(0.8);

  function measure() {
    if (!wrapEl) return;
    const cw = wrapEl.clientWidth;
    if (!cw) return;
    scale = Math.min(1, cw / GRAPH_W);
  }

  onMount(() => {
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
    runId;
    stepIdx = null;
  });

  function selectRun(id: string) {
    runId = id;
  }

  function stepToIdx(i: number) {
    stepIdx = i;
  }

  function stateColor(s: RunState): string {
    return (
      {
        running: "var(--pf-run-running)",
        done: "var(--pf-run-done)",
        failed: "var(--pf-run-failed)",
        blocked: "var(--pf-run-blocked)",
        skipped: "var(--pf-run-skipped)"
      } as const
    )[s];
  }

  function statsFor(r: PipeRun) {
    if (r.state === "running") return { cost: "$0.18", tokens: "42k", tools: 7, files: 3 };
    if (r.state === "done") return { cost: "$0.42", tokens: "118k", tools: 22, files: 5 };
    if (r.state === "failed") return { cost: "$0.06", tokens: "14k", tools: 3, files: 1 };
    if (r.state === "blocked") return { cost: "$0.31", tokens: "87k", tools: 14, files: 4 };
    return { cost: "$0.02", tokens: "3k", tools: 1, files: 0 };
  }

  function nodeState(n: PipeNode): string {
    if (visited.has(n.id)) {
      if (activeNode === n.id && isLive) return "active";
      if (run.state === "failed" && pathSet.has(n.id) && n.id === "a1") return "failed";
      if (run.state === "blocked" && activeNode === n.id) return "blocked";
      return "visited";
    }
    return pathSet.has(n.id) ? "queued" : "idle";
  }

  function nodeIcon(n: PipeNode): IconName {
    if (n.type === "trigger") return (TRIGGER_ICONS[n.kind ?? ""] as IconName) ?? "bolt";
    if (n.type === "sink") return (SINK_ICONS[n.kind ?? ""] as IconName) ?? "check";
    return "panel";
  }

  function findNode(id: string): PipeNode | undefined {
    return PIPE_NODES.find((n) => n.id === id);
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
    const onPath = pathSet.has(from) && pathSet.has(to);
    const visitedEdge = visited.has(from) && visited.has(to);
    return (
      "pf-pipe-edge" + (onPath ? " on-path" : "") + (visitedEdge ? " visited" : "")
    );
  }
</script>

<div class="pf-pipe">
  <div class="pf-pipe-top">
    <div class="pf-pipe-top-id">
      <span class="pf-pipe-chip">Pipeline</span>
      <strong>pr-review</strong>
      <span class="pf-pipe-hash">main@v14</span>
    </div>
    <div class="pf-pipe-top-right">
      <button type="button" class="sc-btn" data-variant="ghost" data-size="sm">
        <Icon name="settings" size={12} />Configure
      </button>
      <button type="button" class="sc-btn" data-variant="outline" data-size="sm">
        <Icon name="pause2" size={12} />Pause
      </button>
      <button type="button" class="sc-btn" data-variant="default" data-size="sm">
        <Icon name="play" size={12} />Run manually
      </button>
    </div>
  </div>

  <div class="pf-pipe-body">
    <div class="pf-pipe-runs">
      <div class="pf-pipe-runs-head">
        <span>Runs</span>
        <span class="count">{RUNS.length}</span>
      </div>
      {#each RUNS as r (r.id)}
        {@const rPathSet = new Set(r.path)}
        <button
          type="button"
          class="pf-run-row"
          data-selected={r.id === runId}
          data-state={r.state}
          onclick={() => selectRun(r.id)}
        >
          <div class="pf-run-head">
            <span class="pf-run-pip {r.state}"></span>
            <span class="pf-run-label">{r.label}</span>
            <span class="pf-run-when">{r.when}</span>
          </div>
          <div class="pf-run-title">{r.title}</div>
          <div class="pf-run-meta">
            <span>{r.author}</span>
            <span class="sep">·</span>
            <span class="mono">{r.elapsed}</span>
          </div>
          <div class="pf-run-mini">
            {#each PIPE_NODES as n, i (n.id)}
              {#if i > 0}
                {@const prevOn = rPathSet.has(PIPE_NODES[i - 1].id)}
                {@const hit = rPathSet.has(n.id)}
                <span class="seg {hit && prevOn ? 'on' : ''} {r.state === 'failed' && i === r.path.length ? 'fail' : ''}"></span>
              {/if}
              {@const hit = rPathSet.has(n.id)}
              {@const cur = r.current === n.id}
              <span class="dot {hit ? 'on' : ''} {cur ? 'cur' : ''} {r.state}"></span>
            {/each}
          </div>
        </button>
      {/each}
    </div>

    <div class="pf-pipe-main">
      <div class="pf-run-header">
        <span class="pf-run-header-pip" style="background: {stateColor(run.state)};"></span>
        <span class="pf-run-header-label">Run&nbsp;{run.label}</span>
        <span class="pf-run-header-state" data-state={run.state}>{run.state}</span>
        <span class="pf-run-header-title">{run.title}</span>
        <span class="pf-run-header-meta-group">
          <span class="pf-run-header-dim"><Icon name="clock" size={11} /><span class="mono">{run.elapsed}</span></span>
          <span class="pf-run-header-dim"><Icon name="coin" size={11} /><span class="mono">{stats.cost}</span></span>
          <span class="pf-run-header-dim"><Icon name="token" size={11} /><span class="mono">{stats.tokens}</span></span>
          <span class="pf-run-header-dim"><Icon name="wrench" size={11} /><span class="mono">{stats.tools}</span></span>
          <span class="pf-run-header-dim"><Icon name="file" size={11} /><span class="mono">{stats.files}</span></span>
        </span>
      </div>

      <div class="pf-pipe-graph-wrap">
        <div bind:this={wrapEl} class="pf-pipe-graph-scaler" style="height: {GRAPH_H * scale}px;">
          <div
            class="pf-pipe-graph"
            style="width: {GRAPH_W}px; height: {GRAPH_H}px; transform: scale({scale}); transform-origin: top left;"
          >
            <svg class="pf-pipe-graph-svg" viewBox="0 0 {GRAPH_W} {GRAPH_H}" width={GRAPH_W} height={GRAPH_H}>
              <defs>
                <marker id="pf-pipe-arr" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
                  <path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
                </marker>
              </defs>
              {#each PIPE_EDGES as e, i (i)}
                {@const d = pathFor(e.from, e.to)}
                {@const midY = edgeMidY(e.from, e.to)}
                {@const midX = edgeMidX(e.from, e.to)}
                {@const visEdge = visited.has(e.from) && visited.has(e.to)}
                <g class={edgeClass(e.from, e.to)}>
                  <path d={d} fill="none" stroke-width={visEdge ? 2 : 1.2} />
                  <path d={d} fill="none" stroke-width="1.2" class="arr-head" marker-end="url(#pf-pipe-arr)" />
                  {#if isLive && visEdge && activeNode === e.to}
                    <circle r="3.5" class="pf-pipe-edge-dot">
                      <animateMotion dur="1.8s" repeatCount="indefinite" path={d} />
                    </circle>
                  {/if}
                  {#if e.label}
                    <g transform="translate({midX}, {midY})">
                      <rect
                        x="-46"
                        y="-9"
                        width="92"
                        height="18"
                        rx="9"
                        fill="var(--background)"
                        class="pf-pipe-edge-pill"
                      ></rect>
                      <text x="0" y="4" text-anchor="middle" font-size="10.5" font-family="var(--font-mono)" class="pf-pipe-edge-text">
                        {e.label}
                      </text>
                    </g>
                  {/if}
                </g>
              {/each}
            </svg>

            {#each PIPE_NODES as n (n.id)}
              {@const p = nodeXY(n.id)}
              {@const st = nodeState(n)}
              <div
                class="pf-pipe-node"
                style="left: {p.x}px; top: {p.y}px; width: {NODE_W}px; height: {NODE_H}px;"
                data-type={n.type}
                data-state={st}
              >
                <div class="pf-pipe-node-head">
                  {#if n.type === "agent"}
                    <Puffer size={20} state={activeNode === n.id && isLive ? "running" : "idle"} />
                  {:else}
                    <span class="pf-pipe-node-ico">
                      <Icon name={nodeIcon(n)} size={12} />
                    </span>
                  {/if}
                  <div class="pf-pipe-node-meta">
                    <span class="name">{n.name ?? n.title}</span>
                    <span class="sub">{n.type === "agent" ? n.model : n.type}</span>
                  </div>
                  {#if activeNode === n.id && isLive}
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
          <span>Trajectory</span>
          <span class="pf-pipe-traj-count">{run.steps.length} steps</span>
          <span style="flex: 1;"></span>
          <button
            type="button"
            class="sc-btn"
            data-variant="ghost"
            data-size="sm"
            disabled={effectiveStep <= 0}
            onclick={() => stepToIdx(Math.max(0, effectiveStep - 1))}
            aria-label="Previous step"
          >
            <Icon name="chevL" size={12} />
          </button>
          <button
            type="button"
            class="sc-btn"
            data-variant="ghost"
            data-size="sm"
            disabled={effectiveStep >= run.steps.length - 1}
            onclick={() => stepToIdx(Math.min(run.steps.length - 1, effectiveStep + 1))}
            aria-label="Next step"
          >
            <Icon name="chevR" size={12} />
          </button>
          <button
            type="button"
            class="sc-btn"
            data-variant={isLive ? "default" : "outline"}
            data-size="sm"
            onclick={() => (stepIdx = null)}
            title="Jump to live"
          >
            <span class="pf-pipe-live-dot"></span>Live
          </button>
        </div>

        <div class="pf-pipe-traj-scrubber">
          <div class="track">
            {#each run.steps as s, i (i)}
              <button
                type="button"
                class="tick {s.status ?? 'done'} {i <= effectiveStep ? 'on' : ''} {i === effectiveStep ? 'cur' : ''}"
                onclick={() => stepToIdx(i)}
                aria-label="Step {i + 1}"
              ></button>
            {/each}
          </div>
        </div>

        <div class="pf-pipe-traj-list">
          {#each run.steps as s, i (i)}
            {@const node = findNode(s.node)}
            <button
              type="button"
              class="pf-pipe-traj-row"
              data-step={i}
              data-current={i === effectiveStep}
              data-past={i < effectiveStep}
              data-status={s.status}
              onclick={() => stepToIdx(i)}
            >
              <span class="t">{s.at}</span>
              <span class="rail">
                <span class="dot" data-kind={s.kind} data-status={s.status}></span>
              </span>
              <span>
                {#if node?.type === "agent"}
                  <span class="lane-chip agent">{s.agent}</span>
                {:else}
                  <span class="lane-chip {node?.type ?? s.kind}">{node?.name ?? node?.title ?? s.kind}</span>
                {/if}
              </span>
              <span class="body">
                <span class="body-title">{s.title}</span>
                {#if s.arg}
                  <span class="body-arg">{s.arg}</span>
                {/if}
              </span>
              <span class="status {s.status ?? 'done'}">
                {#if s.status === "running"}
                  <span class="dot-live"></span>running
                {:else if s.status === "error"}
                  failed
                {:else}
                  ✓
                {/if}
              </span>
            </button>
          {/each}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
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
  .pf-pipe-traj-scrubber .tick {
    all: unset;
    flex: 1;
    border-right: 1px solid var(--background);
    cursor: pointer;
    transition: background 120ms;
  }
</style>
