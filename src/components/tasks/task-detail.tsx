import { useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { AlertTriangleIcon, ArrowUpLeftIcon, BotIcon } from "lucide-react";
import { Skeleton } from "@/components/ui/skeleton";
import { StatusBadge, StatusDot } from "@/components/tasks/status-badge";
import { TimelineEntry } from "@/components/tasks/timeline-entry";
import { useTask } from "@/features/queries";
import { formatFull, formatTokens, formatWhen, isActive, taskDuration } from "@/lib/format";
import { errorMessage } from "@/lib/tauri";
import { useUi } from "@/store/ui";
import type { EntryRow, TaskDetail as Detail, TaskRow } from "@/types/tasks";

export const DETAIL_ID = "task-detail";

export function TaskDetail() {
  const selectedId = useUi((s) => s.selectedTaskId);
  const query = useTask(selectedId);

  if (selectedId === null) {
    return (
      <Centred>
        <p>Select a task to see its timeline.</p>
        <p className="text-xs">
          Use ↑ and ↓ to move through the list, / to filter, ⌘K for commands.
        </p>
      </Centred>
    );
  }
  if (query.isLoading) {
    return (
      <div className="flex flex-col gap-3 p-4" aria-busy>
        <Skeleton className="h-6 w-2/3" />
        <Skeleton className="h-4 w-1/2" />
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-24 w-full" />
      </div>
    );
  }
  if (query.error || !query.data) {
    return (
      <Centred>
        <p className="text-destructive">
          {query.error ? errorMessage(query.error) : "This task could not be loaded."}
        </p>
      </Centred>
    );
  }
  return <DetailView detail={query.data} />;
}

function Centred({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-1 p-6 text-center text-sm text-muted-foreground">
      {children}
    </div>
  );
}

function DetailView({ detail }: { detail: Detail }) {
  const { task, parent, children, entries, tokens } = detail;
  const selectTask = useUi((s) => s.selectTask);
  const focusEntryId = useUi((s) => s.focusEntryId);
  const active = isActive(task.status);

  return (
    <div id={DETAIL_ID} tabIndex={-1} className="flex h-full flex-col outline-none">
      <header className="flex flex-col gap-2 border-b p-4">
        {parent && (
          <button
            type="button"
            onClick={() => selectTask(parent.id)}
            className="inline-flex w-fit items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
          >
            <ArrowUpLeftIcon className="size-3" />
            Subagent of “{parent.title}”
          </button>
        )}
        <h2 className="text-base leading-snug font-semibold">{task.title}</h2>
        <div className="flex items-start gap-2">
          <StatusBadge status={task.status} className="shrink-0" />
          <p className="text-sm text-muted-foreground">{task.statusReason}</p>
        </div>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-0.5 text-xs">
          <Meta label="Project">
            <span title={task.project.key}>
              {task.project.isRealPath ? task.project.key : `${task.project.label} (folder name)`}
            </span>
          </Meta>
          {task.sessionTitle && <Meta label="Session">{task.sessionTitle}</Meta>}
          <Meta label="Started">
            <span title={formatFull(task.startedAt)}>{formatWhen(task.startedAt)}</span>
          </Meta>
          <Meta label={active ? "Running for" : "Took"}>
            {taskDuration(task.startedAt, task.endedAt, task.lastActivityAt, active) || "unknown"}
          </Meta>
          <Meta label="Last activity">
            <span title={formatFull(task.lastActivityAt)}>{formatWhen(task.lastActivityAt)}</span>
          </Meta>
          <Meta label="Entries">{task.entryCount}</Meta>
          {tokens.messages > 0 && (
            <Meta label="Tokens">
              <span
                title={`Input ${tokens.input.toLocaleString("en-GB")}, cache read ${tokens.cacheRead.toLocaleString("en-GB")}, cache write ${tokens.cacheWrite.toLocaleString("en-GB")}${children.length > 0 ? ". Includes subagents." : ""}`}
              >
                {formatTokens(tokens.output)} out ·{" "}
                {formatTokens(tokens.input + tokens.cacheRead + tokens.cacheWrite)} in (
                {formatTokens(tokens.cacheRead)} from cache)
              </span>
            </Meta>
          )}
        </dl>
        {task.sourceMissing && (
          <p className="flex items-start gap-2 rounded-md bg-amber-500/10 p-2 text-xs text-amber-800 dark:text-amber-200">
            <AlertTriangleIcon className="mt-px size-3.5 shrink-0" />
            Claude Code has deleted the original transcript for this session. What you see here is
            this app's archived copy.
          </p>
        )}
        {children.length > 0 && <Subagents tasks={children} onSelect={selectTask} />}
      </header>
      {/* Keyed so each task starts with fresh scroll state. */}
      <Timeline key={task.id} entries={entries} follow={active} focusEntryId={focusEntryId} />
    </div>
  );
}

function Meta({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="min-w-0 truncate">{children}</dd>
    </>
  );
}

function Subagents({ tasks, onSelect }: { tasks: TaskRow[]; onSelect: (id: string) => void }) {
  return (
    <div className="flex flex-col gap-1">
      <p className="text-xs text-muted-foreground">Subagents</p>
      <ul className="flex flex-col gap-0.5">
        {tasks.map((t) => (
          <li key={t.id}>
            <button
              type="button"
              onClick={() => onSelect(t.id)}
              title={t.statusReason}
              className="flex w-full items-center gap-2 rounded px-1 py-0.5 text-left text-sm hover:bg-muted"
            >
              <BotIcon className="size-3.5 shrink-0 text-muted-foreground" />
              <StatusDot status={t.status} />
              <span className="truncate">{t.title}</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

function Timeline({
  entries,
  follow,
  focusEntryId,
}: {
  entries: EntryRow[];
  follow: boolean;
  focusEntryId: number | null;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const atBottom = useRef(true);
  const virtualizer = useVirtualizer({
    count: entries.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 90,
    overscan: 8,
  });

  const scrolledOnce = useRef(false);
  const lastFocus = useRef<number | null>(null);
  const focusIndex = focusEntryId === null ? -1 : entries.findIndex((e) => e.id === focusEntryId);

  // A search hit scrolls to its entry. Otherwise open at the latest entry,
  // then keep following new ones while the task is live, unless you have
  // scrolled up to read something.
  useEffect(() => {
    if (entries.length === 0) return;
    if (focusIndex >= 0 && focusIndex !== lastFocus.current) {
      virtualizer.scrollToIndex(focusIndex, { align: "center" });
      lastFocus.current = focusIndex;
      scrolledOnce.current = true;
      atBottom.current = false;
      return;
    }
    if (!scrolledOnce.current || (follow && atBottom.current)) {
      virtualizer.scrollToIndex(entries.length - 1, { align: "end" });
      scrolledOnce.current = true;
    }
  }, [entries.length, follow, focusIndex, virtualizer]);

  if (entries.length === 0) {
    return (
      <Centred>
        <p>No entries in this task yet.</p>
      </Centred>
    );
  }

  return (
    <div
      ref={scrollRef}
      className="min-h-0 flex-1 overflow-y-auto"
      onScroll={(e) => {
        const el = e.currentTarget;
        atBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
      }}
    >
      <ol style={{ height: virtualizer.getTotalSize() }} className="relative w-full">
        {virtualizer.getVirtualItems().map((item) => {
          const entry = entries[item.index];
          if (!entry) return null;
          return (
            <li
              key={entry.id}
              data-index={item.index}
              ref={virtualizer.measureElement}
              className="absolute top-0 left-0 w-full px-4 py-1"
              style={{ transform: `translateY(${item.start}px)` }}
            >
              <TimelineEntry entry={entry} highlighted={entry.id === focusEntryId} />
            </li>
          );
        })}
      </ol>
    </div>
  );
}
