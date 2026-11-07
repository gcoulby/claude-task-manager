import { useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArchiveIcon, BellIcon, BotIcon, TerminalIcon } from "lucide-react";
import { Skeleton } from "@/components/ui/skeleton";
import { StatusDot } from "@/components/tasks/status-badge";
import { formatWhen, isActive, taskDuration } from "@/lib/format";
import { cn } from "@/lib/utils";
import { errorMessage } from "@/lib/tauri";
import { useUi } from "@/store/ui";
import type { TaskRow } from "@/types/tasks";

const ROW_HEIGHT = 56;

interface Props {
  tasks: TaskRow[] | undefined;
  isLoading: boolean;
  error: unknown;
}

export function TaskList({ tasks, isLoading, error }: Props) {
  const selectedId = useUi((s) => s.selectedTaskId);
  const selectTask = useUi((s) => s.selectTask);
  const filter = useUi((s) => s.filter);
  const scrollRef = useRef<HTMLDivElement>(null);
  const rows = tasks ?? [];

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

  // Keep the keyboard selection in view.
  const selectedIndex = rows.findIndex((r) => r.id === selectedId);
  useEffect(() => {
    if (selectedIndex >= 0) virtualizer.scrollToIndex(selectedIndex, { align: "auto" });
  }, [selectedIndex, virtualizer]);

  if (isLoading) {
    return (
      <div className="flex flex-col gap-2 p-3" aria-busy>
        {Array.from({ length: 8 }, (_, i) => (
          <Skeleton key={i} className="h-11 w-full" />
        ))}
      </div>
    );
  }
  if (error) {
    return (
      <p className="p-4 text-sm text-destructive">Could not load tasks: {errorMessage(error)}</p>
    );
  }
  if (rows.length === 0) {
    const filtered = filter.statuses.length > 0 || filter.project !== null || filter.query;
    return (
      <div className="p-6 text-center text-sm text-muted-foreground">
        {filtered
          ? "No tasks match these filters."
          : "No tasks yet. They appear here as Claude Code sessions are found."}
      </div>
    );
  }

  return (
    <div ref={scrollRef} className="h-full overflow-y-auto" role="listbox" aria-label="Tasks">
      <div style={{ height: virtualizer.getTotalSize() }} className="relative w-full">
        {virtualizer.getVirtualItems().map((item) => {
          const task = rows[item.index];
          if (!task) return null;
          return (
            <div
              key={task.id}
              className="absolute top-0 left-0 w-full"
              style={{ height: item.size, transform: `translateY(${item.start}px)` }}
            >
              <TaskListRow
                task={task}
                selected={task.id === selectedId}
                onSelect={() => selectTask(task.id)}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}

function TriggerIcon({ trigger }: { trigger: TaskRow["trigger"] }) {
  const cls = "size-3 shrink-0 text-muted-foreground";
  if (trigger === "subagent") return <BotIcon className={cls} aria-label="Subagent" />;
  if (trigger === "command") return <TerminalIcon className={cls} aria-label="Command" />;
  if (trigger === "notification")
    return <BellIcon className={cls} aria-label="Background notification" />;
  return null;
}

function TaskListRow({
  task,
  selected,
  onSelect,
}: {
  task: TaskRow;
  selected: boolean;
  onSelect: () => void;
}) {
  const active = isActive(task.status);
  return (
    <button
      type="button"
      role="option"
      aria-selected={selected}
      onClick={onSelect}
      title={task.statusReason}
      className={cn(
        "flex h-full w-full flex-col justify-center gap-0.5 border-b border-border/50 px-3 text-left",
        selected ? "bg-muted" : "hover:bg-muted/50",
        task.depth === 1 && "pl-8",
      )}
    >
      <span className="flex min-w-0 items-center gap-2">
        <StatusDot status={task.status} />
        <TriggerIcon trigger={task.trigger} />
        <span className="truncate text-sm">{task.title}</span>
        {task.sourceMissing && (
          <ArchiveIcon
            className="ml-auto size-3 shrink-0 text-amber-600 dark:text-amber-400"
            aria-label="Original transcript deleted; archived copy only"
          />
        )}
      </span>
      <span className="flex min-w-0 items-center gap-1.5 pl-4 text-xs text-muted-foreground">
        <span className="truncate" title={task.project.key}>
          {task.project.label}
        </span>
        <span aria-hidden>·</span>
        <span className="shrink-0">{formatWhen(task.lastActivityAt)}</span>
        {task.startedAt && (
          <>
            <span aria-hidden>·</span>
            <span className="shrink-0">
              {taskDuration(task.startedAt, task.endedAt, task.lastActivityAt, active)}
            </span>
          </>
        )}
        {task.childCount > 0 && (
          <>
            <span aria-hidden>·</span>
            <span className="shrink-0">
              {task.childCount} {task.childCount === 1 ? "subagent" : "subagents"}
            </span>
          </>
        )}
      </span>
    </button>
  );
}
