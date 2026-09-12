import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { api } from "./lib/tauri";
import "./styles.css";
import "./future-glass.css";

const themeMode = (() => {
  const stored = window.localStorage.getItem("datanexa.theme");
  return stored === "system" || stored === "light" || stored === "dark" ? stored : "system";
})();
const systemPrefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
document.documentElement.dataset.platform = /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent)
  ? "macos"
  : /Windows/i.test(navigator.userAgent)
    ? "windows"
    : "other";
document.documentElement.dataset.systemMaterial = "fallback";
document.documentElement.dataset.theme = themeMode;
document.documentElement.classList.toggle("dark", themeMode === "dark" || (themeMode === "system" && systemPrefersDark));

// Forward global script errors and unhandled promise rejections to the Rust
// debug log so runtime failures are captured even when no toast is shown.
window.addEventListener("error", (event) => {
  void api.logFrontendEvent("error", `${event.message} (${event.filename}:${event.lineno}:${event.colno})`);
});
window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason instanceof Error
    ? `${event.reason.name}: ${event.reason.message}`
    : String(event.reason);
  void api.logFrontendEvent("unhandledrejection", reason);
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
