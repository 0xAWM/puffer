<script lang="ts">
  import Icon from "../../design/Icon.svelte";
  import StatePill from "./StatePill.svelte";
  import { historyFor, type Deployment } from "../../data/mockDeployments";

  type Props = { d: Deployment };
  let { d }: Props = $props();

  let history = $derived(historyFor(d));
</script>

<div class="pf-dep-pane">
  <div class="pf-dep-pane-head">
    <div>
      <h3>Deploy history</h3>
      <p class="sub">{history.length} deploys · keeping last 50</p>
    </div>
    <button type="button" class="sc-btn" data-variant="outline" data-size="sm">
      <Icon name="refresh" size={12} />Trigger deploy
    </button>
  </div>
  <div class="pf-dep-history">
    {#each history as h (h.id)}
      <div class="pf-dep-history-row" data-current={h.current}>
        <span class="pf-dep-history-id mono">{h.id}</span>
        <div class="pf-dep-history-commit">
          <span class="mono">{h.commit}</span>
          <span class="sub">{h.branch} · {h.deployer}</span>
        </div>
        <StatePill state={h.state} />
        <span class="sub mono">{h.dur}</span>
        <span class="sub">{h.time}</span>
        <button type="button" class="pf-dep-ico" aria-label="Logs">
          <Icon name="logs" size={12} />
        </button>
      </div>
    {/each}
  </div>
</div>
