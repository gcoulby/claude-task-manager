import type { TaskStatus } from "@/types/tasks";

export const STATUS_LABEL: Record<TaskStatus, string> = {
  running: "Running",
  waiting: "Waiting",
  done: "Done",
  failed: "Failed",
  stale: "Stale",
};

/** "45 s", "12 min", "2 h 5 min", "3 days". */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "";
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} s`;
  const min = Math.floor(s / 60);
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  if (h < 48) return min % 60 ? `${h} h ${min % 60} min` : `${h} h`;
  return `${Math.floor(h / 24)} days`;
}

/** Duration of a task: to its end, or to now while it is still active. */
export function taskDuration(
  startedAt: string | null,
  endedAt: string | null,
  lastActivityAt: string | null,
  active: boolean,
  now: number = Date.now(),
): string {
  if (!startedAt) return "";
  const start = Date.parse(startedAt);
  const endIso = endedAt ?? (active ? null : lastActivityAt);
  const end = endIso ? Date.parse(endIso) : now;
  return formatDuration(end - start);
}

const timeFmt = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit" });
const dateFmt = new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "short" });
const fullFmt = new Intl.DateTimeFormat("en-GB", { dateStyle: "medium", timeStyle: "medium" });

/** "just now", "5 min ago", "14:03", "yesterday 14:03", "3 Nov 14:03". */
export function formatWhen(iso: string | null, now: Date = new Date()): string {
  if (!iso) return "";
  const d = new Date(iso);
  const diff = now.getTime() - d.getTime();
  if (diff < 60_000 && diff > -60_000) return "just now";
  if (diff > 0 && diff < 3_600_000) return `${Math.floor(diff / 60_000)} min ago`;
  const sameDay = d.toDateString() === now.toDateString();
  if (sameDay) return timeFmt.format(d);
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (d.toDateString() === yesterday.toDateString()) return `yesterday ${timeFmt.format(d)}`;
  return `${dateFmt.format(d)} ${timeFmt.format(d)}`;
}

export function formatFull(iso: string | null): string {
  return iso ? fullFmt.format(new Date(iso)) : "";
}

export function isActive(status: TaskStatus): boolean {
  return status === "running" || status === "waiting";
}

/** 950, 12.3k, 4.5M, 1.2B. */
export function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  const units: [number, string][] = [
    [1e9, "B"],
    [1e6, "M"],
    [1e3, "k"],
  ];
  for (const [size, suffix] of units) {
    if (n >= size) {
      const v = n / size;
      return `${v >= 100 ? Math.round(v) : v.toFixed(1).replace(/\.0$/, "")}${suffix}`;
    }
  }
  return String(n);
}
