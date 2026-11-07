import { useEffect } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";
import { api } from "@/lib/tauri";

interface UpdateState {
  /** An update found and not yet dismissed; drives the install dialog. */
  available: Update | null;
  setAvailable: (update: Update | null) => void;
}

export const useUpdates = create<UpdateState>((set) => ({
  available: null,
  setAvailable: (available) => set({ available }),
}));

/**
 * Asks GitHub whether a newer signed release exists. This is the only network
 * request the app makes, and it sends nothing about you.
 */
export async function checkForUpdate(): Promise<Update | null> {
  return check();
}

/** Checks once in the background after startup, if allowed. Silent on failure. */
export function useStartupUpdateCheck() {
  useEffect(() => {
    let cancelled = false;
    const run = async () => {
      try {
        const settings = await api.getSettings();
        if (!settings.updateCheckOnStartup) return;
        const update = await checkForUpdate();
        if (!cancelled && update) useUpdates.getState().setAvailable(update);
      } catch {
        // Offline or GitHub unreachable: try again next launch.
      }
    };
    void run();
    return () => {
      cancelled = true;
    };
  }, []);
}
