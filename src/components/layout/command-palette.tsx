import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command";
import { StatusDot } from "@/components/tasks/status-badge";
import { useProjects } from "@/features/queries";
import { STATUS_LABEL } from "@/lib/format";
import { useUi, type Theme } from "@/store/ui";
import { TASK_STATUSES, type TaskRow } from "@/types/tasks";

/** Tasks listed in the palette; typing filters further. */
const MAX_TASKS = 50;

export function CommandPalette({ tasks }: { tasks: TaskRow[] }) {
  const open = useUi((s) => s.paletteOpen);
  const setOpen = useUi((s) => s.setPaletteOpen);
  const selectTask = useUi((s) => s.selectTask);
  const setStatuses = useUi((s) => s.setStatuses);
  const setProject = useUi((s) => s.setProject);
  const clearFilters = useUi((s) => s.clearFilters);
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);
  const setTheme = useUi((s) => s.setTheme);
  const projects = useProjects();

  const run = (action: () => void) => {
    action();
    setOpen(false);
  };

  return (
    <CommandDialog
      open={open}
      onOpenChange={setOpen}
      title="Command palette"
      description="Jump to a task or change what is shown"
    >
      <CommandInput placeholder="Type a task title or command…" />
      <CommandList>
        <CommandEmpty>Nothing matches.</CommandEmpty>
        <CommandGroup heading="Tasks">
          {tasks.slice(0, MAX_TASKS).map((t) => (
            <CommandItem
              key={t.id}
              value={`task ${t.title} ${t.project.label} ${t.id}`}
              onSelect={() => run(() => selectTask(t.id))}
            >
              <StatusDot status={t.status} />
              <span className="truncate">{t.title}</span>
              <span className="ml-auto shrink-0 text-xs text-muted-foreground">
                {t.project.label}
              </span>
            </CommandItem>
          ))}
        </CommandGroup>
        <CommandSeparator />
        <CommandGroup heading="Show">
          <CommandItem value="show all tasks clear filters" onSelect={() => run(clearFilters)}>
            All tasks
          </CommandItem>
          <CommandItem
            value="show active running waiting"
            onSelect={() => run(() => setStatuses(["running", "waiting"]))}
          >
            Active tasks (running or waiting)
          </CommandItem>
          {TASK_STATUSES.map((s) => (
            <CommandItem
              key={s}
              value={`show status ${s}`}
              onSelect={() => run(() => setStatuses([s]))}
            >
              <StatusDot status={s} />
              {STATUS_LABEL[s]} only
            </CommandItem>
          ))}
        </CommandGroup>
        {projects.data && projects.data.length > 0 && (
          <CommandGroup heading="Project">
            {projects.data.map((p) => (
              <CommandItem
                key={p.project.key}
                value={`project ${p.project.label} ${p.project.key}`}
                onSelect={() => run(() => setProject(p.project.key))}
              >
                {p.project.label}
                <span className="ml-auto truncate text-xs text-muted-foreground">
                  {p.project.key}
                </span>
              </CommandItem>
            ))}
          </CommandGroup>
        )}
        <CommandSeparator />
        <CommandGroup heading="App">
          <CommandItem
            value="open settings preferences"
            onSelect={() => run(() => setSettingsOpen(true))}
          >
            Settings…
          </CommandItem>
          {(["dark", "light", "system"] as Theme[]).map((t) => (
            <CommandItem key={t} value={`theme ${t}`} onSelect={() => run(() => setTheme(t))}>
              Theme: {t === "system" ? "match system" : t}
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  );
}
