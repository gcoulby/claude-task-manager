import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { keys, queryClient, useArchiveInfo, useSettings } from "@/features/queries";
import { api, errorMessage } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useUi, type Theme } from "@/store/ui";
import type { Settings } from "@/types/tasks";

export function SettingsDialog() {
  const open = useUi((s) => s.settingsOpen);
  const setOpen = useUi((s) => s.setSettingsOpen);
  const settings = useSettings();

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>Stored on this computer only.</DialogDescription>
        </DialogHeader>
        {settings.isLoading && <p className="text-sm text-muted-foreground">Loading…</p>}
        {settings.error && (
          <p className="text-sm text-destructive">{errorMessage(settings.error)}</p>
        )}
        {settings.data && (
          // Keyed so the form resets to stored values each time it opens.
          <SettingsForm key={String(open)} settings={settings.data} />
        )}
      </DialogContent>
    </Dialog>
  );
}

function SettingsForm({ settings }: { settings: Settings }) {
  const [root, setRoot] = useState(settings.projectsRoot ?? "");
  const [stale, setStale] = useState(String(settings.staleMinutes));
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);

  const saveRoot = useMutation({
    mutationFn: (path: string | null) => api.setProjectsRoot(path),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: keys.settings }),
  });
  const saveStale = useMutation({
    mutationFn: (minutes: number) => api.setStaleMinutes(minutes),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: keys.settings });
      void queryClient.invalidateQueries({ queryKey: keys.allTasks });
      void queryClient.invalidateQueries({ queryKey: keys.allTaskDetails });
    },
  });

  const staleNumber = Number.parseInt(stale, 10);

  return (
    <div className="flex flex-col gap-5">
      <section className="flex flex-col gap-2">
        <Label htmlFor="projects-root">Claude Code session folder</Label>
        <Input
          id="projects-root"
          value={root}
          onChange={(e) => setRoot(e.target.value)}
          placeholder={settings.defaultProjectsRoot ?? ""}
          spellCheck={false}
        />
        <p className="text-xs text-muted-foreground">
          Leave empty to use the default
          {settings.defaultProjectsRoot ? ` (${settings.defaultProjectsRoot})` : ""}. Changing it
          rescans. Nothing in this folder is ever modified.
        </p>
        <div className="flex gap-2">
          <Button
            size="sm"
            onClick={() => saveRoot.mutate(root.trim() || null)}
            disabled={saveRoot.isPending || root.trim() === (settings.projectsRoot ?? "")}
          >
            Save folder
          </Button>
          {settings.projectsRoot && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setRoot("");
                saveRoot.mutate(null);
              }}
            >
              Use default
            </Button>
          )}
        </div>
        {saveRoot.error && (
          <p className="text-xs text-destructive">{errorMessage(saveRoot.error)}</p>
        )}
        {saveRoot.isSuccess && <p className="text-xs text-muted-foreground">Saved. Rescanning.</p>}
      </section>

      <section className="flex flex-col gap-2">
        <Label htmlFor="stale-minutes">Mark tasks stale after (minutes without activity)</Label>
        <div className="flex gap-2">
          <Input
            id="stale-minutes"
            type="number"
            min={1}
            max={1440}
            value={stale}
            onChange={(e) => setStale(e.target.value)}
            className="w-28"
          />
          <Button
            size="sm"
            onClick={() => saveStale.mutate(staleNumber)}
            disabled={
              saveStale.isPending ||
              !Number.isFinite(staleNumber) ||
              staleNumber === settings.staleMinutes
            }
          >
            Save
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">
          A running task with no new transcript lines for this long is shown as stale. Long tool
          runs can be quiet, so set this above your slowest builds.
        </p>
        {saveStale.error && (
          <p className="text-xs text-destructive">{errorMessage(saveStale.error)}</p>
        )}
      </section>

      <ArchiveSection />

      <section className="flex flex-col gap-2">
        <Label>Theme</Label>
        <div className="flex gap-1" role="radiogroup" aria-label="Theme">
          {(["dark", "light", "system"] as Theme[]).map((t) => (
            <button
              key={t}
              type="button"
              role="radio"
              aria-checked={theme === t}
              onClick={() => setTheme(t)}
              className={cn(
                "rounded-md border px-3 py-1 text-sm",
                theme === t ? "border-foreground/40 bg-muted" : "text-muted-foreground",
              )}
            >
              {t === "system" ? "Match system" : t === "dark" ? "Dark" : "Light"}
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1_000_000) return `${Math.round(bytes / 1000)} kB`;
  if (bytes < 1_000_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
}

function ArchiveSection() {
  const info = useArchiveInfo();
  return (
    <section className="flex flex-col gap-1.5">
      <Label>Archive</Label>
      {info.error && <p className="text-xs text-destructive">{errorMessage(info.error)}</p>}
      {info.data && (
        <>
          <p className="text-xs text-muted-foreground">
            Claude Code deletes transcripts after {info.data.retentionDays} days
            {info.data.retentionIsDefault ? " (its default)" : " (set in your Claude settings)"}.
            This app keeps its own copy, so history survives that clean-up.
          </p>
          <p className="text-xs">
            {info.data.counts.sessions} sessions, {info.data.counts.tasks} tasks,{" "}
            {info.data.counts.entries} entries, {formatBytes(info.data.dbBytes)}
            {info.data.counts.sessionsArchivedOnly > 0 &&
              `. ${info.data.counts.sessionsArchivedOnly} kept only here.`}
          </p>
          <p className="truncate font-mono text-xs text-muted-foreground" title={info.data.dbPath}>
            {info.data.dbPath}
          </p>
        </>
      )}
    </section>
  );
}
