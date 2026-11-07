import { SearchIcon, XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { StatusDot } from "@/components/tasks/status-badge";
import { useProjects } from "@/features/queries";
import { STATUS_LABEL } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useUi } from "@/store/ui";
import { TASK_STATUSES } from "@/types/tasks";

export const SEARCH_INPUT_ID = "task-search";

/** Radix Select cannot use an empty value, so "all" needs a sentinel. */
const ALL_PROJECTS = "__all__";

export function TaskFilters() {
  const filter = useUi((s) => s.filter);
  const toggleStatus = useUi((s) => s.toggleStatus);
  const setProject = useUi((s) => s.setProject);
  const setQuery = useUi((s) => s.setQuery);
  const clearFilters = useUi((s) => s.clearFilters);
  const searchMode = useUi((s) => s.searchMode);
  const setSearchMode = useUi((s) => s.setSearchMode);
  const projects = useProjects();

  const hasFilters = filter.statuses.length > 0 || filter.project !== null || filter.query;

  return (
    <div className="flex flex-col gap-2 border-b p-2">
      <div className="flex items-center gap-1">
        <div className="relative min-w-0 flex-1">
          <SearchIcon className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            id={SEARCH_INPUT_ID}
            value={filter.query ?? ""}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") e.currentTarget.blur();
            }}
            placeholder={searchMode === "logs" ? "Search all logs  /" : "Filter by title  /"}
            aria-label={searchMode === "logs" ? "Search all logs" : "Filter tasks by title"}
            className="h-8 pl-7"
          />
        </div>
        <div
          className="flex shrink-0 rounded-md border p-0.5"
          role="radiogroup"
          aria-label="Search in"
        >
          {(["titles", "logs"] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              role="radio"
              aria-checked={searchMode === mode}
              onClick={() => setSearchMode(mode)}
              className={cn(
                "rounded px-2 py-0.5 text-xs",
                searchMode === mode ? "bg-muted text-foreground" : "text-muted-foreground",
              )}
            >
              {mode === "titles" ? "Titles" : "Logs"}
            </button>
          ))}
        </div>
      </div>
      {searchMode === "titles" && (
        <>
          <div className="flex flex-wrap items-center gap-1" role="group" aria-label="Status">
            {TASK_STATUSES.map((status) => {
              const on = filter.statuses.includes(status);
              return (
                <button
                  key={status}
                  type="button"
                  aria-pressed={on}
                  onClick={() => toggleStatus(status)}
                  className={cn(
                    "inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-xs transition-colors",
                    on
                      ? "border-foreground/30 bg-muted text-foreground"
                      : "border-transparent text-muted-foreground hover:bg-muted/60",
                  )}
                >
                  <StatusDot status={status} />
                  {STATUS_LABEL[status]}
                </button>
              );
            })}
          </div>
          <div className="flex items-center gap-1">
            <Select
              value={filter.project ?? ALL_PROJECTS}
              onValueChange={(v) => setProject(v === ALL_PROJECTS ? null : v)}
            >
              <SelectTrigger size="sm" className="h-8 min-w-0 flex-1" aria-label="Project">
                <SelectValue placeholder="All projects" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={ALL_PROJECTS}>All projects</SelectItem>
                {projects.data?.map((p) => (
                  <SelectItem key={p.project.key} value={p.project.key} title={p.project.key}>
                    {p.project.label}
                    <span className="text-muted-foreground">
                      {" "}
                      · {p.sessionCount} {p.sessionCount === 1 ? "session" : "sessions"}
                    </span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {hasFilters && (
              <Button variant="ghost" size="sm" onClick={clearFilters} aria-label="Clear filters">
                <XIcon />
                Clear
              </Button>
            )}
          </div>
        </>
      )}
    </div>
  );
}
