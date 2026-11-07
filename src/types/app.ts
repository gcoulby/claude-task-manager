/** Mirrors `commands::settings::AppInfo` in Rust. */
export interface AppInfo {
  name: string;
  version: string;
}

/** Mirrors the serialised `AppError` in Rust. */
export interface AppError {
  kind: string;
  message: string;
}
