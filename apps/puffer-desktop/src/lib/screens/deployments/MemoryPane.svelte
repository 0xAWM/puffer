<script lang="ts">
  import Icon, { type IconName } from "../../design/Icon.svelte";
  import { KIND_META, MEMORY, type Deployment } from "../../data/mockDeployments";

  type Props = { d: Deployment };
  let { d }: Props = $props();

  let items = $derived(MEMORY[d.id] ?? MEMORY["d-prod-api"]);
  let filter = $state<string>("all");
  const kinds = ["all", ...Object.keys(KIND_META)];
  let filtered = $derived(filter === "all" ? items : items.filter((m) => m.kind === filter));

  function srcIcon(kind: string): IconName {
    if (kind === "deploy") return "rocket";
    if (kind === "pr") return "git";
    if (kind === "logs") return "logs";
    return "bolt";
  }
</script>

<div class="pf-dep-pane">
  <div class="pf-dep-pane-head">
    <div>
      <h3>Memory</h3>
      <p class="sub">
        {items.length} notes Puffer has learned running <strong>{d.name}</strong> — surfaced automatically on future deploys and debug sessions.
      </p>
    </div>
    <button type="button" class="sc-btn" data-variant="default" data-size="sm">
      <Icon name="plus" size={12} />Add note
    </button>
  </div>

  <div class="pf-dep-mem-filters">
    {#each kinds as k (k)}
      {@const meta = k !== "all" ? KIND_META[k] : null}
      <button type="button" class="pf-dep-mem-filter" data-active={filter === k} onclick={() => (filter = k)}>
        {#if meta}
          <Icon name={meta.icon as IconName} size={11} color={meta.color} />
        {/if}
        {k === "all" ? "All" : meta?.label}
        <span class="pf-dep-mem-filter-count">
          {k === "all" ? items.length : items.filter((m) => m.kind === k).length}
        </span>
      </button>
    {/each}
  </div>

  <div class="pf-dep-mem-list">
    {#each filtered as m (m.id)}
      {@const meta = KIND_META[m.kind]}
      <div class="pf-dep-mem">
        <div class="pf-dep-mem-gutter" style="background: {meta.color};"></div>
        <div class="pf-dep-mem-body">
          <div class="pf-dep-mem-head">
            <span
              class="pf-dep-mem-kind"
              style="color: {meta.color}; border-color: color-mix(in oklab, {meta.color} 35%, var(--border)); background: color-mix(in oklab, {meta.color} 8%, transparent);"
            >
              <Icon name={meta.icon as IconName} size={10} />{meta.label}
            </span>
            <span class="pf-dep-mem-title">{m.title}</span>
            <span class="pf-dep-mem-conf" data-conf={m.confidence}>
              <span class="dot"></span>{m.confidence}
            </span>
          </div>
          <div class="pf-dep-mem-text">{m.body}</div>
          <div class="pf-dep-mem-foot">
            <span class="pf-dep-mem-src">
              <Icon name={srcIcon(m.source.kind)} size={10} />
              {m.source.kind}: <span class="mono">{m.source.ref}</span>
            </span>
            <span class="pf-dep-mem-tags">
              {#each m.tags as t (t)}
                <span class="pf-dep-mem-tag">#{t}</span>
              {/each}
            </span>
            <span style="flex: 1;"></span>
            <span class="pf-dep-mem-meta">saved by <strong>{m.savedBy}</strong> · {m.time}</span>
            <span class="pf-dep-mem-uses" title={`Referenced ${m.uses} times`}>
              <Icon name="refresh" size={10} />×{m.uses}
            </span>
            <button type="button" class="pf-dep-ico" title="More" aria-label="More">
              <Icon name="moreH" size={11} />
            </button>
          </div>
        </div>
      </div>
    {/each}
  </div>
</div>
