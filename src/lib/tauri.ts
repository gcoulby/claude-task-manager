import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppError, AppInfo } from "@/types/app";
import type { ArchiveChanged, IngestState } from "@/types/ingest";
import type {
  ArchiveInfo,
  ProjectSummary,
  SearchHit,
  Settings,
  TaskDetail,
  TaskFilter,
  TaskRow,
} from "@/types/tasks";

function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    "message" in value &&
    typeof value.message === "string"
  );
}

/** Rejects with an Error whose message the UI can show as-is. */
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (err) {
    const message = isAppError(err) ? err.message : String(err);
    throw new Error(message, { cause: err });
  }
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  ingestState: () => call<IngestState>("ingest_state"),
  listTasks: (filter: TaskFilter) => call<TaskRow[]>("list_tasks", { filter }),
  getTask: (taskId: string) => call<TaskDetail>("get_task", { taskId }),
  getEntryText: (entryId: number) => call<string>("get_entry_text", { entryId }),
  searchEntries: (query: string) => call<SearchHit[]>("search_entries", { query }),
  listProjects: () => call<ProjectSummary[]>("list_projects"),
  archiveInfo: () => call<ArchiveInfo>("archive_info"),
  getSettings: () => call<Settings>("get_settings"),
  setProjectsRoot: (path: string | null) => call<null>("set_projects_root", { path }),
  setStaleMinutes: (minutes: number) => call<null>("set_stale_minutes", { minutes }),
  setUpdateCheckOnStartup: (enabled: boolean) =>
    call<null>("set_update_check_on_startup", { enabled }),
};

/** Event names must match the constants in Rust. */
export const events = {
  onIngestState: (handler: (state: IngestState) => void): Promise<UnlistenFn> =>
    listen<IngestState>("ingest://state", (e) => handler(e.payload)),
  onArchiveChanged: (handler: (change: ArchiveChanged) => void): Promise<UnlistenFn> =>
    listen<ArchiveChanged>("archive://changed", (e) => handler(e.payload)),
};

export function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
