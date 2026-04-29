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

### Browser mode

The Svelte app can run outside Tauri when you point it at an already-running
Puffer daemon:

```bash
puffer daemon --bind 127.0.0.1:1421 --token dev-token --print-handshake
npm run dev
open "http://127.0.0.1:1420/?daemonUrl=ws://127.0.0.1:1421/ws&daemonToken=dev-token"
```

The browser build also accepts `VITE_PUFFER_DAEMON_URL` and
`VITE_PUFFER_DAEMON_TOKEN`. A daemon handshake supplied in the URL is cached in
localStorage so reloads keep using the same daemon.

## Notes

- The desktop host reads the existing Puffer session store and does not introduce a second session database.
- GitHub actions rely on local `git` and `gh` being available in the selected session repository.
- On Linux, Tauri/WebKit system packages are required. On this machine, the missing pieces were `javascriptcoregtk-4.1` and `libsoup-3.0`, which prevented a native Linux `cargo check` for the app host.
