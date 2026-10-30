import { relaunch } from "@tauri-apps/plugin-process";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./tauri";

export type UpdateErrorPhase = "check" | "download" | "relaunch";

export type UpdateState =
  | { kind: "disabled" }
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "up-to-date" }
  | { kind: "available"; version: string }
  | { kind: "downloading"; version: string; downloaded: number; total?: number }
  | { kind: "relaunching"; version: string }
  | { kind: "error"; phase: UpdateErrorPhase; version?: string };

type UpdateAvailablePayload = {
  version: string;
  current_version: string;
};

type UpdateDownloadProgress = {
  downloaded: number;
  total?: number | null;
};

const UPDATE_AVAILABLE_EVENT = "updater://available";
const UPDATE_DOWNLOAD_PROGRESS_EVENT = "updater://download-progress";

export function useAppUpdater(enabled: boolean | null, autoCheck: boolean) {
  const [state, setState] = useState<UpdateState>({ kind: "idle" });
  const updateVersionRef = useRef<string | null>(null);
  const busyRef = useRef(false);

  const checkForUpdates = useCallback(async () => {
    if (!enabled) {
      if (enabled === false) setState({ kind: "disabled" });
      return;
    }
    if (busyRef.current) return;

    busyRef.current = true;
    setState({ kind: "checking" });
    try {
      const update = await api.checkAppUpdate();
      updateVersionRef.current = update?.version ?? null;
      setState(update ? { kind: "available", version: update.version } : { kind: "up-to-date" });
    } catch (error) {
      api.logFrontendEvent("error", `app update check failed: ${error}`);
      setState({ kind: "error", phase: "check" });
    } finally {
      busyRef.current = false;
    }
  }, [enabled]);

  const installUpdate = useCallback(async () => {
    const version = updateVersionRef.current;
    if (!enabled || !version || busyRef.current) return;

    busyRef.current = true;
    let downloaded = 0;
    let total: number | undefined;
    setState({ kind: "downloading", version, downloaded });
    api.logFrontendEvent("info", `app update download started (version=${version})`);

    // Register before invoking so the first chunks are not missed.
    let unlisten: UnlistenFn | undefined;
    try {
      unlisten = await listen<UpdateDownloadProgress>(UPDATE_DOWNLOAD_PROGRESS_EVENT, (event) => {
        downloaded = event.payload.downloaded;
        total = event.payload.total ?? undefined;
        setState({ kind: "downloading", version, downloaded, total });
      });
    } catch {
      // Progress is best-effort; the command still reports success or failure.
    }

    try {
      await api.installAppUpdate(version);
    } catch (error) {
      unlisten?.();
      api.logFrontendEvent("error", `app update download failed (version=${version}): ${error}`);
      setState({ kind: "error", phase: "download", version });
      busyRef.current = false;
      return;
    }
    unlisten?.();

    setState({ kind: "relaunching", version });
    api.logFrontendEvent("info", `app update downloaded, relaunching (version=${version})`);
    try {
      await relaunch();
    } catch (error) {
      api.logFrontendEvent("error", `app update relaunch failed (version=${version}): ${error}`);
      setState({ kind: "error", phase: "relaunch", version });
      busyRef.current = false;
    }
  }, [enabled]);

  useEffect(() => {
    if (enabled === null) return;
    if (!enabled) {
      setState({ kind: "disabled" });
      return;
    }

    setState((current) => current.kind === "disabled" ? { kind: "idle" } : current);
  }, [enabled]);

  // Background updater notification: the Rust task emits this event when it
  // detects a newer version. Fetch the update details so installUpdate can use
  // them directly without an extra round-trip on user click.
  useEffect(() => {
    if (enabled !== true) return;
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;

    void (async () => {
      try {
        unlisten = await listen<UpdateAvailablePayload>(UPDATE_AVAILABLE_EVENT, () => {
          void checkForUpdates();
        });
        if (cancelled) {
          unlisten();
          unlisten = undefined;
        }
      } catch {
        // Listening is best-effort; the user can still trigger a manual check.
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [checkForUpdates, enabled]);

  // Foreground fallback: when the main window becomes visible (or on mount),
  // ask the backend whether the 24h interval has elapsed. This is not a
  // background task — it only runs while the window is shown to the user.
  useEffect(() => {
    if (enabled !== true || !autoCheck) return;
    if (typeof document === "undefined") return;

    const performDueCheck = () => {
      void api.checkUpdatesIfDue()
        .then((version) => {
          if (version) {
            void checkForUpdates();
          }
        })
        .catch(() => undefined);
    };

    // Run once on mount to cover the startup case (visibilitychange does not
    // fire when the window is already visible at load time).
    performDueCheck();

    const handleVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        performDueCheck();
      }
    };

    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => document.removeEventListener("visibilitychange", handleVisibilityChange);
  }, [autoCheck, checkForUpdates, enabled]);

  return {
    state,
    checkForUpdates,
    installUpdate
  };
}
