import { useEffect } from "react";
import { useDefaultLayout } from "react-resizable-panels";
import { CommandIcon, SettingsIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { CommandPalette } from "@/components/layout/command-palette";
import { StatusBar } from "@/components/layout/status-bar";
import { SettingsDialog } from "@/components/settings/settings-dialog";
import { DETAIL_ID, TaskDetail } from "@/components/tasks/task-detail";
import { SEARCH_INPUT_ID, TaskFilters } from "@/components/tasks/task-filters";
import { SearchResults } from "@/components/tasks/search-results";
import { TaskList } from "@/components/tasks/task-list";
import { useSearch, useTasks } from "@/features/queries";
import { useLiveUpdates } from "@/features/use-live-updates";
import { useTheme } from "@/features/use-theme";
import { useUi } from "@/store/ui";
import type { SearchHit, TaskRow } from "@/types/tasks";

/** Pane sizes are a convenience; if storage is unavailable, use defaults. */
const layoutStorage = {
  getItem: (key: string) => {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  },
  setItem: (key: string, value: string) => {
    try {
      localStorage.setItem(key, value);
    } catch {
      // Not remembered this time.
    }
  },
};

function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT"
  );
}

/** Global keys: ⌘/Ctrl+K palette, / filter, arrows move, Enter opens. */
function useKeyboard(tasks: TaskRow[], hits: SearchHit[] | null) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const ui = useUi.getState();
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        ui.setPaletteOpen(!ui.paletteOpen);
        return;
      }
      if (ui.paletteOpen || ui.settingsOpen || isTyping(e.target)) return;
      // Leave keys alone while a dropdown has focus.
      if (e.target instanceof HTMLElement && e.target.closest("[data-slot=select-content]")) return;

      if (e.key === "/") {
        e.preventDefault();
        document.getElementById(SEARCH_INPUT_ID)?.focus();
        return;
      }
      if ((e.key === "ArrowDown" || e.key === "ArrowUp") && hits) {
        if (hits.length === 0) return;
        e.preventDefault();
        const index = hits.findIndex((h) => h.entryId === ui.focusEntryId);
        const next =
          index < 0
            ? 0
            : Math.min(hits.length - 1, Math.max(0, index + (e.key === "ArrowDown" ? 1 : -1)));
        const hit = hits[next];
        if (hit) ui.openSearchHit(hit.taskId, hit.entryId);
        return;
      }
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        if (tasks.length === 0) return;
        e.preventDefault();
        const index = tasks.findIndex((t) => t.id === ui.selectedTaskId);
        const next =
          index < 0
            ? 0
            : Math.min(tasks.length - 1, Math.max(0, index + (e.key === "ArrowDown" ? 1 : -1)));
        const task = tasks[next];
        if (task) ui.selectTask(task.id);
        return;
      }
      if (e.key === "Enter" && ui.selectedTaskId) {
        e.preventDefault();
        document.getElementById(DETAIL_ID)?.focus();
        return;
      }
      if (e.key === "Escape" && ui.selectedTaskId) {
        ui.selectTask(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tasks, hits]);
}

export function AppShell() {
  useTheme();
  useLiveUpdates();
  const filter = useUi((s) => s.filter);
  const searchMode = useUi((s) => s.searchMode);
  const focusEntryId = useUi((s) => s.focusEntryId);
  const setPaletteOpen = useUi((s) => s.setPaletteOpen);
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);
  const searchingLogs = searchMode === "logs";
  // In log search the text box searches logs, so it must not also filter titles.
  const tasks = useTasks(searchingLogs ? { ...filter, query: null } : filter);
  const rows = tasks.data ?? [];
  const search = useSearch(filter.query ?? "", searchingLogs);
  useKeyboard(rows, searchingLogs ? (search.data ?? []) : null);
  const { defaultLayout, onLayoutChanged } = useDefaultLayout({
    id: "main-panes",
    storage: layoutStorage,
  });

  return (
    <div className="flex h-screen flex-col">
      <header className="flex items-center gap-2 border-b px-3 py-1.5">
        <h1 className="text-sm font-semibold">Claude Task Manager</h1>
        <span className="text-xs text-muted-foreground">
          {tasks.data ? `${rows.length} ${rows.length === 1 ? "task" : "tasks"}` : ""}
        </span>
        <div className="ml-auto flex items-center gap-1">
          <Button variant="ghost" size="sm" onClick={() => setPaletteOpen(true)}>
            <CommandIcon />
            <span className="text-xs">K</span>
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => setSettingsOpen(true)}
            aria-label="Settings"
          >
            <SettingsIcon />
          </Button>
        </div>
      </header>
      <ResizablePanelGroup
        orientation="horizontal"
        defaultLayout={defaultLayout}
        onLayoutChanged={onLayoutChanged}
        className="min-h-0 flex-1"
      >
        <ResizablePanel id="list" defaultSize="38" minSize="22" maxSize="60">
          <div className="flex h-full flex-col">
            <TaskFilters />
            <div className="min-h-0 flex-1">
              {searchingLogs ? (
                <SearchResults
                  query={filter.query ?? ""}
                  hits={search.data}
                  isLoading={search.isLoading}
                  error={search.error}
                  activeEntryId={focusEntryId}
                />
              ) : (
                <TaskList tasks={tasks.data} isLoading={tasks.isLoading} error={tasks.error} />
              )}
            </div>
          </div>
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel id="detail" minSize="30">
          <TaskDetail />
        </ResizablePanel>
      </ResizablePanelGroup>
      <StatusBar />
      <CommandPalette tasks={rows} />
      <SettingsDialog />
    </div>
  );
}
