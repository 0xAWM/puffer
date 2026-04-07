<script lang="ts">
  import type { DesktopPreferences, InspectorTab, SettingsSnapshot } from "../types";

  export let snapshot: SettingsSnapshot | null = null;
  export let loading = false;
  export let preferences: DesktopPreferences;
  export let onPreferenceChange: <K extends keyof DesktopPreferences>(
    key: K,
    value: DesktopPreferences[K]
  ) => void = () => {};
  export let onResetPreferences: () => void = () => {};
  export let onRefresh: () => void = () => {};
  export let onLogout: (providerId: string) => void = () => {};

  const dateFormatter = new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit"
  });

  function authSummary(expiresAtMs: number | null): string {
    if (!expiresAtMs) {
      return "No expiry recorded";
    }
    return `Expires ${dateFormatter.format(expiresAtMs)}`;
  }

  function updateTab(value: string) {
    if (value === "latest-diff" || value === "history" || value === "tool-details") {
      onPreferenceChange("defaultInspectorTab", value satisfies InspectorTab);
    }
  }
</script>

<section class="settings-page">
  <div class="settings-header">
    <div>
      <p class="eyebrow">Settings</p>
      <h2>Desktop and runtime configuration</h2>
      <p class="subcopy">Local desktop preferences live alongside a read-only runtime snapshot from the Rust host.</p>
    </div>
    <button class="refresh" on:click={onRefresh}>Refresh snapshot</button>
  </div>

  <div class="settings-grid">
    <article class="section">
      <div class="section-title">
        <p class="eyebrow">Desktop</p>
        <h3>Preferences</h3>
      </div>

      <label class="toggle-row">
        <div>
          <strong>Remember selected session</strong>
          <span>Restore the last viewed session when the app reopens.</span>
        </div>
        <input
          type="checkbox"
          checked={preferences.rememberSession}
          on:change={(event) =>
            onPreferenceChange("rememberSession", (event.currentTarget as HTMLInputElement).checked)}
        />
      </label>

      <label class="toggle-row">
        <div>
          <strong>Remember inspector layout</strong>
          <span>Persist inspector width, tab, and open state.</span>
        </div>
        <input
          type="checkbox"
          checked={preferences.rememberInspectorLayout}
          on:change={(event) =>
            onPreferenceChange(
              "rememberInspectorLayout",
              (event.currentTarget as HTMLInputElement).checked
            )}
        />
      </label>

      <label class="toggle-row">
        <div>
          <strong>Open inspector by default</strong>
          <span>Used when layout persistence is disabled.</span>
        </div>
        <input
          type="checkbox"
          checked={preferences.launchInspectorOpen}
          on:change={(event) =>
            onPreferenceChange("launchInspectorOpen", (event.currentTarget as HTMLInputElement).checked)}
        />
      </label>

      <label class="field">
        <span>Default inspector tab</span>
        <select
          value={preferences.defaultInspectorTab}
          on:change={(event) => updateTab((event.currentTarget as HTMLSelectElement).value)}
        >
          <option value="latest-diff">Latest Diff</option>
          <option value="history">History</option>
          <option value="tool-details">Tool Details</option>
        </select>
      </label>

      <label class="field">
        <span>Default inspector width</span>
        <div class="range-row">
          <input
            type="range"
            min="32"
            max="68"
            step="1"
            value={preferences.defaultInspectorWidth}
            on:input={(event) =>
              onPreferenceChange(
                "defaultInspectorWidth",
                Number((event.currentTarget as HTMLInputElement).value)
              )}
          />
          <strong>{preferences.defaultInspectorWidth}%</strong>
        </div>
      </label>

      <button class="reset" on:click={onResetPreferences}>Reset desktop preferences</button>
    </article>

    <article class="section">
      <div class="section-title">
        <p class="eyebrow">Runtime</p>
        <h3>Configuration snapshot</h3>
      </div>
      {#if loading}
        <div class="empty-card">Loading runtime settings...</div>
      {:else if snapshot}
        <div class="meta-list">
          <div><span>App</span><strong>{snapshot.config.appName}</strong></div>
          <div><span>Default provider</span><strong>{snapshot.config.defaultProvider ?? "unset"}</strong></div>
          <div><span>Default model</span><strong>{snapshot.config.defaultModel ?? "unset"}</strong></div>
          <div><span>Theme</span><strong>{snapshot.config.theme}</strong></div>
          <div><span>Mascot</span><strong>{snapshot.config.mascotDisplayName} ({snapshot.config.mascotEnabled ? "enabled" : "disabled"})</strong></div>
          <div><span>OpenAI base URL</span><strong>{snapshot.config.openaiBaseUrl ?? "default"}</strong></div>
        </div>
      {:else}
        <div class="empty-card">No runtime settings snapshot is available.</div>
      {/if}
    </article>

    <article class="section">
      <div class="section-title">
        <p class="eyebrow">Inventory</p>
        <h3>Resources and sessions</h3>
      </div>
      {#if snapshot}
        <div class="meta-list compact">
          <div><span>Sessions</span><strong>{snapshot.sessions.totalSessions}</strong></div>
          <div><span>Folder groups</span><strong>{snapshot.sessions.folderGroups}</strong></div>
          <div><span>Providers</span><strong>{snapshot.resources.providers}</strong></div>
          <div><span>Tools</span><strong>{snapshot.resources.tools}</strong></div>
          <div><span>Agents</span><strong>{snapshot.resources.agents}</strong></div>
          <div><span>Prompts</span><strong>{snapshot.resources.prompts}</strong></div>
          <div><span>Skills</span><strong>{snapshot.resources.skills}</strong></div>
          <div><span>Plugins</span><strong>{snapshot.resources.plugins}</strong></div>
          <div><span>MCP servers</span><strong>{snapshot.resources.mcpServers}</strong></div>
          <div><span>IDEs</span><strong>{snapshot.resources.ides}</strong></div>
        </div>
      {:else}
        <div class="empty-card">Resource counts will appear after the snapshot loads.</div>
      {/if}
    </article>

    <article class="section">
      <div class="section-title">
        <p class="eyebrow">Auth</p>
        <h3>Stored credentials</h3>
      </div>
      {#if snapshot?.auth.length}
        <div class="stack">
          {#each snapshot.auth as auth}
            <div class="card-row">
              <div>
                <strong>{auth.providerId}</strong>
                <span>{auth.kind}{auth.email ? ` · ${auth.email}` : ""}</span>
              </div>
              <div class="right-copy">
                <strong>{auth.planType ?? "n/a"}</strong>
                <span>{authSummary(auth.expiresAtMs)}</span>
              </div>
              <button class="logout" on:click={() => onLogout(auth.providerId)}>Logout</button>
            </div>
          {/each}
        </div>
      {:else}
        <div class="empty-card">No stored credentials were found.</div>
      {/if}
    </article>

    <article class="section wide">
      <div class="section-title">
        <p class="eyebrow">Providers</p>
        <h3>Registered provider surface</h3>
      </div>
      {#if snapshot?.providers.length}
        <div class="provider-table">
          {#each snapshot.providers as provider}
            <div class="provider-row">
              <div>
                <strong>{provider.displayName}</strong>
                <span>{provider.id} · {provider.defaultApi}</span>
              </div>
              <div>
                <strong>{provider.modelCount} models</strong>
                <span>{provider.authModes.join(", ")}</span>
              </div>
              <div class="path-cell">
                <strong>{provider.sourceKind}</strong>
                <span>{provider.sourcePath ?? provider.baseUrl}</span>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <div class="empty-card">Provider details will appear after the snapshot loads.</div>
      {/if}
    </article>

    <article class="section wide">
      <div class="section-title">
        <p class="eyebrow">Paths</p>
        <h3>Resolved config and resource paths</h3>
      </div>
      {#if snapshot}
        <div class="path-list">
          <div><span>Workspace root</span><code>{snapshot.workspaceRoot}</code></div>
          <div><span>Workspace config</span><code>{snapshot.workspaceConfigFile}</code></div>
          <div><span>User config</span><code>{snapshot.userConfigFile}</code></div>
          <div><span>Auth store</span><code>{snapshot.authStoreFile}</code></div>
          <div><span>Bundled resources</span><code>{snapshot.builtinResourcesDir}</code></div>
        </div>
      {:else}
        <div class="empty-card">Path information will appear after the snapshot loads.</div>
      {/if}
    </article>
  </div>
</section>

<style>
  .settings-page {
    min-height: 0;
    overflow: auto;
    padding: 1rem;
    display: grid;
    gap: 1rem;
    background: rgba(255, 252, 246, 0.44);
  }

  .settings-header {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: start;
    padding: 1rem 1.05rem;
    border-radius: 22px;
    border: 1px solid rgba(111, 101, 89, 0.14);
    background: rgba(255, 255, 255, 0.74);
    box-shadow: var(--shadow-soft);
  }

  .eyebrow {
    margin: 0 0 0.28rem;
    font-size: 0.72rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  h2,
  h3 {
    margin: 0;
  }

  .subcopy {
    margin: 0.45rem 0 0;
    color: var(--text-muted);
    line-height: 1.5;
    max-width: 48rem;
  }

  .refresh,
  .reset {
    border: 1px solid rgba(111, 101, 89, 0.18);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.78);
    color: var(--text);
    padding: 0.62rem 0.88rem;
    cursor: pointer;
  }

  .settings-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1rem;
  }

  .section {
    display: grid;
    gap: 0.9rem;
    padding: 1rem 1.05rem;
    border-radius: 22px;
    border: 1px solid rgba(111, 101, 89, 0.14);
    background: rgba(255, 255, 255, 0.74);
    box-shadow: var(--shadow-soft);
  }

  .section.wide {
    grid-column: span 2;
  }

  .section-title {
    display: grid;
    gap: 0.18rem;
  }

  .toggle-row {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
  }

  .toggle-row strong,
  .field span:first-child,
  .card-row strong,
  .provider-row strong {
    display: block;
  }

  .toggle-row span,
  .field span,
  .card-row span,
  .provider-row span,
  .path-list span,
  .meta-list span {
    color: var(--text-muted);
    line-height: 1.45;
  }

  .field {
    display: grid;
    gap: 0.45rem;
  }

  select,
  input[type="range"] {
    width: 100%;
  }

  select {
    border: 1px solid rgba(111, 101, 89, 0.18);
    border-radius: 14px;
    background: rgba(255, 255, 255, 0.84);
    color: var(--text);
    padding: 0.76rem 0.9rem;
  }

  .range-row {
    display: flex;
    gap: 0.8rem;
    align-items: center;
  }

  .meta-list {
    display: grid;
    gap: 0.65rem;
  }

  .meta-list.compact {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .meta-list div,
  .path-list div {
    display: grid;
    gap: 0.18rem;
  }

  .stack,
  .provider-table,
  .path-list {
    display: grid;
    gap: 0.7rem;
  }

  .card-row,
  .provider-row {
    display: grid;
    grid-template-columns: minmax(0, 1.1fr) minmax(0, 0.8fr);
    gap: 1rem;
    padding: 0.85rem 0.95rem;
    border-radius: 18px;
    background: rgba(255, 252, 246, 0.84);
    border: 1px solid rgba(111, 101, 89, 0.12);
  }

  .provider-row {
    grid-template-columns: minmax(0, 0.9fr) minmax(0, 0.7fr) minmax(0, 1.2fr);
  }

  .right-copy {
    text-align: right;
  }

  .logout {
    border: 1px solid rgba(157, 58, 43, 0.16);
    border-radius: 999px;
    background: rgba(247, 225, 220, 0.72);
    color: var(--danger);
    padding: 0.55rem 0.8rem;
    cursor: pointer;
    justify-self: end;
  }

  .path-cell,
  code {
    overflow-wrap: anywhere;
  }

  .empty-card {
    padding: 1rem;
    border-radius: 18px;
    background: rgba(255, 252, 246, 0.72);
    border: 1px dashed rgba(111, 101, 89, 0.24);
    color: var(--text-muted);
  }

  code {
    font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
    font-size: 0.82rem;
    padding: 0.08rem 0.28rem;
    border-radius: 8px;
    background: rgba(247, 243, 235, 0.88);
  }

  @media (max-width: 1100px) {
    .settings-grid {
      grid-template-columns: 1fr;
    }

    .section.wide {
      grid-column: auto;
    }

    .provider-row,
    .card-row {
      grid-template-columns: 1fr;
    }

    .right-copy {
      text-align: left;
    }

    .logout {
      justify-self: start;
    }
  }

  @media (max-width: 780px) {
    .settings-header,
    .toggle-row,
    .range-row {
      grid-template-columns: 1fr;
      flex-direction: column;
      align-items: stretch;
    }

    .meta-list.compact {
      grid-template-columns: 1fr;
    }
  }
</style>
