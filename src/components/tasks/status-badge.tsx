import { cn } from "@/lib/utils";
import { STATUS_LABEL } from "@/lib/format";
import type { TaskStatus } from "@/types/tasks";

const TONE: Record<TaskStatus, string> = {
  running: "bg-sky-500/15 text-sky-700 dark:text-sky-300 ring-sky-500/30",
  waiting: "bg-amber-500/15 text-amber-700 dark:text-amber-300 ring-amber-500/30",
  done: "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300 ring-emerald-500/20",
  failed: "bg-red-500/15 text-red-700 dark:text-red-300 ring-red-500/30",
  stale: "bg-zinc-500/15 text-zinc-600 dark:text-zinc-300 ring-zinc-500/30",
};

const DOT: Record<TaskStatus, string> = {
  running: "bg-sky-500 animate-pulse",
  waiting: "bg-amber-500",
  done: "bg-emerald-500",
  failed: "bg-red-500",
  stale: "bg-zinc-400",
};

export function StatusDot({ status, className }: { status: TaskStatus; className?: string }) {
  return (
    <span
      aria-hidden
      className={cn("inline-block size-2 shrink-0 rounded-full", DOT[status], className)}
    />
  );
}

export function StatusBadge({ status, className }: { status: TaskStatus; className?: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md px-1.5 py-0.5 text-xs font-medium ring-1 ring-inset",
        TONE[status],
        className,
      )}
    >
      <StatusDot status={status} />
      {STATUS_LABEL[status]}
    </span>
  );
}
