<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import Icon from "../../design/Icon.svelte";
  import {
    fsUnwatch,
    fsWatch,
    isDaemonReachable,
    listDir,
    readFile,
    type DirEntry,
    type FsChangedEvent,
    type ReadFileResult
  } from "../../api/desktop";
  import { ensureLocalDaemonClient } from "../../api/daemonClient";

  type Props = { cwd: string };
  let { cwd }: Props = $props();

  // Directory cache: absolute path → its (already-loaded) entries. Keeps
  // the tree interactions snappy across expand/collapse cycles and lets
  // us distinguish "still loading" (no key) from "empty dir" (key with
  // zero entries).
  let cache = $state<Map<string, DirEntry[]>>(new Map());
  let expanded = $state<Set<string>>(new Set());
  let loading = $state<Set<string>>(new Set());
  let errors = $state<Map<string, string>>(new Map());

  // Active right-pane state. `activeLoading` flips during a readFile RPC
  // so we don't flash the previous file while the new one loads.
  let activePath = $state<string | null>(null);
  let activeSize = $state<number>(0);
  let activeFile = $state<ReadFileResult | null>(null);
  let activeLoading = $state(false);
  let activeError = $state<string | null>(null);

  // Root is derived from cwd — switching sessions resets everything.
  let root = $derived(cwd);

  // Web preview has no daemon, so file listing / reading RPCs can't
  // succeed. Render a preview-mode notice instead of a red error.
  const previewMode = !isDaemonReachable();

  $effect(() => {
    // Reset whenever cwd changes. We mutate state inside `untrack` so
    // Svelte doesn't treat our own writes as reactive dependencies of
    // this effect — otherwise setting `loading` / `cache` from
    // loadDir's synchronous prelude loops back into the effect.
    const next = root;
    if (!next || previewMode) return;
    untrack(() => {
      cache = new Map();
      expanded = new Set([next]);
      loading = new Set();
      errors = new Map();
      activePath = null;
      activeFile = null;
      activeError = null;
      activeSize = 0;
      void loadDir(next);
    });
  });

  // Filesystem-watcher lifecycle. When `cwd` is set, ask the daemon to watch
  // it recursively and subscribe to `workspace:fs:changed`. When cwd changes
  // or the component unmounts, unhook and tear down the watch. The daemon's
  // replay machinery will re-fire historical events on reconnect flagged with
  // `replay: true`; we ignore those because a freshly-mounted pane hasn't
  // cached anything yet, so there's nothing stale to refresh.
  //
  // The watch is owned by this effect so it automatically tears down + re-
  // creates when `root` changes (session switch). onDestroy handles the final
  // unmount.
  let currentWatchId: string | null = null;
  let fsEventUnsubscribe: (() => void) | null = null;
  let destroyed = false;

  async function rebuildWatch(target: string) {
    // Tear down whatever's left from the previous watch root first.
    await teardownWatch();
    if (destroyed || !target) return;

    try {
      const client = await ensureLocalDaemonClient();
      const listener = (payload: FsChangedEvent) => {
        if (!payload) return;
        // Replay events describe mutations that happened before this pane
        // existed — nothing to invalidate. The cache is already fresh from
        // the latest listDir on mount.
        if (payload.replay) return;
        if (payload.watchId !== currentWatchId) return;
        handleFsChanged(payload.paths ?? []);
      };
      fsEventUnsubscribe = client.on<FsChangedEvent>("workspace:fs:changed", listener);

      // Start the actual watch. If the pane was unmounted / rerooted while
      // this await was pending, unwatch immediately to avoid leaking.
      const { watchId } = await fsWatch([target], true);
      if (destroyed || target !== root) {
        await fsUnwatch(watchId).catch(() => {
          /* best-effort */
        });
        return;
      }
      currentWatchId = watchId;
    } catch (_err) {
      // Failing to install the watcher isn't fatal — the pane still works,
      // just without auto-refresh. Don't spam the user with a toast; the
      // cache fallback (expand/collapse) still works.
      console.warn("fsWatch failed; FilesPane will not auto-refresh", _err);
    }
  }

  async function teardownWatch() {
    fsEventUnsubscribe?.();
    fsEventUnsubscribe = null;
    const id = currentWatchId;
    currentWatchId = null;
    if (id) {
      try {
        await fsUnwatch(id);
      } catch {
        /* ignore — the daemon might be gone already */
      }
    }
  }

  $effect(() => {
    const target = root;
    if (!target || previewMode) return;
    void rebuildWatch(target);
  });

  onDestroy(() => {
    destroyed = true;
    void teardownWatch();
  });

  /** Invalidate cached directories that contain any changed path + kick off
   *  a re-fetch for the ones currently expanded, so the tree reflects reality
   *  without collapsing. Also reloads the active right-pane file if it was
   *  in the changed set. */
  function handleFsChanged(changed: string[]) {
    if (!changed || changed.length === 0) return;

    // Collect the set of cached directory keys that need invalidation. For
    // each changed path, walk up its parents: any parent that's currently
    // cached lists this path as one of its entries. We have to be careful
    // to handle both direct parents (for creates/deletes) AND the changed
    // path itself if it IS a directory (for descendant changes — some
    // backends coalesce an intermediate directory mtime bump).
    const toInvalidate = new Set<string>();
    for (const p of changed) {
      if (!p) continue;
      // Normalise trailing slashes.
      const norm = p.endsWith("/") && p.length > 1 ? p.slice(0, -1) : p;
      // Walk up through parents until we leave the root. Each ancestor that
      // we have cached needs to be refreshed — the change might be a new
      // file in that ancestor or (for recursive backends like FSEvents) an
      // mtime bump on that ancestor's own entry list.
      let current = norm;
      while (current && current.length >= root.length) {
        if (cache.has(current)) toInvalidate.add(current);
        const parent = parentPath(current);
        if (parent === current) break;
        current = parent;
      }
    }

    if (toInvalidate.size === 0 && activePath && changed.includes(activePath)) {
      // Active file changed but we don't have its containing dir cached —
      // just reload the file contents.
      void reloadActiveFile();
      return;
    }
    if (toInvalidate.size === 0) return;

    // Refresh each invalidated directory. If the directory is currently
    // expanded, re-fetch and overwrite; otherwise just evict from cache
    // so the next expand picks up fresh data.
    for (const dir of toInvalidate) {
      if (expanded.has(dir)) {
        void refreshDir(dir);
      } else {
        const next = new Map(cache);
        next.delete(dir);
        cache = next;
      }
    }

    if (activePath && changed.includes(activePath)) {
      void reloadActiveFile();
    }
  }

  async function refreshDir(path: string) {
    // Refresh in place: re-listDir and merge into the cache without
    // touching the `loading` set (we don't want a spinner on every
    // passive update — that would flicker during an agent edit burst).
    try {
      const entries = await listDir(path);
      const nextCache = new Map(cache);
      nextCache.set(path, entries);
      cache = nextCache;
      // If the directory used to error out but now loads, clear the error.
      if (errors.has(path)) {
        const nextErrors = new Map(errors);
        nextErrors.delete(path);
        errors = nextErrors;
      }
    } catch (err) {
      // On refresh error (dir removed), evict the cache entry so the tree
      // stops rendering stale children. Don't surface the error loudly —
      // it'll resurface naturally if the user tries to expand again.
      const nextCache = new Map(cache);
      nextCache.delete(path);
      cache = nextCache;
      void err;
    }
  }

  async function reloadActiveFile() {
    const target = activePath;
    if (!target) return;
    try {
      const result = await readFile(target);
      if (activePath === target) {
        activeFile = result;
        activeSize = result.size;
        activeError = null;
      }
    } catch (err) {
      if (activePath === target) {
        activeError = err instanceof Error ? err.message : String(err);
      }
    }
  }

  function parentPath(p: string): string {
    if (!p) return p;
    const idx = p.lastIndexOf("/");
    if (idx <= 0) return "/";
    return p.slice(0, idx);
  }

  async function loadDir(path: string) {
    if (cache.has(path) || loading.has(path)) return;
    const nextLoading = new Set(loading);
    nextLoading.add(path);
    loading = nextLoading;
    try {
      const entries = await listDir(path);
      const nextCache = new Map(cache);
      nextCache.set(path, entries);
      cache = nextCache;
      const nextErrors = new Map(errors);
      nextErrors.delete(path);
      errors = nextErrors;
    } catch (err) {
      const nextErrors = new Map(errors);
      nextErrors.set(path, err instanceof Error ? err.message : String(err));
      errors = nextErrors;
    } finally {
      const next = new Set(loading);
      next.delete(path);
      loading = next;
    }
  }

  function joinPath(parent: string, name: string): string {
    if (parent.endsWith("/")) return `${parent}${name}`;
    return `${parent}/${name}`;
  }

  function toggleDir(path: string) {
    const next = new Set(expanded);
    if (next.has(path)) {
      next.delete(path);
    } else {
      next.add(path);
      if (!cache.has(path)) void loadDir(path);
    }
    expanded = next;
  }

  async function openFile(path: string, size: number) {
    activePath = path;
    activeSize = size;
    activeFile = null;
    activeError = null;
    activeLoading = true;
    try {
      const result = await readFile(path);
      if (activePath === path) {
        activeFile = result;
      }
    } catch (err) {
      if (activePath === path) {
        activeError = err instanceof Error ? err.message : String(err);
      }
    } finally {
      if (activePath === path) activeLoading = false;
    }
  }

  function fmtSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  }

  function splitLines(content: string): string[] {
    // No trailing empty line for files that end with a newline; the
    // editor view already renders the final "\n" as the row terminator.
    const trimmed = content.endsWith("\n") ? content.slice(0, -1) : content;
    return trimmed.split("\n");
  }

  let viewerLines = $derived(
    activeFile && activeFile.encoding === "utf8" ? splitLines(activeFile.content) : []
  );

  type TreeRow = {
    path: string;
    name: string;
    depth: number;
    kind: "file" | "directory" | "symlink";
    size: number;
  };

  // Flatten the tree into a single row list so we can render with one
  // {#each}. Recursion is iterative (stack) to keep Svelte happy — each
  // child only appears when its parent is in `expanded`.
  function buildRows(current: string, depth: number, acc: TreeRow[]) {
    const entries = cache.get(current);
    if (!entries) return;
    for (const e of entries) {
      const childPath = joinPath(current, e.name);
      acc.push({
        path: childPath,
        name: e.name,
        depth,
        kind: e.kind,
        size: e.size
      });
      if (
        (e.kind === "directory" || e.kind === "symlink") &&
        expanded.has(childPath)
      ) {
        buildRows(childPath, depth + 1, acc);
      }
    }
  }

  let rows = $derived.by<TreeRow[]>(() => {
    const acc: TreeRow[] = [];
    // Touch these so Svelte knows to re-derive when they change — the
    // cache / expanded sets are replaced by reference on update, but
    // derived.by only tracks values it reads inside the closure.
    cache;
    expanded;
    buildRows(root, 0, acc);
    return acc;
  });
</script>

<div class="pf-files-pane">
  <aside class="tree">
    <div class="tree-head">
      <Icon name="folder" size={12} />
      <span class="tree-root" title={root}>
        {root ? (root.split("/").pop() || root) : "workspace"}
      </span>
    </div>
    <div class="tree-body">
      {#if previewMode}
        <div class="tree-empty">
          <div class="msg">Files view is live in the desktop app</div>
          <div class="sub">Launch Puffer locally to browse this session's working directory.</div>
        </div>
      {:else if errors.has(root) && !cache.has(root)}
        <div class="tree-empty">
          <div class="msg">Failed to load directory</div>
          <div class="sub mono">{errors.get(root)}</div>
        </div>
      {:else if loading.has(root) && !cache.has(root)}
        <div class="tree-empty sub">Loading...</div>
      {:else if rows.length === 0 && cache.has(root)}
        <div class="tree-empty sub">Empty directory</div>
      {:else}
        {#each rows as row (row.path)}
          <button
            type="button"
            class="row"
            class:active={activePath === row.path}
            style="padding-left: {8 + row.depth * 14}px"
            onclick={() =>
              row.kind === "directory" || (row.kind === "symlink" && !errors.has(row.path))
                ? toggleDir(row.path)
                : openFile(row.path, row.size)}
            title={row.path}
          >
            {#if row.kind === "directory"}
              <span class="chev" class:on={expanded.has(row.path)}>
                <Icon name="chevR" size={10} />
              </span>
              <Icon
                name={expanded.has(row.path) ? "folderOpen" : "folder"}
                size={12}
                color="var(--muted-foreground)"
              />
            {:else if row.kind === "symlink"}
              <span class="chev" class:on={expanded.has(row.path)}>
                <Icon name="chevR" size={10} />
              </span>
              <Icon name="link" size={12} color="var(--muted-foreground)" />
            {:else}
              <span class="chev-spacer"></span>
              <Icon name="file" size={12} color="var(--muted-foreground)" />
            {/if}
            <span class="row-name">{row.name}</span>
          </button>
          {#if (row.kind === "directory" || row.kind === "symlink") && expanded.has(row.path) && errors.has(row.path)}
            <div class="row-error mono" style="padding-left: {8 + (row.depth + 1) * 14}px">
              {errors.get(row.path)}
            </div>
          {:else if (row.kind === "directory" || row.kind === "symlink") && expanded.has(row.path) && loading.has(row.path) && !cache.has(row.path)}
            <div class="row-sub" style="padding-left: {8 + (row.depth + 1) * 14}px">
              Loading...
            </div>
          {:else if (row.kind === "directory" || row.kind === "symlink") && expanded.has(row.path) && cache.has(row.path) && cache.get(row.path)!.length === 0}
            <div class="row-sub" style="padding-left: {8 + (row.depth + 1) * 14}px">
              (empty)
            </div>
          {/if}
        {/each}
      {/if}
    </div>
  </aside>

  <section class="viewer">
    {#if previewMode}
      <div class="viewer-empty">
        <Icon name="file" size={20} color="var(--muted-foreground)" />
        <div class="title">File preview is live in the desktop app</div>
        <div class="sub">Open Puffer locally to preview files from this session.</div>
      </div>
    {:else if !activePath}
      <div class="viewer-empty">
        <Icon name="file" size={20} color="var(--muted-foreground)" />
        <div class="title">No file selected</div>
        <div class="sub">Pick a file in the tree on the left to preview it here.</div>
      </div>
    {:else}
      <header class="viewer-head">
        <Icon name="file" size={12} color="var(--muted-foreground)" />
        <span class="path mono" title={activePath}>{activePath}</span>
        <span class="size">{fmtSize(activeSize)}</span>
        {#if activeFile?.truncated}
          <span class="badge">truncated</span>
        {/if}
      </header>
      <div class="viewer-body">
        {#if activeLoading && !activeFile}
          <div class="viewer-msg sub">Loading...</div>
        {:else if activeError}
          <div class="viewer-msg err mono">{activeError}</div>
        {:else if activeFile && activeFile.encoding === "utf8"}
          <pre class="code"><!--
            --><div class="gutter">{#each viewerLines as _line, i}<span class="gl">{i + 1}</span>{/each}</div><!--
            --><div class="lines">{#each viewerLines as line}<span class="ln">{line || " "}</span>{/each}</div><!--
          --></pre>
        {:else if activeFile && activeFile.encoding === "base64"}
          <div class="viewer-msg">
            Binary file ({fmtSize(activeFile.size)}). Download is not supported yet.
          </div>
        {/if}
      </div>
    {/if}
  </section>
</div>

<style>
  .pf-files-pane {
    flex: 1;
    display: flex;
    min-height: 0;
    overflow: hidden;
  }

  .tree {
    width: 240px;
    flex-shrink: 0;
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    background: color-mix(in oklab, var(--background) 97%, var(--muted));
  }
  .tree-head {
    flex-shrink: 0;
    padding: 8px 10px;
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--muted-foreground);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border-bottom: 1px solid var(--border);
  }
  .tree-root {
    font-family: var(--font-mono);
    text-transform: none;
    letter-spacing: 0;
    color: var(--foreground);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tree-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 0;
    font-size: 12px;
  }
  .tree-empty {
    padding: 20px 12px;
    color: var(--muted-foreground);
    font-size: 12px;
    text-align: center;
  }
  .tree-empty .msg {
    color: var(--foreground);
    font-weight: 500;
    margin-bottom: 4px;
  }
  .tree-empty .sub { font-size: 11px; }
  .tree-empty .mono { font-family: var(--font-mono); }
  .tree-empty.sub {
    text-align: left;
    font-style: italic;
  }

  .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px 3px 8px;
    background: transparent;
    border: 0;
    border-radius: 4px;
    color: var(--foreground);
    cursor: pointer;
    font: inherit;
    text-align: left;
    transition: background 100ms;
  }
  .row:hover { background: var(--accent); }
  .row.active { background: var(--muted); color: var(--foreground); }
  .row .chev {
    width: 12px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--muted-foreground);
    transition: transform 120ms;
  }
  .row .chev.on { transform: rotate(90deg); }
  .row .chev-spacer { width: 12px; display: inline-block; }
  .row-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .row-sub,
  .row-error {
    font-size: 11px;
    color: var(--muted-foreground);
    padding: 2px 8px;
    font-style: italic;
  }
  .row-error {
    color: oklch(0.55 0.2 30);
    font-style: normal;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .viewer {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--background);
  }
  .viewer-head {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    background: color-mix(in oklab, var(--background) 97%, var(--muted));
  }
  .viewer-head .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--foreground);
  }
  .viewer-head .size {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--muted-foreground);
  }
  .viewer-head .badge {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 1px 5px;
    border-radius: 3px;
    background: oklch(0.7 0.16 40);
    color: white;
  }

  .viewer-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .viewer-empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 40px;
    color: var(--muted-foreground);
    text-align: center;
  }
  .viewer-empty .title { font-size: 14px; font-weight: 600; color: var(--foreground); }
  .viewer-empty .sub { font-size: 12.5px; max-width: 360px; line-height: 1.55; }

  .viewer-msg {
    padding: 20px 24px;
    color: var(--muted-foreground);
    font-size: 12.5px;
  }
  .viewer-msg.sub { font-style: italic; }
  .viewer-msg.err {
    color: oklch(0.55 0.2 30);
    font-family: var(--font-mono);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .code {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
    display: flex;
    min-height: 100%;
  }
  .code .gutter {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    padding: 10px 8px 10px 12px;
    color: var(--muted-foreground);
    border-right: 1px solid var(--border);
    background: color-mix(in oklab, var(--background) 97%, var(--muted));
    user-select: none;
    text-align: right;
    min-width: 38px;
  }
  .code .gutter .gl {
    display: block;
    font-variant-numeric: tabular-nums;
  }
  .code .lines {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    padding: 10px 12px;
    color: var(--foreground);
  }
  .code .lines .ln {
    display: block;
    white-space: pre;
  }
</style>
