import { useEffect } from "react";
import { keys, queryClient } from "@/features/queries";
import { events } from "@/lib/tauri";

/** Refreshes archive-backed queries whenever ingest reports a change. */
export function useLiveUpdates() {
  useEffect(() => {
    const refresh = () => {
      void queryClient.invalidateQueries({ queryKey: keys.allTasks });
      void queryClient.invalidateQueries({ queryKey: keys.allTaskDetails });
      void queryClient.invalidateQueries({ queryKey: keys.projects });
      void queryClient.invalidateQueries({ queryKey: keys.archiveInfo });
      void queryClient.invalidateQueries({ queryKey: keys.allSearches });
      void queryClient.invalidateQueries({ queryKey: keys.tokens });
    };
    const subscriptions = [
      events.onArchiveChanged(refresh),
      events.onIngestState((state) => {
        if (state.phase === "ready") refresh();
      }),
    ];
    return () => {
      for (const sub of subscriptions) void sub.then((unlisten) => unlisten());
    };
  }, []);
}
