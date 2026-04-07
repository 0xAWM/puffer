# Puffer Desktop

Parallel desktop shell for Puffer Code, built with Tauri 2, Svelte, and Vite.

## What It Does

- Groups sessions by workspace folder in a left sidebar
- Renders conversation history, tool activity, permission requests, and diff snapshots
- Shows a right-hand inspector for latest diff, history, and selected-item details
- Exposes one-click GitHub pull request and merge actions through local `gh`

## Development

```bash
npm install
npm run check
npm run build
npm run tauri dev
```

## Notes

- The desktop host reads the existing Puffer session store and does not introduce a second session database.
- GitHub actions rely on local `git` and `gh` being available in the selected session repository.
- On Linux, Tauri/WebKit system packages are required. On this machine, the missing pieces were `javascriptcoregtk-4.1` and `libsoup-3.0`, which prevented a native Linux `cargo check` for the app host.
