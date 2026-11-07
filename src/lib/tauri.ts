import { invoke } from "@tauri-apps/api/core";
import type { AppError, AppInfo } from "@/types/app";

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
};
