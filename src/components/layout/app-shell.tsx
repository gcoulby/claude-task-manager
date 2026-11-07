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
import { TaskList } from "@/components/tasks/task-list";
import { useTasks } from "@/features/queries";
import { useLiveUpdates } from "@/features/use-live-updates";
import { useTheme } from "@/features/use-theme";
import { useUi } from "@/store/ui";
import type { TaskRow } from "@/types/tasks";

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
function useKeyboard(tasks: TaskRow[]) {
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
  }, [tasks]);
}

export function AppShell() {
  useTheme();
  useLiveUpdates();
  const filter = useUi((s) => s.filter);
  const setPaletteOpen = useUi((s) => s.setPaletteOpen);
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);
  const tasks = useTasks(filter);
  const rows = tasks.data ?? [];
  useKeyboard(rows);
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
              <TaskList tasks={tasks.data} isLoading={tasks.isLoading} error={tasks.error} />
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
