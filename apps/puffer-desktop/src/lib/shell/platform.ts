// Lightweight platform detection — no @tauri-apps/api dependency required
// because we only need these booleans at mount time.

export type Platform = "web" | "tauri-mac" | "tauri-windows" | "tauri-linux";

export function detectPlatform(): Platform {
  if (typeof window === "undefined") return "web";
  // Tauri 2 injects __TAURI_INTERNALS__; older fallback: __TAURI__
  const isTauri =
    Boolean((window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__) ||
    Boolean((window as unknown as { __TAURI__?: unknown }).__TAURI__);
  if (!isTauri) return "web";
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes("mac")) return "tauri-mac";
  if (ua.includes("win")) return "tauri-windows";
  return "tauri-linux";
}

export function isTauri(): boolean {
  const p = detectPlatform();
  return p !== "web";
}

export function isDesktopMac(): boolean {
  return detectPlatform() === "tauri-mac";
}
