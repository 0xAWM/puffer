//! Thin WebSocket client for the `puffer daemon` wire protocol.
//!
//! Protocol (one JSON object per WebSocket text frame):
//!   server → client on connect:
//!     { event: "hello", payload: { protocolVersion, workspaceRoot } }
//!   client → server:
//!     { id, method, params }
//!   server → client (correlated by id):
//!     { id, result } | { id, error: { code, message } }
//!   server → client (fire-and-forget streaming):
//!     { event: "<channel>", payload }
//!
//! One DaemonClient manages one connection. Callers:
//!   client.request("list_grouped_sessions", {})  // → Promise<result>
//!   client.on("session:<sid>:event", handler)    // subscribe to an event
//!
//! The first consumer who needs the daemon calls ensureDaemonClient(), which
//! either asks Tauri to start a local daemon or (when remote) uses the
//! supplied URL + token.

import { invoke } from "@tauri-apps/api/core";

export type DaemonHandshake = {
  url: string;
  token: string;
  protocolVersion: string;
  workspaceRoot: string;
};

type Pending = {
  resolve: (value: unknown) => void;
  reject: (reason: unknown) => void;
};

type RpcError = { code: string; message: string };

export type ConnectionState = "idle" | "connecting" | "open" | "reconnecting" | "closed";

export class DaemonClient {
  private ws: WebSocket | null = null;
  private pending = new Map<string, Pending>();
  private listeners = new Map<string, Set<(payload: unknown) => void>>();
  private connectionListeners = new Set<(state: ConnectionState) => void>();
  private nextId = 1;
  private readyPromise: Promise<void> | null = null;
  private _state: ConnectionState = "idle";
  private autoReconnect = true;
  private reconnectAttempt = 0;

  constructor(public readonly handshake: DaemonHandshake) {}

  get state(): ConnectionState {
    return this._state;
  }

  private setState(next: ConnectionState) {
    if (this._state === next) return;
    this._state = next;
    for (const fn of this.connectionListeners) fn(next);
  }

  /** Subscribe to connection-state changes ("connecting" | "open" |
   *  "reconnecting" | "closed"). UI shows a banner when this isn't "open". */
  onConnectionChange(handler: (state: ConnectionState) => void): () => void {
    this.connectionListeners.add(handler);
    // Fire the current state immediately so the caller doesn't race the
    // first real change.
    handler(this._state);
    return () => {
      this.connectionListeners.delete(handler);
    };
  }

  async connect(): Promise<void> {
    if (this.readyPromise) return this.readyPromise;
    this.setState("connecting");
    this.readyPromise = new Promise<void>((resolve, reject) => {
      const url = appendToken(this.handshake.url, this.handshake.token);
      const ws = new WebSocket(url);
      this.ws = ws;
      let opened = false;
      ws.addEventListener("open", () => {
        opened = true;
        this.reconnectAttempt = 0;
        this.setState("open");
      });
      ws.addEventListener("message", (event) => {
        this.dispatch(event.data);
      });
      ws.addEventListener("error", () => {
        if (!opened) reject(new Error(`daemon websocket failed: ${url}`));
      });
      ws.addEventListener("close", (ev) => {
        this.ws = null;
        const err = new Error(`daemon websocket closed (${ev.code})`);
        for (const [, pending] of this.pending) pending.reject(err);
        this.pending.clear();
        if (!opened) {
          reject(err);
          this.setState("closed");
          return;
        }
        // Already opened at least once — surface the disconnect + kick off
        // an auto-reconnect loop. Listeners need to re-subscribe after a
        // successful reconnect; they can observe state transitions.
        if (this.autoReconnect) {
          this.setState("reconnecting");
          this.scheduleReconnect();
        } else {
          this.setState("closed");
        }
      });

      // First expected message is the "hello" event; treat that as ready.
      const helloHandler = (payload: unknown) => {
        this.off("hello", helloHandler);
        resolve();
      };
      this.on("hello", helloHandler);
    });
    return this.readyPromise;
  }

  private scheduleReconnect() {
    this.reconnectAttempt += 1;
    // Exponential backoff capped at 10s: 500 / 1000 / 2000 / 4000 / 8000 /
    // 10000…
    const delay = Math.min(500 * 2 ** (this.reconnectAttempt - 1), 10_000);
    setTimeout(() => {
      if (!this.autoReconnect) return;
      this.readyPromise = null;
      void this.connect().catch(() => {
        // connect() sets state to closed on first-open failure; schedule
        // another attempt so the caller doesn't have to.
        if (this.autoReconnect) this.scheduleReconnect();
      });
    }, delay);
  }

  private dispatch(raw: unknown) {
    if (typeof raw !== "string") return;
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      console.warn("daemon: non-JSON frame", raw);
      return;
    }
    if (!parsed || typeof parsed !== "object") return;
    const msg = parsed as { id?: string; result?: unknown; error?: RpcError; event?: string; payload?: unknown };
    if (msg.id !== undefined) {
      const pending = this.pending.get(msg.id);
      if (pending) {
        this.pending.delete(msg.id);
        if (msg.error) {
          pending.reject(new Error(`${msg.error.code}: ${msg.error.message}`));
        } else {
          pending.resolve(msg.result);
        }
      }
      return;
    }
    if (msg.event !== undefined) {
      const set = this.listeners.get(msg.event);
      if (set) {
        for (const fn of set) fn(msg.payload);
      }
    }
  }

  /** Issues an RPC and resolves with the `result` field. */
  async request<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    await this.connect();
    const ws = this.ws;
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      throw new Error("daemon websocket is not open");
    }
    const id = String(this.nextId++);
    const frame = JSON.stringify({ id, method, params });
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: (value) => resolve(value as T),
        reject
      });
      ws.send(frame);
    });
  }

  /** Subscribe to a server-sent event channel. Returns a disposer. */
  on<T = unknown>(event: string, handler: (payload: T) => void): () => void {
    let set = this.listeners.get(event);
    if (!set) {
      set = new Set();
      this.listeners.set(event, set);
    }
    set.add(handler as (payload: unknown) => void);
    return () => this.off(event, handler);
  }

  off<T = unknown>(event: string, handler: (payload: T) => void): void {
    const set = this.listeners.get(event);
    if (!set) return;
    set.delete(handler as (payload: unknown) => void);
    if (set.size === 0) this.listeners.delete(event);
  }

  close() {
    this.autoReconnect = false;
    this.ws?.close();
    this.ws = null;
    this.setState("closed");
  }
}

function appendToken(url: string, token: string): string {
  return url.includes("?")
    ? `${url}&token=${encodeURIComponent(token)}`
    : `${url}?token=${encodeURIComponent(token)}`;
}

// ---------------------------------------------------------------------------
// Singleton management — local (via Tauri) and remote (paste URL+token).
// ---------------------------------------------------------------------------

let sharedClient: DaemonClient | null = null;
let sharedConnectPromise: Promise<DaemonClient> | null = null;

function canInvokeTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Returns the singleton local daemon client, starting the subprocess if
 *  this is the first caller. Requires the Tauri host. */
export async function ensureLocalDaemonClient(): Promise<DaemonClient> {
  if (sharedClient) return sharedClient;
  if (sharedConnectPromise) return sharedConnectPromise;
  if (!canInvokeTauri()) {
    throw new Error("local daemon requires the Tauri desktop shell");
  }
  sharedConnectPromise = (async () => {
    const handshake = await invoke<DaemonHandshake>("start_local_daemon");
    const client = new DaemonClient(handshake);
    await client.connect();
    sharedClient = client;
    return client;
  })();
  try {
    return await sharedConnectPromise;
  } finally {
    sharedConnectPromise = null;
  }
}

/** Returns (and caches) a client against a remote daemon's URL + token.
 *  Each distinct URL caches its own client so switching remotes works. */
const remoteClients = new Map<string, Promise<DaemonClient>>();
export async function ensureRemoteDaemonClient(
  url: string,
  token: string
): Promise<DaemonClient> {
  const key = `${url}\x00${token}`;
  const existing = remoteClients.get(key);
  if (existing) return existing;
  const promise = (async () => {
    const handshake: DaemonHandshake = {
      url,
      token,
      protocolVersion: "1",
      workspaceRoot: ""
    };
    const client = new DaemonClient(handshake);
    await client.connect();
    return client;
  })();
  remoteClients.set(key, promise);
  return promise;
}

/** Swaps the shared daemon client — used when the user connects to a remote
 *  daemon. Existing connections are closed; pending subscribers need to
 *  re-subscribe after the swap. Returns the new live client. */
export async function switchDaemonClient(handshake: DaemonHandshake): Promise<DaemonClient> {
  // Tear down the old client first so listeners / RPCs on it surface as
  // "closed" errors rather than silently dropping frames.
  if (sharedClient) {
    try {
      sharedClient.close();
    } catch {
      /* ignore */
    }
    sharedClient = null;
  }
  sharedConnectPromise = null;
  const client = new DaemonClient(handshake);
  await client.connect();
  sharedClient = client;
  return client;
}

/** Returns the currently-shared client if one is open, without attempting
 *  to start a new daemon. Useful for UI that wants to know the active
 *  workspace without forcing a spawn. */
export function currentDaemonClient(): DaemonClient | null {
  return sharedClient;
}
