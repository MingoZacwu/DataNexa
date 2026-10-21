import { isTauri } from "../lib/tauri";

// Versioned so a materially different tip can be shown again later.
const BACKGROUND_RUN_TIP_KEY = "datanexa.tip.background-run.v1";

// Browser preview has no persistent install state, so both helpers are no-ops
// there; the in-memory session guard in App keeps the tip from repeating while
// iterating on the UI.
export function hasSeenBackgroundRunTip(): boolean {
  if (!isTauri) return false;
  try {
    return window.localStorage.getItem(BACKGROUND_RUN_TIP_KEY) !== null;
  } catch {
    return false;
  }
}

export function markBackgroundRunTip(state: "shown" | "enabled"): void {
  if (!isTauri) return;
  try {
    window.localStorage.setItem(BACKGROUND_RUN_TIP_KEY, state);
  } catch {
    // Storage can be unavailable; the in-memory session guard still prevents
    // the tip from repeating while the window stays open.
  }
}
