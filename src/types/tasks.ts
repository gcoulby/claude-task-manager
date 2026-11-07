/** Mirrors `ingest::status::Status`. */
export type TaskStatus = "running" | "waiting" | "done" | "failed" | "stale";

export const TASK_STATUSES: readonly TaskStatus[] = [
  "running",
  "waiting",
  "stale",
  "failed",
  "done",
] as const;

/** Mirrors `commands::tasks::Project`. */
export interface Project {
  key: string;
  label: string;
  isRealPath: boolean;
}

/** Mirrors `commands::tasks::TaskRow`. */
export interface TaskRow {
  id: string;
  sessionId: string;
  parentTaskId: string | null;
  agentId: string | null;
  title: string;
  trigger: "prompt" | "notification" | "command" | "subagent";
  status: TaskStatus;
  statusReason: string;
  startedAt: string | null;
  endedAt: string | null;
  lastActivityAt: string | null;
  entryCount: number;
  depth: 0 | 1;
  childCount: number;
  project: Project;
  sessionTitle: string | null;
  sourceMissing: boolean;
}

/** Mirrors `commands::tasks::TaskFilter`. */
export interface TaskFilter {
  statuses: TaskStatus[];
  project: string | null;
  query: string | null;
}

export type EntryKind =
  | "prompt"
  | "notification"
  | "command"
  | "interrupt"
  | "meta"
  | "text"
  | "thinking"
  | "tool_use"
  | "tool_result"
  | "api_error"
  | "compact"
  | "system";

/** Mirrors `commands::tasks::EntryRow`. */
export interface EntryRow {
  id: number;
  kind: EntryKind;
  toolName: string | null;
  preview: string;
  expandable: boolean;
  isError: boolean;
  ts: string | null;
}

/** Mirrors `commands::tasks::TaskDetail`. */
export interface TaskDetail {
  task: TaskRow;
  parent: TaskRow | null;
  children: TaskRow[];
  entries: EntryRow[];
  sourcePath: string | null;
}

/** Mirrors `commands::sessions::ProjectSummary`. */
export interface ProjectSummary {
  project: Project;
  sessionCount: number;
  lastActivityAt: string | null;
}

/** Mirrors `commands::settings::Settings`. */
export interface Settings {
  projectsRoot: string | null;
  defaultProjectsRoot: string | null;
  staleMinutes: number;
}
