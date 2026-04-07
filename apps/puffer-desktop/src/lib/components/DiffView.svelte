<script lang="ts">
  import type { DiffSnapshot } from "../types";

  export let diff: DiffSnapshot;
  export let compact = false;
  const compactLineLimit = 18;

  function classify(line: string) {
    if (line.startsWith("+")) {
      return "added";
    }
    if (line.startsWith("-")) {
      return "removed";
    }
    if (line.startsWith("@@")) {
      return "meta";
    }
    return "normal";
  }

  function diffStats(text: string) {
    const lines = text.split("\n");
    let additions = 0;
    let removals = 0;
    let hunks = 0;

    for (const line of lines) {
      if (line.startsWith("@@")) {
        hunks += 1;
      } else if (line.startsWith("+") && !line.startsWith("+++")) {
        additions += 1;
      } else if (line.startsWith("-") && !line.startsWith("---")) {
        removals += 1;
      }
    }

    return { additions, removals, hunks };
  }

  $: stats = diffStats(diff.patchExcerpt);
  $: patchLines = diff.patchExcerpt.split("\n");
  $: displayedLines = compact ? patchLines.slice(0, compactLineLimit) : patchLines;
  $: isTruncated = compact && patchLines.length > compactLineLimit;
</script>

<article class:compact class="diff-card">
  <header>
    <div>
      <p class="eyebrow">Latest snapshot</p>
      <h3>{diff.title}</h3>
    </div>
    <div class="stats">
      <span>{diff.command}</span>
      <span>{diff.status}</span>
    </div>
  </header>

  <div class="meta-grid">
    <div>
      <span>Unstaged</span>
      <strong>{diff.unstagedDiffstat}</strong>
    </div>
    <div>
      <span>Staged</span>
      <strong>{diff.stagedDiffstat}</strong>
    </div>
  </div>

  <div class="delta-row">
    <span class="delta added">+{stats.additions} added</span>
    <span class="delta removed">-{stats.removals} removed</span>
    <span class="delta neutral">{stats.hunks} hunks</span>
  </div>

  <pre class="patch">{#each displayedLines as line}<span class={classify(line)}>{line || " "}</span>
{/each}</pre>
  {#if isTruncated}
    <p class="truncation-note">Showing first {compactLineLimit} of {patchLines.length} diff lines.</p>
  {/if}
</article>

<style>
  .diff-card {
    display: grid;
    gap: 1rem;
    padding: 1rem;
    border: 1px solid var(--border);
    border-radius: 22px;
    background: rgba(255, 252, 246, 0.84);
    box-shadow: var(--shadow-soft);
  }

  .diff-card.compact {
    gap: 0.75rem;
    padding: 0.85rem;
  }

  header {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
  }

  h3 {
    margin: 0.2rem 0 0;
    font-size: 1rem;
    line-height: 1.3;
  }

  .eyebrow {
    margin: 0;
    font-size: 0.72rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .stats {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.3rem;
    color: var(--text-muted);
    font-size: 0.82rem;
    text-align: right;
  }

  .meta-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.75rem;
  }

  .meta-grid div {
    display: grid;
    gap: 0.2rem;
    padding: 0.75rem;
    border-radius: 16px;
    background: rgba(228, 221, 208, 0.42);
  }

  .meta-grid span {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }

  .meta-grid strong {
    font-weight: 500;
    line-height: 1.45;
  }

  .delta-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .delta {
    padding: 0.34rem 0.56rem;
    border-radius: 999px;
    font-size: 0.76rem;
    border: 1px solid rgba(111, 101, 89, 0.14);
    background: rgba(255, 255, 255, 0.72);
    color: var(--text-muted);
  }

  .delta.added {
    color: var(--accent);
    border-color: rgba(20, 99, 86, 0.14);
    background: rgba(222, 238, 232, 0.56);
  }

  .delta.removed {
    color: var(--danger);
    border-color: rgba(157, 58, 43, 0.14);
    background: rgba(247, 225, 220, 0.56);
  }

  .patch {
    margin: 0;
    padding: 1rem;
    border-radius: 16px;
    background: #f7f3eb;
    border: 1px solid rgba(109, 95, 79, 0.14);
    font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
    font-size: 0.82rem;
    line-height: 1.6;
    white-space: pre-wrap;
    overflow: auto;
  }

  .patch span {
    display: block;
  }

  .patch .added {
    color: var(--accent);
  }

  .patch .removed {
    color: var(--danger);
  }

  .patch .meta {
    color: #8a5b2a;
  }

  .truncation-note {
    margin: -0.35rem 0 0;
    color: var(--text-muted);
    font-size: 0.78rem;
  }
</style>
