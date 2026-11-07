import { useState } from "react";
import { relaunch } from "@tauri-apps/plugin-process";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useUpdates } from "@/features/updates";
import { errorMessage } from "@/lib/tauri";

type Phase =
  | { kind: "idle" }
  | { kind: "downloading"; received: number; total: number | null }
  | { kind: "installing" }
  | { kind: "error"; message: string };

function percent(received: number, total: number | null): string {
  return total ? `${Math.min(100, Math.round((received / total) * 100))}%` : "";
}

/** Offers a found update. Nothing is installed until you choose to. */
export function UpdateDialog() {
  const update = useUpdates((s) => s.available);
  const setAvailable = useUpdates((s) => s.setAvailable);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const busy = phase.kind === "downloading" || phase.kind === "installing";

  const install = async () => {
    if (!update) return;
    let received = 0;
    let total: number | null = null;
    try {
      setPhase({ kind: "downloading", received, total });
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? null;
        if (event.event === "Progress") {
          received += event.data.chunkLength;
          setPhase({ kind: "downloading", received, total });
        }
        if (event.event === "Finished") setPhase({ kind: "installing" });
      });
      await relaunch();
    } catch (err) {
      setPhase({ kind: "error", message: errorMessage(err) });
    }
  };

  return (
    <Dialog
      open={update !== null}
      onOpenChange={(open) => {
        if (!open && !busy) {
          setAvailable(null);
          setPhase({ kind: "idle" });
        }
      }}
    >
      <DialogContent className="sm:max-w-lg" showCloseButton={!busy}>
        <DialogHeader>
          <DialogTitle>Version {update?.version} is available</DialogTitle>
          <DialogDescription>
            You have {update?.currentVersion}. The app restarts after installing.
          </DialogDescription>
        </DialogHeader>
        {update?.body && (
          <div className="max-h-64 overflow-y-auto rounded-md bg-muted/50 p-3">
            <p className="mb-1 text-xs font-medium text-muted-foreground">Release notes</p>
            <pre className="font-sans text-sm whitespace-pre-wrap">{update.body}</pre>
          </div>
        )}
        {phase.kind === "downloading" && (
          <p className="text-sm text-muted-foreground">
            Downloading… {percent(phase.received, phase.total)}
          </p>
        )}
        {phase.kind === "installing" && (
          <p className="text-sm text-muted-foreground">Installing and restarting…</p>
        )}
        {phase.kind === "error" && (
          <p className="text-sm text-destructive">The update failed: {phase.message}</p>
        )}
        <DialogFooter>
          <Button variant="ghost" disabled={busy} onClick={() => setAvailable(null)}>
            Not now
          </Button>
          <Button disabled={busy} onClick={() => void install()}>
            Install and restart
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
