import { AlertTriangle, Eye, EyeOff, X } from "lucide-react";
import { useState } from "react";
import { commands, type ResolvedVar, type ResolveReport } from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { showError, unwrap } from "@/lib/api";
import { displayValue, sourceLabel, usageLabel } from "@/lib/vars";
import { cn } from "cn";

interface InspectorPanelProps {
  dir: string;
  report: ResolveReport | null;
  onClose: () => void;
}

/**
 * Every variable the request uses: its value, where it came from, what it shadows and where
 * it is used (PLAN.md §5.4). Secrets are masked until revealed.
 */
export function InspectorPanel({ dir, report, onClose }: InspectorPanelProps) {
  return (
    <aside className="flex h-full flex-col border-l bg-muted/20">
      <div className="flex h-9 items-center justify-between border-b px-3">
        <span className="text-sm font-medium">Variables</span>
        <Button
          variant="ghost"
          size="icon"
          className="size-6"
          onClick={onClose}
          aria-label="Close inspector"
        >
          <X />
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-2 text-xs">
        {report?.errors.map((error) => (
          <p
            key={error}
            className="mb-2 flex gap-1.5 rounded-md bg-destructive/10 p-2 text-destructive"
          >
            <AlertTriangle className="size-3.5 shrink-0" /> {error}
          </p>
        ))}
        {report && report.vars.length === 0 && (
          <p className="p-2 text-muted-foreground">
            This request uses no variables. Write <code>{"{{name}}"}</code> anywhere to use one.
          </p>
        )}
        {report?.vars.map((v) => (
          <VariableCard key={v.name} dir={dir} variable={v} />
        ))}
      </div>
    </aside>
  );
}

function VariableCard({ dir, variable: v }: { dir: string; variable: ResolvedVar }) {
  const [revealed, setRevealed] = useState<string | null>(null);
  // Secrets can be revealed when they come straight from an environment.
  const env =
    v.source?.kind === "collectionEnv"
      ? { scope: { kind: "collection" as const, dir }, name: v.source.name }
      : v.source?.kind === "globalEnv"
        ? { scope: { kind: "global" as const }, name: v.source.name }
        : null;

  const toggleReveal = async () => {
    if (revealed !== null) {
      setRevealed(null);
      return;
    }
    if (!env) return;
    try {
      setRevealed((await unwrap(commands.revealSecret(env.scope, env.name, v.name))) ?? "");
    } catch (e) {
      showError(e);
    }
  };

  return (
    <div className="mb-2 rounded-md border bg-background p-2">
      <div className="flex items-center gap-1.5">
        <span className="font-mono font-semibold">{v.name}</span>
        {v.isSecret && <Badge variant="outline">secret</Badge>}
        <div className="flex-1" />
        <Badge variant={v.resolved ? "secondary" : "destructive"}>{sourceLabel(v.source)}</Badge>
      </div>
      <div className="mt-1 flex items-start gap-1">
        <span
          className={cn(
            "flex-1 font-mono break-all",
            !v.resolved && "text-destructive",
            v.masked && revealed === null && "text-muted-foreground",
          )}
        >
          {revealed ?? displayValue(v)}
        </span>
        {v.masked && env && (
          <Button
            variant="ghost"
            size="icon"
            className="size-5"
            aria-label={revealed === null ? "Reveal" : "Hide"}
            onClick={() => void toggleReveal()}
          >
            {revealed === null ? <Eye /> : <EyeOff />}
          </Button>
        )}
      </div>
      {v.shadowed.length > 0 && (
        <div className="mt-1.5 border-t pt-1.5">
          <span className="text-muted-foreground">Overrides</span>
          {v.shadowed.map((s, i) => (
            <div
              key={i}
              className="flex gap-2 text-muted-foreground line-through decoration-muted-foreground/50"
            >
              <span className="shrink-0">{sourceLabel(s.source)}:</span>
              <span className="truncate font-mono">
                {s.isSecret ? "••••••••" : (s.value ?? "unset")}
              </span>
            </div>
          ))}
        </div>
      )}
      <div className="mt-1.5 text-muted-foreground">
        Used in: {v.usedIn.map(usageLabel).join(", ")}
      </div>
    </div>
  );
}
