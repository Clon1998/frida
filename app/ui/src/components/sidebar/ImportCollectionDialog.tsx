import { open } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, FileUp, KeyRound } from "lucide-react";
import { useState } from "react";
import { commands, type ImportSummary, type OpenApiGrouping } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { toApiError, unwrap } from "@/lib/api";
import { type ImportKind, useWorkspace } from "@/stores/workspace";

type Step =
  | { kind: "pick" }
  | { kind: "preview"; file: string; summary: ImportSummary }
  | { kind: "done"; summary: ImportSummary };

const TEXTS: Record<ImportKind, { title: string; intro: string; choose: string }> = {
  openapi: {
    title: "Import OpenAPI",
    intro: "Creates a new collection from an OpenAPI 3.0 or 3.1 file (YAML or JSON).",
    choose: "Choose file…",
  },
  bruno: {
    title: "Import Bruno collection",
    intro:
      "Creates a new collection from a Bruno collection folder (.bru files or the YAML format). The Bruno files are not changed.",
    choose: "Choose folder…",
  },
};

/**
 * "Import OpenAPI…" / "Import Bruno collection…": choose the source, see what it will create,
 * choose where, import, then see the summary (counts, secrets to fill in, warnings).
 * PLAN.md §11.1 and §11.2.
 */
export function ImportCollectionDialog({
  kind,
  onOpenChange,
}: {
  /** `null` when closed. */
  kind: ImportKind | null;
  onOpenChange: (open: boolean) => void;
}) {
  const isOpen = kind !== null;
  return (
    <Dialog open={isOpen} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {/* Mounted only while open, so every import starts fresh. */}
        {kind && <ImportFlow kind={kind} onClose={() => onOpenChange(false)} />}
      </DialogContent>
    </Dialog>
  );
}

function ImportFlow({ kind, onClose }: { kind: ImportKind; onClose: () => void }) {
  const importCollection = useWorkspace((s) => s.importCollection);
  const texts = TEXTS[kind];
  const [step, setStep] = useState<Step>({ kind: "pick" });
  const [target, setTarget] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // OpenAPI only: folders from the URL paths (like Postman and Bruno) or from the tags.
  const [grouping, setGrouping] = useState<OpenApiGrouping>("paths");

  const preview = async (file: string, grouping: OpenApiGrouping) => {
    setError(null);
    try {
      const summary = await unwrap(
        kind === "openapi" ? commands.previewOpenapi(file, grouping) : commands.previewBruno(file),
      );
      setStep({ kind: "preview", file, summary });
    } catch (e) {
      setError(toApiError(e).message);
    }
  };

  const pickFile = async () => {
    const file =
      kind === "openapi"
        ? await open({
            title: "Choose an OpenAPI file",
            filters: [{ name: "OpenAPI", extensions: ["yaml", "yml", "json"] }],
          })
        : await open({ directory: true, title: "Choose the Bruno collection folder" });
    if (typeof file !== "string") return;
    await preview(file, grouping);
  };

  const pickTarget = async () => {
    const dir = await open({ directory: true, title: "Folder for the new collection" });
    if (typeof dir === "string") setTarget(dir);
  };

  const runImport = async (file: string) => {
    setBusy(true);
    setError(null);
    try {
      setStep({ kind: "done", summary: await importCollection(kind, file, target, grouping) });
    } catch (e) {
      setError(toApiError(e).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>{step.kind === "done" ? "Imported" : texts.title}</DialogTitle>
        <DialogDescription>
          {step.kind === "pick" && texts.intro}
          {step.kind === "preview" && step.file}
          {step.kind === "done" && `“${step.summary.collectionName}” was added to the workspace.`}
        </DialogDescription>
      </DialogHeader>

      {step.kind === "pick" && (
        <Button variant="outline" className="w-full" onClick={() => void pickFile()}>
          <FileUp /> {texts.choose}
        </Button>
      )}

      {step.kind !== "pick" && <SummaryView summary={step.summary} done={step.kind === "done"} />}

      {step.kind === "preview" && kind === "openapi" && (
        <div className="flex items-center gap-2 text-sm">
          <p className="font-medium">Folders from</p>
          <Select
            value={grouping}
            onValueChange={(value) => {
              const next = value as OpenApiGrouping;
              setGrouping(next);
              void preview(step.file, next);
            }}
          >
            <SelectTrigger size="sm" className="w-56" aria-label="Folders from">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="paths">URL paths (/legs/resolve → legs › resolve)</SelectItem>
              <SelectItem value="tags">Tags (one folder per tag)</SelectItem>
            </SelectContent>
          </Select>
        </div>
      )}

      {step.kind === "preview" && (
        <div className="space-y-1 text-sm">
          <p className="font-medium">Save the collection in</p>
          <div className="flex items-center gap-2">
            <code className="min-w-0 flex-1 truncate rounded bg-muted px-2 py-1 text-xs">
              {target ?? "collections/ in the workspace (default)"}
            </code>
            <Button variant="outline" size="sm" onClick={() => void pickTarget()}>
              Choose…
            </Button>
            {target && (
              <Button variant="ghost" size="sm" onClick={() => setTarget(null)}>
                Default
              </Button>
            )}
          </div>
        </div>
      )}

      {error && <p className="text-sm text-destructive">{error}</p>}

      <DialogFooter>
        {step.kind === "preview" ? (
          <>
            <Button variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button disabled={busy} onClick={() => void runImport(step.file)}>
              Import
            </Button>
          </>
        ) : (
          <Button variant={step.kind === "done" ? "default" : "ghost"} onClick={onClose}>
            {step.kind === "done" ? "Done" : "Cancel"}
          </Button>
        )}
      </DialogFooter>
    </>
  );
}

function SummaryView({ summary, done }: { summary: ImportSummary; done: boolean }) {
  return (
    <div className="space-y-3 text-sm">
      <p>
        <span className="font-medium">{summary.collectionName}</span>: {summary.requests} requests
        {summary.folders > 0 && ` in ${summary.folders} folders`}
        {summary.environments > 0 && `, ${summary.environments} environment(s)`}
        {summary.baseUrl && (
          <>
            , base URL <code className="text-xs">{summary.baseUrl}</code> in a <code>default</code>{" "}
            environment
          </>
        )}
        .
      </p>
      {summary.secretsToFill.length > 0 && (
        <p className="flex gap-2">
          <KeyRound className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <span>
            {done ? "Fill in" : "Will need"} these secrets in the environment editor:{" "}
            {summary.secretsToFill.map((s) => (
              <code key={s} className="mr-1 text-xs">
                {s}
              </code>
            ))}
          </span>
        </p>
      )}
      {summary.warnings.length > 0 && (
        <div className="max-h-48 overflow-auto rounded-md border bg-warning/5 p-2">
          <p className="mb-1 flex items-center gap-1 font-medium text-warning">
            <AlertTriangle className="size-4" /> {summary.warnings.length} warnings
          </p>
          <ul className="list-disc space-y-1 pl-5 text-xs">
            {summary.warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
