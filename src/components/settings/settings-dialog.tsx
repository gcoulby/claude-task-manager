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
import {
  keys,
  queryClient,
  useAppInfo,
  useArchiveInfo,
  usePlanUsage,
  useSettings,
} from "@/features/queries";
import { checkForUpdate, useUpdates } from "@/features/updates";
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
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-lg">
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
    <div className="flex min-w-0 flex-col gap-5">
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

      <UsageSection settings={settings} />

      <AboutSection settings={settings} />

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
          <p className="font-mono text-xs break-all text-muted-foreground">{info.data.dbPath}</p>
        </>
      )}
    </section>
  );
}

type CheckResult =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "latest" }
  | { kind: "found"; version: string }
  | { kind: "error"; message: string };

function AboutSection({ settings }: { settings: Settings }) {
  const info = useAppInfo();
  const setAvailable = useUpdates((s) => s.setAvailable);
  const setOpen = useUi((s) => s.setSettingsOpen);
  const [result, setResult] = useState<CheckResult>({ kind: "idle" });
  const toggle = useMutation({
    mutationFn: (enabled: boolean) => api.setUpdateCheckOnStartup(enabled),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: keys.settings }),
  });

  const runCheck = async () => {
    setResult({ kind: "checking" });
    try {
      const update = await checkForUpdate();
      if (!update) {
        setResult({ kind: "latest" });
        return;
      }
      setResult({ kind: "found", version: update.version });
      setOpen(false);
      setAvailable(update);
    } catch (err) {
      setResult({ kind: "error", message: errorMessage(err) });
    }
  };

  return (
    <section className="flex flex-col gap-2">
      <Label>About</Label>
      <p className="text-sm">
        Claude Task Manager {info.data ? `version ${info.data.version}` : ""}
      </p>
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={settings.updateCheckOnStartup}
          disabled={toggle.isPending}
          onChange={(e) => toggle.mutate(e.target.checked)}
          className="size-4 accent-foreground"
        />
        Check for updates when the app starts
      </label>
      <p className="text-xs text-muted-foreground">
        The check asks GitHub whether a newer release exists and sends nothing about you. It is the
        only network request this app makes itself; plan usage is fetched by Claude Code. Updates
        are only installed when you agree.
      </p>
      <div className="flex items-center gap-2">
        <Button
          size="sm"
          variant="outline"
          disabled={result.kind === "checking"}
          onClick={() => void runCheck()}
        >
          Check for updates
        </Button>
        <span className="text-xs text-muted-foreground">
          {result.kind === "checking" && "Checking…"}
          {result.kind === "latest" && "You have the latest version."}
          {result.kind === "found" && `Version ${result.version} is available.`}
        </span>
      </div>
      {toggle.error && <p className="text-xs text-destructive">{errorMessage(toggle.error)}</p>}
      {result.kind === "error" && (
        <p className="text-xs text-destructive">Could not check: {result.message}</p>
      )}
    </section>
  );
}

function UsageSection({ settings }: { settings: Settings }) {
  const plan = usePlanUsage();
  const [enabled, setEnabled] = useState(settings.planUsageEnabled);
  const [minutes, setMinutes] = useState(String(settings.planUsageMinutes));
  const [path, setPath] = useState(settings.claudePath ?? "");
  const save = useMutation({
    mutationFn: () => api.setPlanUsage(enabled, Number.parseInt(minutes, 10), path.trim() || null),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: keys.settings }),
  });
  const minutesNumber = Number.parseInt(minutes, 10);
  const changed =
    enabled !== settings.planUsageEnabled ||
    minutesNumber !== settings.planUsageMinutes ||
    (path.trim() || null) !== settings.claudePath;

  return (
    <section className="flex flex-col gap-2">
      <Label>Plan usage</Label>
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => setEnabled(e.target.checked)}
          className="size-4 accent-foreground"
        />
        Show plan limits from Claude Code&apos;s /usage
      </label>
      <p className="text-xs text-muted-foreground">
        Runs <code>claude -p /usage</code> in the background. It calls no model, costs nothing and
        saves no session. Claude Code fetches the figures from Anthropic.
      </p>
      <div className="flex items-center gap-2 text-sm">
        <span className="text-muted-foreground">Every</span>
        <Input
          type="number"
          min={1}
          max={1440}
          value={minutes}
          onChange={(e) => setMinutes(e.target.value)}
          className="w-20"
          aria-label="Minutes between checks"
        />
        <span className="text-muted-foreground">minutes</span>
      </div>
      <Label htmlFor="claude-path" className="text-xs font-normal text-muted-foreground">
        Claude Code program (leave empty to find it automatically)
      </Label>
      <Input
        id="claude-path"
        value={path}
        onChange={(e) => setPath(e.target.value)}
        placeholder={plan.data?.binary ?? "Not found yet"}
        spellCheck={false}
      />
      <div className="flex items-center gap-2">
        <Button
          size="sm"
          disabled={save.isPending || !changed || !Number.isFinite(minutesNumber)}
          onClick={() => save.mutate()}
        >
          Save
        </Button>
        {plan.data && (
          <span className="text-xs text-muted-foreground">
            {plan.data.error ?? "Working."} Last checked{" "}
            {new Date(plan.data.fetchedAtMs).toLocaleTimeString("en-GB", {
              hour: "2-digit",
              minute: "2-digit",
            })}
            .
          </span>
        )}
      </div>
      {save.error && <p className="text-xs text-destructive">{errorMessage(save.error)}</p>}
    </section>
  );
}
