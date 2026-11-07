import { keepPreviousData, QueryClient, useQuery } from "@tanstack/react-query";
import { api } from "@/lib/tauri";
import type { TaskFilter } from "@/types/tasks";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Data changes only through archive events, which invalidate it.
      staleTime: Infinity,
      retry: false,
      refetchOnWindowFocus: false,
    },
  },
});

export const keys = {
  tasks: (filter: TaskFilter) => ["tasks", filter] as const,
  allTasks: ["tasks"] as const,
  task: (id: string) => ["task", id] as const,
  allTaskDetails: ["task"] as const,
  entryText: (id: number) => ["entry-text", id] as const,
  projects: ["projects"] as const,
  settings: ["settings"] as const,
};

/** Staleness depends on the clock, so lists re-check once a minute. */
const CLOCK_REFRESH_MS = 60_000;

export function useTasks(filter: TaskFilter) {
  return useQuery({
    queryKey: keys.tasks(filter),
    queryFn: () => api.listTasks(filter),
    placeholderData: keepPreviousData,
    refetchInterval: CLOCK_REFRESH_MS,
  });
}

export function useTask(id: string | null) {
  return useQuery({
    queryKey: keys.task(id ?? ""),
    queryFn: () => api.getTask(id ?? ""),
    enabled: id !== null,
    refetchInterval: CLOCK_REFRESH_MS,
  });
}

export function useEntryText(id: number, enabled: boolean) {
  return useQuery({
    queryKey: keys.entryText(id),
    queryFn: () => api.getEntryText(id),
    enabled,
  });
}

export function useProjects() {
  return useQuery({ queryKey: keys.projects, queryFn: api.listProjects });
}

export function useSettings() {
  return useQuery({ queryKey: keys.settings, queryFn: api.getSettings });
}
