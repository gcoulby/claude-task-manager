import { useState } from "react";
import { ChevronDownIcon, ChevronRightIcon } from "lucide-react";
import { useEntryText } from "@/features/queries";
import { formatFull, formatWhen } from "@/lib/format";
import { errorMessage } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import type { EntryRow } from "@/types/tasks";

function label(e: EntryRow): string {
  switch (e.kind) {
    case "prompt":
      return "You";
    case "notification":
      return "Background notification";
    case "command":
      return "Command";
    case "interrupt":
      return "Stopped by you";
    case "text":
      return "Claude";
    case "thinking":
      return "Thinking";
    case "tool_use":
      return `Tool call · ${e.toolName ?? "tool"}`;
    case "tool_result":
      return `${e.isError ? "Error from" : "Result from"} ${e.toolName ?? "tool"}`;
    case "api_error":
      return "API error";
    case "compact":
      return "Compacted";
    case "meta":
      return "Context";
    case "system":
      return "System";
  }
}

const MONO_KINDS = new Set<EntryRow["kind"]>(["tool_use", "tool_result"]);
const QUIET_KINDS = new Set<EntryRow["kind"]>(["meta", "system", "compact", "thinking"]);

export function TimelineEntry({ entry }: { entry: EntryRow }) {
  const [expanded, setExpanded] = useState(false);
  const full = useEntryText(entry.id, expanded);
  const mono = MONO_KINDS.has(entry.kind);
  const quiet = QUIET_KINDS.has(entry.kind);
  const error = entry.isError || entry.kind === "api_error";
  const text = expanded && full.data !== undefined ? full.data : entry.preview;

  return (
    <div
      className={cn(
        "border-l-2 py-2 pr-2 pl-3",
        entry.kind === "prompt" && "border-l-sky-500 bg-sky-500/5",
        entry.kind === "text" && "border-l-foreground/30",
        mono && "border-l-border",
        quiet && "border-l-transparent",
        error && "border-l-red-500 bg-red-500/5",
      )}
    >
      <div className="mb-1 flex items-center gap-2 text-xs">
        <span
          className={cn(
            "font-medium",
            quiet ? "text-muted-foreground" : "text-foreground",
            error && "text-red-600 dark:text-red-400",
          )}
        >
          {label(entry)}
        </span>
        <span className="text-muted-foreground" title={formatFull(entry.ts)}>
          {formatWhen(entry.ts)}
        </span>
        {entry.expandable && (
          <button
            type="button"
            onClick={() => setExpanded((v) => !v)}
            className="ml-auto inline-flex items-center gap-0.5 text-muted-foreground hover:text-foreground"
            aria-expanded={expanded}
          >
            {expanded ? (
              <ChevronDownIcon className="size-3.5" />
            ) : (
              <ChevronRightIcon className="size-3.5" />
            )}
            {expanded ? "Show less" : "Show all"}
          </button>
        )}
      </div>
      {text && (
        <pre
          className={cn(
            "text-sm break-words whitespace-pre-wrap",
            mono ? "font-mono text-xs" : "font-sans",
            quiet && "text-muted-foreground",
            !expanded && "line-clamp-6",
            expanded && "max-h-[32rem] overflow-y-auto",
          )}
        >
          {text}
        </pre>
      )}
      {expanded && full.isLoading && <p className="text-xs text-muted-foreground">Loading…</p>}
      {expanded && full.error && (
        <p className="text-xs text-destructive">Could not load: {errorMessage(full.error)}</p>
      )}
    </div>
  );
}
