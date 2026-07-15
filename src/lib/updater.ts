import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { useCallback, useEffect, useRef, useState } from "react";

const UPDATE_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;
const UPDATE_LAST_CHECK_STORAGE_KEY = "datanexa.updater.last-check";

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

function readLastCheckAt() {
  if (typeof window === "undefined") return null;
  const value = Number(window.localStorage.getItem(UPDATE_LAST_CHECK_STORAGE_KEY));
  return Number.isFinite(value) && value > 0 ? value : null;
}

export function useAppUpdater(enabled: boolean | null, autoCheck: boolean) {
  const [state, setState] = useState<UpdateState>({ kind: "idle" });
  const [lastCheckAt, setLastCheckAt] = useState<number | null>(readLastCheckAt);
  const updateRef = useRef<Update | null>(null);
  const busyRef = useRef(false);

  const recordCheckAttempt = useCallback(() => {
    const checkedAt = Date.now();
    window.localStorage.setItem(UPDATE_LAST_CHECK_STORAGE_KEY, String(checkedAt));
    setLastCheckAt(checkedAt);
  }, []);

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
      recordCheckAttempt();
      busyRef.current = false;
    }
  }, [enabled, recordCheckAttempt]);

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

  useEffect(() => {
    if (!enabled || !autoCheck) return;

    const elapsed = lastCheckAt ? Date.now() - lastCheckAt : UPDATE_CHECK_INTERVAL_MS;
    const delay = Math.max(0, UPDATE_CHECK_INTERVAL_MS - elapsed);
    const timer = window.setTimeout(() => {
      void checkForUpdates();
    }, delay);

    return () => window.clearTimeout(timer);
  }, [autoCheck, checkForUpdates, enabled, lastCheckAt]);

  return {
    state,
    checkForUpdates,
    installUpdate
  };
}
