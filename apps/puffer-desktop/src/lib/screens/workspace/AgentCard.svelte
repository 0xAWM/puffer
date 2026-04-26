<script lang="ts">
  import Puffer from "../../design/Puffer.svelte";
  import Icon from "../../design/Icon.svelte";
  import { AGENT_STATE_LABELS, agentPufferState, type MockAgent } from "../../data/mockProjects";

  type Props = { a: MockAgent; onOpen?: () => void };
  let { a, onOpen }: Props = $props();
</script>

<button class="pf-pw-agent" data-status={a.status} onclick={onOpen}>
  <div class="head">
    <Puffer size={22} state={agentPufferState(a.status)} />
    <div class="identity">
      <span class="name">{a.name}</span>
      <span class="model">{a.model}</span>
    </div>
    <span class="status-pill" data-status={a.status}>{AGENT_STATE_LABELS[a.status] ?? a.status}</span>
  </div>
  {#if a.title}
    <div class="title">{a.title}</div>
  {/if}
  <div class="branch-row">
    <Icon name="branch" size={10} />
    <span class="branch">{a.branch}</span>
  </div>
  {#if a.status === "running"}
    <div class="progress">
      <div class="bar" style="width: {a.progress}%;"></div>
    </div>
  {/if}
  <div class="step">{a.step}</div>
  <div class="meta">
    <span><Icon name="bolt" size={10} />{a.tools}</span>
    <span><Icon name="clock" size={10} />{a.elapsed}</span>
    <span class="worktree" title="worktree">{a.worktree}</span>
  </div>
</button>
