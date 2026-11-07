import { useArchiveInfo } from "@/features/queries";
import { useIngestState } from "@/features/use-ingest-state";
import type { IngestState } from "@/types/ingest";

function describe(state: IngestState | null): { text: string; tone: "muted" | "error" } {
  if (state === null || state.phase === "idle") return { text: "Starting…", tone: "muted" };
  switch (state.phase) {
    case "scanning": {
      const { filesDone, filesTotal } = state.progress;
      return { text: `Reading transcripts: ${filesDone} of ${filesTotal} files`, tone: "muted" };
    }
    case "ready": {
      if (!state.rootExists) {
        return {
          text: `No Claude Code session folder at ${state.root}. Waiting for it to appear.`,
          tone: "muted",
        };
      }
      const { files, errors, malformedLines } = state.summary;
      const problems = errors.length + malformedLines;
      return problems > 0
        ? {
            text: `${files} files archived, ${problems} could not be read. Watching for changes.`,
            tone: "error",
          }
        : { text: `${files} files archived. Watching for changes.`, tone: "muted" };
    }
    case "failed":
      return { text: state.message, tone: "error" };
  }
}

export function StatusBar() {
  const { text, tone } = describe(useIngestState());
  const archive = useArchiveInfo();
  const archivedOnly = archive.data?.counts.sessionsArchivedOnly ?? 0;
  return (
    <footer className="flex items-center gap-3 border-t px-3 py-1 text-xs" role="status">
      <span className={tone === "error" ? "text-destructive" : "text-muted-foreground"}>
        {text}
      </span>
      {archivedOnly > 0 && (
        <span
          className="ml-auto text-muted-foreground"
          title="Claude Code has deleted these transcripts. This app's archive is the only copy."
        >
          {archivedOnly} {archivedOnly === 1 ? "session" : "sessions"} kept only here
        </span>
      )}
    </footer>
  );
}
