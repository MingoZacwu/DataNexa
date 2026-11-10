import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ConfigErrorPage } from "./features/errors/ConfigErrorPage";
import { detectLocale, messages } from "./i18n";
import { detectThemeMode, resolveTheme, systemTheme } from "./app/theme";
import { api } from "./lib/tauri";
import type { ConfigLoadFailure } from "./types";
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

/** Gate between boot and the workspace. A corrupt config.toml must never mount
 * `App`: its effects would start snapshot polling and surface errors behind the
 * recovery screen. */
function Root() {
  const [configError, setConfigError] = useState<ConfigLoadFailure | null | undefined>(undefined);

  // `App` normally negotiates the native window material and reconciles the
  // effective dark state with it, but it is not mounted while the recovery
  // screen is up. Without this mirror the page can keep light tokens while the
  // native material paints dark, which makes every label unreadable.
  useEffect(() => {
    const mode = detectThemeMode();
    const query = window.matchMedia("(prefers-color-scheme: dark)");

    const applyDark = (dark: boolean) => {
      document.documentElement.classList.toggle("dark", dark);
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    };

    // Follow the OS while the preference is "system" and no native material
    // has reported a theme yet.
    const onSystemChange = () => {
      if (mode === "system") applyDark(query.matches);
    };

    void api
      .setWindowMaterialTheme(mode === "system" ? null : resolveTheme(mode, systemTheme()) === "dark")
      .then(([nativeDark, materialEnabled]) => {
        document.documentElement.dataset.systemMaterial = materialEnabled ? "enabled" : "fallback";
        // The native surface decides the real appearance, so trust it over the
        // webview media query whenever a material is actually in use.
        if (materialEnabled) {
          applyDark(nativeDark);
        } else if (mode === "system") {
          applyDark(query.matches);
        }
      })
      .catch(() => {
        document.documentElement.dataset.systemMaterial = "fallback";
        if (mode === "system") applyDark(query.matches);
      });

    query.addEventListener("change", onSystemChange);
    return () => query.removeEventListener("change", onSystemChange);
  }, []);

  useEffect(() => {
    let cancelled = false;
    api
      .bootStatus()
      .then((status) => {
        if (!cancelled) setConfigError(status.config_error ?? null);
      })
      .catch(() => {
        // A boot status failure is not a configuration failure; let the
        // workspace render and surface its own error instead of blocking.
        if (!cancelled) setConfigError(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    api
      .bootStatus()
      .then((status) => {
        if (!cancelled) setConfigError(status.config_error ?? null);
      })
      .catch(() => {
        // A boot status failure is not a configuration failure; let the
        // workspace render and surface its own error instead of blocking.
        if (!cancelled) setConfigError(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (configError === undefined) {
    return null;
  }
  if (configError) {
    return <ConfigErrorPage t={messages[detectLocale()]} error={configError} />;
  }
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>
);
