import { useEffect, useState } from "react";
import { api, events } from "@/lib/tauri";
import type { IngestState } from "@/types/ingest";

/** Current ingest state: fetched once, then kept fresh by events. */
export function useIngestState(): IngestState | null {
  const [state, setState] = useState<IngestState | null>(null);

  useEffect(() => {
    let cancelled = false;
    // Subscribe first so an event that lands during the fetch is not lost
    // behind a stale fetch result.
    let gotEvent = false;
    const unlisten = events.onIngestState((next) => {
      gotEvent = true;
      if (!cancelled) setState(next);
    });
    api
      .ingestState()
      .then((initial) => {
        if (!cancelled && !gotEvent) setState(initial);
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setState({ phase: "failed", message: err instanceof Error ? err.message : String(err) });
        }
      });
    return () => {
      cancelled = true;
      void unlisten.then((fn) => fn());
    };
  }, []);

  return state;
}
