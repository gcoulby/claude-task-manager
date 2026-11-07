import { RefreshCwIcon } from "lucide-react";
import { usePlanUsage, useTokenUsage } from "@/features/queries";
import { formatTokens } from "@/lib/format";
import { api } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import type { Meter, TokenTotals } from "@/types/tasks";

/** "Current week (all models)" reads better in a narrow header as "Week". */
function shortLabel(label: string): string {
  const l = label.toLowerCase();
  if (l === "current session") return "Session";
  if (l === "current week (all models)") return "Week";
  return label.replace(/^current\s+/i, "").replace(/^\w/, (c) => c.toUpperCase());
}

function MeterView({ meter }: { meter: Meter }) {
  const tone =
    meter.percent >= 90 ? "bg-red-500" : meter.percent >= 70 ? "bg-amber-500" : "bg-sky-500";
  return (
    <div
      className="flex items-center gap-1.5"
      title={`${meter.label}: ${meter.percent}% used${meter.resets ? `, resets ${meter.resets}` : ""}`}
    >
      <span className="text-muted-foreground">{shortLabel(meter.label)}</span>
      <span
        className="relative h-1.5 w-14 overflow-hidden rounded-full bg-muted"
        role="meter"
        aria-valuenow={meter.percent}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={meter.label}
      >
        <span
          className={cn("absolute inset-y-0 left-0", tone)}
          style={{ width: `${meter.percent}%` }}
        />
      </span>
      <span className="tabular-nums">{meter.percent}%</span>
    </div>
  );
}

function tokenTitle(label: string, t: TokenTotals): string {
  return (
    `${label}: ${t.messages} responses\n` +
    `Output ${t.output.toLocaleString("en-GB")}\n` +
    `Input ${t.input.toLocaleString("en-GB")}\n` +
    `Cache read ${t.cacheRead.toLocaleString("en-GB")}\n` +
    `Cache write ${t.cacheWrite.toLocaleString("en-GB")}`
  );
}

function Tokens({ label, totals }: { label: string; totals: TokenTotals | undefined }) {
  if (!totals) return null;
  return (
    <span className="text-muted-foreground" title={tokenTitle(label, totals)}>
      {label} <span className="text-foreground tabular-nums">{formatTokens(totals.output)}</span>{" "}
      out ·{" "}
      <span className="tabular-nums">
        {formatTokens(totals.input + totals.cacheRead + totals.cacheWrite)}
      </span>{" "}
      in
    </span>
  );
}

/** Plan limits from /usage and token totals from the archive. */
export function UsageBar() {
  const plan = usePlanUsage();
  const tokens = useTokenUsage();
  const usage = plan.data;

  return (
    <div className="flex min-w-0 items-center gap-4 text-xs">
      {usage && usage.meters.length > 0 && (
        <div className="flex items-center gap-3" title={usage.raw}>
          {usage.meters.slice(0, 3).map((m) => (
            <MeterView key={m.label} meter={m} />
          ))}
        </div>
      )}
      {usage && usage.meters.length === 0 && usage.error && (
        <span className="truncate text-muted-foreground" title={usage.raw || usage.error}>
          Plan usage unavailable
        </span>
      )}
      {usage && (
        <button
          type="button"
          onClick={() => void api.refreshPlanUsage()}
          className="text-muted-foreground hover:text-foreground"
          aria-label="Refresh plan usage"
          title="Check plan usage now"
        >
          <RefreshCwIcon className="size-3" />
        </button>
      )}
      <Tokens label="Today" totals={tokens.data?.today} />
      <Tokens label="7 days" totals={tokens.data?.week} />
    </div>
  );
}
