import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { detectPlatform } from "./lib/shell/platform";

// Tag <html> with a platform class so CSS can adapt chrome without
// reading the userAgent from every component.
const platform = detectPlatform();
if (platform !== "web") {
  document.documentElement.classList.add("is-tauri");
  document.documentElement.classList.add(`is-${platform}`);
}

const app = mount(App, {
  target: document.getElementById("app")!
});

export default app;
