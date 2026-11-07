import { useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { formatWhen } from "@/lib/format";
import { errorMessage } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useUi } from "@/store/ui";
import type { SearchHit } from "@/types/tasks";

const ROW_HEIGHT = 76;

const MARK_START = "\u0001";
const MARK_END = "\u0002";

/** Splits an FTS snippet into plain and matched parts. */
function snippetParts(text: string): { text: string; match: boolean }[] {
  const parts: { text: string; match: boolean }[] = [];
  let rest = text;
  for (;;) {
    const start = rest.indexOf(MARK_START);
    if (start < 0) break;
    const end = rest.indexOf(MARK_END, start);
    if (end < 0) break;
    if (start > 0) parts.push({ text: rest.slice(0, start), match: false });
    parts.push({ text: rest.slice(start + 1, end), match: true });
    rest = rest.slice(end + 1);
  }
  if (rest) parts.push({ text: rest, match: false });
  return parts;
}

function Snippet({ text }: { text: string }) {
  return (
    <>
      {snippetParts(text).map((part, i) =>
        part.match ? (
          <mark key={i} className="rounded-sm bg-amber-300/40 px-0.5 text-inherit">
            {part.text}
          </mark>
        ) : (
          <span key={i}>{part.text}</span>
        ),
      )}
    </>
  );
}

function kindLabel(hit: SearchHit): string {
  switch (hit.kind) {
    case "prompt":
      return "Prompt";
    case "text":
      return "Claude";
    case "thinking":
      return "Thinking";
    case "tool_use":
      return `Call · ${hit.toolName ?? "tool"}`;
    case "tool_result":
      return `Result · ${hit.toolName ?? "tool"}`;
    default:
      return hit.kind.replace("_", " ");
  }
}

interface Props {
  query: string;
  hits: SearchHit[] | undefined;
  isLoading: boolean;
  error: unknown;
  activeEntryId: number | null;
}

export function SearchResults({ query, hits, isLoading, error, activeEntryId }: Props) {
  const openSearchHit = useUi((s) => s.openSearchHit);
  const scrollRef = useRef<HTMLDivElement>(null);
  const rows = hits ?? [];
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  });

  const activeIndex = rows.findIndex((h) => h.entryId === activeEntryId);
  useEffect(() => {
    if (activeIndex >= 0) virtualizer.scrollToIndex(activeIndex, { align: "auto" });
  }, [activeIndex, virtualizer]);

  if (query.trim() === "") {
    return (
      <p className="p-6 text-center text-sm text-muted-foreground">
        Type to search prompts, replies and tool output across every session.
      </p>
    );
  }
  if (error) {
    return <p className="p-4 text-sm text-destructive">Search failed: {errorMessage(error)}</p>;
  }
  if (isLoading) {
    return <p className="p-4 text-sm text-muted-foreground">Searching…</p>;
  }
  if (rows.length === 0) {
    return <p className="p-6 text-center text-sm text-muted-foreground">No matches.</p>;
  }

  return (
    <div
      ref={scrollRef}
      className="h-full overflow-y-auto"
      role="listbox"
      aria-label="Search results"
    >
      <div style={{ height: virtualizer.getTotalSize() }} className="relative w-full">
        {virtualizer.getVirtualItems().map((item) => {
          const hit = rows[item.index];
          if (!hit) return null;
          const active = hit.entryId === activeEntryId;
          return (
            <button
              key={hit.entryId}
              type="button"
              role="option"
              aria-selected={active}
              onClick={() => openSearchHit(hit.taskId, hit.entryId)}
              className={cn(
                "absolute top-0 left-0 flex w-full flex-col gap-0.5 border-b border-border/50 px-3 py-1.5 text-left",
                active ? "bg-muted" : "hover:bg-muted/50",
              )}
              style={{ height: item.size, transform: `translateY(${item.start}px)` }}
            >
              <span className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
                <span className="shrink-0 font-medium text-foreground">{kindLabel(hit)}</span>
                <span className="truncate">{hit.taskTitle}</span>
                <span className="ml-auto shrink-0">{formatWhen(hit.ts)}</span>
              </span>
              <span className="line-clamp-2 text-sm break-words">
                <Snippet text={hit.snippet} />
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
