<script lang="ts">
  import Icon, { type IconName } from "../../design/Icon.svelte";
  import { INTEGRATIONS, type Deployment } from "../../data/mockDeployments";

  type Props = { d: Deployment };
  let { d }: Props = $props();

  let items = $derived(INTEGRATIONS[d.id] ?? INTEGRATIONS["d-prod-api"]);

  const providerIcon: Record<string, IconName> = {
    postgres: "server", redis: "server", stripe: "coin", sentry: "bug",
    github: "git", slack: "plug", s3: "layers", openai: "sparkles"
  };
</script>

<div class="pf-dep-pane">
  <div class="pf-dep-pane-head">
    <div>
      <h3>Providers &amp; integrations</h3>
      <p class="sub">External services this deployment talks to. Connection strings are injected at build time.</p>
    </div>
    <button type="button" class="sc-btn" data-variant="default" data-size="sm">
      <Icon name="plus" size={12} />Add provider
    </button>
  </div>
  <div class="pf-dep-provs">
    {#each items as p (p.name)}
      <div class="pf-dep-prov">
        <div class="pf-dep-prov-ico">
          <Icon name={providerIcon[p.kind] ?? "plug"} size={16} />
        </div>
        <div class="pf-dep-prov-body">
          <div class="pf-dep-prov-name">{p.name}</div>
          <div class="pf-dep-prov-note">{p.note}</div>
        </div>
        <span class="pf-dep-prov-status" data-state={p.status === "connected" ? "healthy" : "degraded"}>
          <span class="dot"></span>{p.status}
        </span>
        <button type="button" class="pf-dep-ico" aria-label="Settings">
          <Icon name="settings" size={12} />
        </button>
      </div>
    {/each}
  </div>
</div>
