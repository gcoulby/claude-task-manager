/** Mirrors `ingest::scanner::ScanProgress`. */
export interface ScanProgress {
  filesDone: number;
  filesTotal: number;
  entriesAdded: number;
}

/** Mirrors `ingest::scanner::ScanSummary`. */
export interface ScanSummary {
  files: number;
  filesRead: number;
  entriesAdded: number;
  malformedLines: number;
  sessionsRebuilt: number;
  errors: string[];
}

/** Mirrors `state::IngestState`. */
export type IngestState =
  | { phase: "idle" }
  | { phase: "scanning"; progress: ScanProgress }
  | { phase: "ready"; summary: ScanSummary; root: string; rootExists: boolean }
  | { phase: "failed"; message: string };

/** Mirrors `ingest::runner::ArchiveChanged`. An empty list means anything may have changed. */
export interface ArchiveChanged {
  sessionIds: string[];
}
