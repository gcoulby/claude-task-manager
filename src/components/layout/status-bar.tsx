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
      const { files, errors, malformedLines } = state.summary;
      const problems = errors.length + malformedLines;
      return problems > 0
        ? { text: `${files} files archived, ${problems} could not be read`, tone: "error" }
        : { text: `${files} files archived`, tone: "muted" };
    }
    case "failed":
      return { text: state.message, tone: "error" };
  }
}

export function StatusBar() {
  const { text, tone } = describe(useIngestState());
  return (
    <footer
      className={
        "border-t px-3 py-1 text-xs " +
        (tone === "error" ? "text-destructive" : "text-muted-foreground")
      }
      role="status"
    >
      {text}
    </footer>
  );
}
