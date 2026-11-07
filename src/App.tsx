import { useEffect, useState } from "react";
import { api } from "@/lib/tauri";
import type { AppInfo } from "@/types/app";

type State =
  { kind: "loading" } | { kind: "ready"; info: AppInfo } | { kind: "error"; message: string };

export default function App() {
  const [state, setState] = useState<State>({ kind: "loading" });

  useEffect(() => {
    api
      .appInfo()
      .then((info) => setState({ kind: "ready", info }))
      .catch((err: unknown) =>
        setState({ kind: "error", message: err instanceof Error ? err.message : String(err) }),
      );
  }, []);

  return (
    <main className="flex h-screen flex-col items-center justify-center gap-2">
      <h1 className="text-2xl font-semibold">Claude Task Manager</h1>
      {state.kind === "loading" && <p className="text-muted-foreground">Loading…</p>}
      {state.kind === "ready" && (
        <p className="text-muted-foreground">Version {state.info.version}, backend connected.</p>
      )}
      {state.kind === "error" && (
        <p className="text-destructive">Could not reach the backend: {state.message}</p>
      )}
    </main>
  );
}
