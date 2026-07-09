import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

const themeMode = (() => {
  const stored = window.localStorage.getItem("datanexa.theme");
  return stored === "system" || stored === "light" || stored === "dark" ? stored : "system";
})();
const systemPrefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;

document.documentElement.dataset.platform = /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent) ? "macos" : "other";
document.documentElement.dataset.theme = themeMode;
document.documentElement.classList.toggle("dark", themeMode === "dark" || (themeMode === "system" && systemPrefersDark));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
