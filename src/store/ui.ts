import { create } from "zustand";
import type { TaskFilter, TaskStatus } from "@/types/tasks";

export type Theme = "dark" | "light" | "system";

/** What the search box does: filter task titles, or search inside logs. */
export type SearchMode = "titles" | "logs";

const THEME_KEY = "ctm.theme";

function readTheme(): Theme {
  try {
    const stored = localStorage.getItem(THEME_KEY);
    if (stored === "dark" || stored === "light" || stored === "system") return stored;
  } catch {
    // Storage can be unavailable; the default is fine.
  }
  return "dark";
}

interface UiState {
  filter: TaskFilter;
  selectedTaskId: string | null;
  /** A timeline entry to scroll to and highlight, from a search hit. */
  focusEntryId: number | null;
  searchMode: SearchMode;
  paletteOpen: boolean;
  settingsOpen: boolean;
  theme: Theme;
  toggleStatus: (status: TaskStatus) => void;
  setStatuses: (statuses: TaskStatus[]) => void;
  setProject: (project: string | null) => void;
  setQuery: (query: string) => void;
  clearFilters: () => void;
  selectTask: (id: string | null) => void;
  openSearchHit: (taskId: string, entryId: number) => void;
  setSearchMode: (mode: SearchMode) => void;
  setPaletteOpen: (open: boolean) => void;
  setSettingsOpen: (open: boolean) => void;
  setTheme: (theme: Theme) => void;
}

const emptyFilter: TaskFilter = { statuses: [], project: null, query: null };

export const useUi = create<UiState>((set) => ({
  filter: emptyFilter,
  selectedTaskId: null,
  focusEntryId: null,
  searchMode: "titles",
  paletteOpen: false,
  settingsOpen: false,
  theme: readTheme(),
  toggleStatus: (status) =>
    set((s) => {
      const has = s.filter.statuses.includes(status);
      const statuses = has
        ? s.filter.statuses.filter((x) => x !== status)
        : [...s.filter.statuses, status];
      return { filter: { ...s.filter, statuses } };
    }),
  setStatuses: (statuses) => set((s) => ({ filter: { ...s.filter, statuses } })),
  setProject: (project) => set((s) => ({ filter: { ...s.filter, project } })),
  setQuery: (query) => set((s) => ({ filter: { ...s.filter, query: query || null } })),
  clearFilters: () => set({ filter: emptyFilter }),
  selectTask: (id) => set({ selectedTaskId: id, focusEntryId: null }),
  openSearchHit: (taskId, entryId) => set({ selectedTaskId: taskId, focusEntryId: entryId }),
  setSearchMode: (searchMode) => set({ searchMode }),
  setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setTheme: (theme) => {
    try {
      localStorage.setItem(THEME_KEY, theme);
    } catch {
      // Not remembered this time; still applied.
    }
    set({ theme });
  },
}));
