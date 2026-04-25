<script lang="ts">
  import Icon from "../design/Icon.svelte";
  import type { AccentKey, AgentState, DensityKey, FontMixKey, ScreenId, ThemeKey, Tweaks } from "./tweaks.ts";

  type Props = {
    tweaks: Tweaks;
    onChange: <K extends keyof Tweaks>(key: K, value: Tweaks[K]) => void;
  };

  let { tweaks, onChange }: Props = $props();

  let open = $state(false);

  const screens: ScreenId[] = ["workspace", "pipelines", "deployments", "settings"];
  const states: AgentState[] = ["idle", "thinking", "running", "awaiting"];
  const themes: ThemeKey[] = ["light", "dark"];
  const accents: { k: AccentKey; c: string }[] = [
    { k: "violet", c: "oklch(0.55 0.22 295)" },
    { k: "cyan", c: "oklch(0.62 0.14 215)" },
    { k: "amber", c: "oklch(0.72 0.18 70)" },
    { k: "rose", c: "oklch(0.62 0.22 15)" },
    { k: "lime", c: "oklch(0.72 0.18 130)" },
    { k: "mono", c: "oklch(0.205 0 0)" }
  ];
  const fonts: { k: FontMixKey; l: string }[] = [
    { k: "sans-mono", l: "sans + mono" },
    { k: "all-mono", l: "all mono" }
  ];
  const densities: DensityKey[] = ["compact", "comfortable", "airy"];
</script>

<button
  type="button"
  class="pf-tweaks-toggle"
  onclick={() => (open = !open)}
  aria-label="Toggle tweaks panel"
>
  <Icon name="settings" size={15} />
</button>

{#if open}
  <div class="pf-tweaks-panel">
    <h3>
      Tweaks
      <button
        type="button"
        class="sc-btn"
        data-variant="ghost"
        data-size="icon-sm"
        style="width: 22px; height: 22px;"
        onclick={() => (open = false)}
        aria-label="Close tweaks"
      >
        <Icon name="x" size={12} />
      </button>
    </h3>

    <div class="pf-tweak-group">
      <div class="pf-tweak-label">Screen</div>
      <div class="pf-tweak-row">
        {#each screens as s (s)}
          <button
            type="button"
            class="pf-tweak-pill"
            data-active={tweaks.screen === s}
            onclick={() => onChange("screen", s)}
          >{s}</button>
        {/each}
      </div>
    </div>

    <div class="pf-tweak-group">
      <div class="pf-tweak-label">Agent state</div>
      <div class="pf-tweak-row">
        {#each states as s (s)}
          <button
            type="button"
            class="pf-tweak-pill"
            data-active={tweaks.agentState === s}
            onclick={() => onChange("agentState", s)}
          >{s}</button>
        {/each}
      </div>
    </div>

    <div class="pf-tweak-group">
      <div class="pf-tweak-label">Theme</div>
      <div class="pf-tweak-row">
        {#each themes as t (t)}
          <button
            type="button"
            class="pf-tweak-pill"
            data-active={tweaks.theme === t}
            onclick={() => onChange("theme", t)}
          >{t}</button>
        {/each}
      </div>
    </div>

    <div class="pf-tweak-group">
      <div class="pf-tweak-label">Accent</div>
      <div class="pf-tweak-row">
        {#each accents as a (a.k)}
          <button
            type="button"
            class="swatch"
            data-active={tweaks.accent === a.k}
            style="background: {a.c};"
            onclick={() => onChange("accent", a.k)}
            aria-label={a.k}
          ></button>
        {/each}
      </div>
    </div>

    <div class="pf-tweak-group">
      <div class="pf-tweak-label">Font mix</div>
      <div class="pf-tweak-row">
        {#each fonts as f (f.k)}
          <button
            type="button"
            class="pf-tweak-pill"
            data-active={tweaks.fontMix === f.k}
            onclick={() => onChange("fontMix", f.k)}
          >{f.l}</button>
        {/each}
      </div>
    </div>

    <div class="pf-tweak-group">
      <div class="pf-tweak-label">Density</div>
      <div class="pf-tweak-row">
        {#each densities as d (d)}
          <button
            type="button"
            class="pf-tweak-pill"
            data-active={tweaks.density === d}
            onclick={() => onChange("density", d)}
          >{d}</button>
        {/each}
      </div>
    </div>

    <div class="pf-switch-row">
      <span style="font-size: 12px;">Show sidebar</span>
      <input
        type="checkbox"
        class="sc-switch"
        checked={tweaks.showSidebar}
        onchange={(e) => onChange("showSidebar", (e.currentTarget as HTMLInputElement).checked)}
      />
    </div>
  </div>
{/if}
