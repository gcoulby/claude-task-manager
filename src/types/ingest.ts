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
  | { phase: "ready"; summary: ScanSummary }
  | { phase: "failed"; message: string };
