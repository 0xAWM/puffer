<script lang="ts">
  import Icon, { type IconName } from "../../design/Icon.svelte";
  import type { ToolTimelineItem } from "../../types";

  type Props = { item: ToolTimelineItem };
  let { item }: Props = $props();

  function iconFor(name: string): IconName {
    const t = name.toLowerCase();
    if (t.includes("edit") || t.includes("write")) return "edit";
    if (t.includes("read") || t.includes("view")) return "file";
    if (t.includes("grep") || t.includes("search")) return "search";
    if (t.includes("bash") || t.includes("shell") || t.includes("exec")) return "terminal";
    if (t.includes("fetch") || t.includes("web")) return "globe";
    if (t.includes("git") || t.includes("diff")) return "git";
    if (t.includes("list") || t.includes("ls")) return "folder";
    return "bolt";
  }

  function argLine(input: string): string {
    const first = input.split("\n").find((l) => l.trim().length > 0) ?? "";
    const trimmed = first.trim();
    if (trimmed.startsWith("{")) {
      try {
        const obj = JSON.parse(input) as Record<string, unknown>;
        const single =
          obj.path ?? obj.file_path ?? obj.pattern ?? obj.command ??
          obj.query ?? obj.url ?? obj.cwd ?? obj.regex ?? obj.prompt ?? null;
        if (typeof single === "string" && single) return single;
        const arr =
          (obj.paths as unknown) ?? (obj.files as unknown) ??
          (obj.globs as unknown) ?? (obj.urls as unknown) ?? null;
        if (Array.isArray(arr) && arr.every((x) => typeof x === "string")) {
          if (arr.length === 0) return "—";
          if (arr.length === 1) return arr[0] as string;
          return `${arr[0]}  +${arr.length - 1}`;
        }
      } catch {
        /* fall through */
      }
    }
    return trimmed;
  }

  function statusLabel(s: string): string {
    const n = s.toLowerCase();
    if (n.includes("run") || n === "pending") return "running";
    if (n.includes("err") || n.includes("fail")) return "failed";
    return "done";
  }

  // Preview / expand thresholds — tuned so small tool outputs render fully
  // while bash dumps + grep floods collapse to a glance with a chevron.
  const PREVIEW_LINES = 4;
  const AUTO_COLLAPSE_LINE_THRESHOLD = 8;

  let allLines = $derived(item.output.split("\n"));
  let nonEmptyLines = $derived(allLines.filter((l) => l.length > 0));
  let totalLines = $derived(nonEmptyLines.length);
  let hasOutput = $derived(totalLines > 0);
  let isPending = $derived(
    item.status.toLowerCase().startsWith("run") || item.status === "pending"
  );
  let isLarge = $derived(totalLines > AUTO_COLLAPSE_LINE_THRESHOLD);

  // Per-card collapse state — seeded from content size; user can override.
  // Pending cards render expanded so the placeholder stays visible; they
  // re-seed to the size-based default as soon as output arrives.
  let collapsed = $state(false);
  $effect(() => {
    collapsed = isPending ? false : isLarge;
  });

  let visibleLines = $derived(
    collapsed ? nonEmptyLines.slice(0, PREVIEW_LINES) : nonEmptyLines
  );
  let hiddenCount = $derived(Math.max(0, totalLines - visibleLines.length));
  let toggleable = $derived(hasOutput);

  let arg = $derived(argLine(item.input));
  let status = $derived(statusLabel(item.status));
</script>

<div class="pf-tool" data-collapsed={collapsed} data-pending={isPending}>
  <button
    type="button"
    class="pf-tool-head"
    onclick={() => (toggleable ? (collapsed = !collapsed) : undefined)}
    aria-expanded={toggleable ? !collapsed : undefined}
    aria-label={toggleable ? (collapsed ? "Expand tool output" : "Collapse tool output") : undefined}
    disabled={!toggleable}
  >
    <span class="pf-tool-icon"><Icon name={iconFor(item.toolName)} size={13} /></span>
    <span class="pf-tool-name">{item.toolName}</span>
    <span class="pf-tool-arg" title={arg}>{arg}</span>
    <span class="pf-tool-status" data-state={status}>
      <span class="dot"></span>{status}
    </span>
    {#if toggleable}
      <span class="pf-tool-chevron" aria-hidden="true">
        <Icon name={collapsed ? "chevR" : "chevD"} size={11} />
      </span>
    {/if}
  </button>
  {#if hasOutput}
    <div class="pf-tool-body">
      <div class="terminal">
        {#each visibleLines as line, i (i)}
          <div class:dim={line.trim().startsWith("//") || line.trim().startsWith("#")}>{line}</div>
        {/each}
        {#if collapsed && hiddenCount > 0}
          <button type="button" class="pf-tool-more" onclick={() => (collapsed = false)}>
            Show {hiddenCount} more {hiddenCount === 1 ? "line" : "lines"}
          </button>
        {/if}
      </div>
    </div>
  {:else if isPending}
    <div class="pf-tool-body pf-tool-pending-body">
      <div class="pf-tool-pending">
        <div class="pf-tool-pending-bar"></div>
        <div class="pf-tool-pending-text">awaiting result…</div>
      </div>
    </div>
  {/if}
</div>

<style>
  .pf-tool-head {
    width: 100%;
    text-align: left;
    background: color-mix(in oklab, var(--muted) 50%, var(--background));
    border: 0;
    font: inherit;
    cursor: pointer;
  }
  .pf-tool-head:disabled { cursor: default; }
  .pf-tool-chevron {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    color: var(--muted-foreground);
    flex-shrink: 0;
    margin-left: 4px;
    transition: transform 120ms;
  }
  .pf-tool-head:hover .pf-tool-chevron {
    color: var(--foreground);
  }
  .pf-tool-more {
    all: unset;
    display: inline-flex;
    margin-top: 4px;
    padding: 2px 8px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: oklch(0.7 0.1 145);
    background: transparent;
    cursor: pointer;
    border-radius: 4px;
  }
  .pf-tool-more:hover {
    background: color-mix(in oklab, oklch(0.7 0.1 145) 12%, transparent);
  }
  .pf-tool-pending-body {
    background: oklch(0.16 0 0);
    padding: 0;
  }
  .pf-tool-pending {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 14px;
    font-family: var(--font-mono);
  }
  .pf-tool-pending-bar {
    height: 10px;
    border-radius: 3px;
    background: linear-gradient(
      90deg,
      oklch(0.3 0 0) 0%,
      oklch(0.45 0 0) 50%,
      oklch(0.3 0 0) 100%
    );
    background-size: 200% 100%;
    animation: pf-shimmer 1.4s linear infinite;
    width: 62%;
  }
  .pf-tool-pending-text {
    color: oklch(0.7 0 0);
    font-size: 11.5px;
    font-style: italic;
    opacity: 0.85;
  }
  @keyframes pf-shimmer {
    0% { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }
</style>
