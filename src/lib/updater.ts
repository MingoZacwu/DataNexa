import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";

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

const UPDATE_AVAILABLE_EVENT = "updater://available";

export function useAppUpdater(enabled: boolean | null, autoCheck: boolean) {
  const [state, setState] = useState<UpdateState>({ kind: "idle" });
  const updateRef = useRef<Update | null>(null);
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
      const update = await check();
      const previousUpdate = updateRef.current;
      updateRef.current = update;
      if (previousUpdate && previousUpdate !== update) {
        await previousUpdate.close().catch(() => undefined);
      }
      setState(update ? { kind: "available", version: update.version } : { kind: "up-to-date" });
    } catch {
      setState({ kind: "error", phase: "check" });
    } finally {
      busyRef.current = false;
    }
  }, [enabled]);

  const installUpdate = useCallback(async () => {
    const update = updateRef.current;
    if (!enabled || !update || busyRef.current) return;

    busyRef.current = true;
    let downloaded = 0;
    let total: number | undefined;
    setState({ kind: "downloading", version: update.version, downloaded });

    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          downloaded = 0;
          total = event.data.contentLength;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
        }
        setState({
          kind: "downloading",
          version: update.version,
          downloaded,
          total
        });
      });
    } catch {
      setState({ kind: "error", phase: "download", version: update.version });
      busyRef.current = false;
      return;
    }

    setState({ kind: "relaunching", version: update.version });
    try {
      await relaunch();
    } catch {
      setState({ kind: "error", phase: "relaunch", version: update.version });
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
  // detects a newer version. Fetch the Update object so installUpdate can use
  // it directly without an extra round-trip on user click.
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
      void invoke<string | null>("check_updates_if_due")
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
