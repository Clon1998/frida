import { arrayMove } from "@dnd-kit/sortable";
import { Ban, ClipboardPaste, Copy, Eye, EyeOff, Lock, Plus, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { commands, type EnvScope } from "@/bindings";
import { CellInput, GridCell, GridRow, GridTable } from "@/components/request/GridTable";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ConfirmDialog } from "@/components/workspace/ConfirmDialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { AwsProfilePicker } from "./AwsProfilePicker";
import { type EnvActions, type EnvTarget, EnvironmentList } from "./EnvironmentList";
import { useRowIds } from "@/components/request/useRowIds";
import { NameDialog } from "@/components/workspace/NameDialog";
import { showError, toApiError, unwrap } from "@/lib/api";
import { forDisk, secretOps } from "@/lib/envs";
import {
  type EnvDraft,
  type EnvEntry,
  type EnvModel,
  type EnvVar,
  envDraftKey,
  scopeOf,
  useEnvs,
} from "@/stores/envs";
import { useSubTab } from "@/stores/subTabs";
import { useActiveCollection, useWorkspace } from "@/stores/workspace";
import { ENV_COLORS } from "@/lib/envColors";
import { cn } from "cn";

type ScopeKind = "global" | "collection";

const NO_PENDING: Record<string, string> = {};

/**
 * The environment editor tab (PLAN.md §14): global and collection environments, their
 * variables, and secret values, which go to the OS keychain and never into the YAML file.
 */
export function EnvironmentsEditor({ dir }: { dir: string }) {
  const collection = useActiveCollection();
  const backend = useEnvs((s) => s.backend);
  const kind = useEnvs((s) => s.editorScope);
  const setKind = useEnvs((s) => s.setEditorScope);
  const entries = useEnvs((s) => (kind === "global" ? s.global : s.collection));
  const selected = useEnvs((s) => s.editorSelected[kind]);
  const setSelectedIn = useEnvs((s) => s.setEditorSelected);
  const setSelected = (name: string | null) => setSelectedIn(kind, name);
  const [creating, setCreating] = useState(false);
  const [pasting, setPasting] = useState(false);
  const scope = scopeOf(kind, dir);

  // Keep a valid selection when the list or the scope changes.
  const current = entries.find((e) => e.environment.name === selected) ?? entries[0] ?? null;

  const create = async (name: string) => {
    try {
      await unwrap(commands.createEnvironment(scope, name));
      await useEnvs.getState().load(dir);
      setSelected(name);
    } catch (e) {
      showError(e);
    }
  };

  /** Copies the saved environment `name` (with its secret values) and selects the copy. */
  const duplicate = async (name: string) => {
    try {
      const copy = await unwrap(commands.duplicateEnvironment(scope, name));
      await useEnvs.getState().load(dir);
      setSelected(copy);
      toast.success(`Duplicated as "${copy}"`);
    } catch (e) {
      showError(e);
    }
  };

  const workspace = useWorkspace((s) => s.workspace);
  /** Global for collection environments; the collections (this one first) for global ones. */
  const targets: EnvTarget[] =
    kind === "collection"
      ? [
          { label: "Global (workspace)", scope: { kind: "global" } },
          ...(workspace?.collections ?? [])
            .filter((c) => c.error === null && c.dir !== dir)
            .map((c) => ({ label: c.name, scope: scopeOf("collection", c.dir) })),
        ]
      : (workspace?.collections ?? [])
          .filter((c) => c.error === null)
          .sort((a, b) => (a.dir === dir ? -1 : b.dir === dir ? 1 : 0))
          .map((c) => ({
            label: c.dir === dir ? `${c.name} (this collection)` : c.name,
            scope: scopeOf("collection", c.dir),
          }));

  const actions: EnvActions = {
    duplicate: (name) => void duplicate(name),
    copyToClipboard: (name) =>
      void (async () => {
        try {
          await navigator.clipboard.writeText(await unwrap(commands.environmentYaml(scope, name)));
          toast.success(`Copied "${name}"`, {
            description:
              "Paste it into another collection's environments. Secret values are not included.",
          });
        } catch (e) {
          showError(e);
        }
      })(),
    transfer: (name, target, mode) =>
      void (async () => {
        try {
          const added = await unwrap(commands.transferEnvironment(scope, name, target.scope, mode));
          await useEnvs.getState().load(dir);
          if (mode === "move" && current?.environment.name === name) setSelected(null);
          // Global ↔ this collection: both are in this editor, so offer to go there.
          const here: ScopeKind | null =
            target.scope.kind === "global"
              ? "global"
              : target.scope.dir === dir
                ? "collection"
                : null;
          toast.success(
            `${mode === "copy" ? "Copied" : "Moved"} "${name}" to ${target.label}` +
              (added !== name ? ` as "${added}"` : ""),
            here
              ? {
                  action: {
                    label: "Show",
                    onClick: () => {
                      setKind(here);
                      setSelectedIn(here, added);
                    },
                  },
                }
              : undefined,
          );
        } catch (e) {
          showError(e);
        }
      })(),
  };

  const paste = async (yaml: string) => {
    const name = await unwrap(commands.pasteEnvironment(scope, yaml));
    await useEnvs.getState().load(dir);
    setSelected(name);
    toast.success(`Added "${name}"`);
  };

  const locked = backend !== null && !backend.keychain && !backend.unlocked;

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b px-3 py-2">
        <Tabs value={kind} onValueChange={(v) => setKind(v as ScopeKind)}>
          <TabsList>
            <TabsTrigger value="collection">
              <span className="max-w-48 truncate">Collection: {collection?.name}</span>
            </TabsTrigger>
            <TabsTrigger value="global">Global (workspace)</TabsTrigger>
          </TabsList>
        </Tabs>
        <p className="text-xs text-muted-foreground">
          Collection values override global ones. Secrets are stored in{" "}
          {backend?.keychain === false ? "an encrypted file" : "the OS keychain"}, never in git.
        </p>
      </div>

      {locked ? (
        <UnlockForm fileExists={backend.fileExists} />
      ) : (
        <div className="flex min-h-0 flex-1">
          <aside className="flex w-52 shrink-0 flex-col gap-1 border-r p-2">
            <EnvironmentList
              dir={dir}
              scope={scope}
              entries={entries}
              selected={current?.environment.name ?? null}
              onSelect={setSelected}
              targets={targets}
              actions={actions}
            />
            {entries.length === 0 && (
              <p className="px-2 py-1 text-xs text-muted-foreground">No environments yet.</p>
            )}
            <Button size="sm" variant="outline" className="mt-1" onClick={() => setCreating(true)}>
              <Plus /> New environment
            </Button>
            <Button
              size="sm"
              variant="ghost"
              title="Add an environment copied with “Copy to clipboard”"
              onClick={() => setPasting(true)}
            >
              <ClipboardPaste /> Paste…
            </Button>
          </aside>
          <div className="min-w-0 flex-1 overflow-auto p-3">
            {current && (
              // `key` remounts the form, resetting its state, when another environment is
              // selected or the saved file changes (after a save, or a git pull).
              <EnvForm
                key={`${kind}:${JSON.stringify(current)}`}
                dir={dir}
                scope={scope}
                entry={current}
                onRenamed={setSelected}
                onDuplicate={() => void duplicate(current.environment.name)}
              />
            )}
          </div>
        </div>
      )}

      <PasteEnvironmentDialog open={pasting} onOpenChange={setPasting} onPaste={paste} />
      <NameDialog
        open={creating}
        title="New environment"
        label="Name"
        onOpenChange={setCreating}
        onConfirm={(name) => void create(name)}
      />
    </div>
  );
}

/**
 * "Paste…": a box to paste an environment into (Ctrl/Cmd+V). A dialog instead of reading the
 * clipboard directly, which the webview may only allow after a system prompt.
 */
function PasteEnvironmentDialog({
  open,
  onOpenChange,
  onPaste,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onPaste: (yaml: string) => Promise<void>;
}) {
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const close = (next: boolean) => {
    if (!next) {
      setText("");
      setError(null);
    }
    onOpenChange(next);
  };
  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Paste environment</DialogTitle>
          <DialogDescription>
            Paste an environment copied with &ldquo;Copy to clipboard&rdquo; (Ctrl/Cmd+V). If the
            name is taken, it gets a number.
          </DialogDescription>
        </DialogHeader>
        <Textarea
          autoFocus
          aria-label="Environment"
          className="h-48 font-mono text-xs"
          spellCheck={false}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setError(null);
          }}
        />
        {error && <p className="text-xs text-destructive">{error}</p>}
        <DialogFooter>
          <Button variant="outline" onClick={() => close(false)}>
            Cancel
          </Button>
          <Button
            disabled={!text.trim()}
            onClick={() =>
              void onPaste(text).then(
                () => close(false),
                (e) => setError(toApiError(e).message),
              )
            }
          >
            Add environment
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function UnlockForm({ fileExists }: { fileExists: boolean }) {
  const unlock = useEnvs((s) => s.unlock);
  const [password, setPassword] = useState("");
  return (
    <form
      className="m-auto flex w-80 flex-col gap-3 text-sm"
      onSubmit={(e) => {
        e.preventDefault();
        void unlock(password).then((ok) => ok && setPassword(""));
      }}
    >
      <Lock className="mx-auto size-6 text-muted-foreground" />
      <p className="text-center text-muted-foreground">
        No OS keychain is available, so secrets are kept in an encrypted file.{" "}
        {fileExists ? "Enter your master password to unlock it." : "Choose a master password."}
      </p>
      <Input
        type="password"
        autoFocus
        placeholder="Master password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
      />
      <Button type="submit" disabled={password.length === 0}>
        {fileExists ? "Unlock" : "Create secret file"}
      </Button>
    </form>
  );
}

interface EnvFormProps {
  dir: string;
  scope: EnvScope;
  entry: EnvEntry;
  onRenamed: (name: string) => void;
  onDuplicate: () => void;
}

type AwsSource = NonNullable<EnvVar["source"]>;
type SourceKind = "keychain" | AwsSource["type"];

/** The secret sources (PLAN.md §9.1, M17), as offered in the "Source" column. */
const SOURCE_KINDS: { value: AwsSource["type"]; label: string }[] = [
  { value: "aws-secrets-manager", label: "Secrets Manager" },
  { value: "cognito-client-secret", label: "Cognito client secret" },
  { value: "api-gateway-key", label: "API Gateway key" },
];

/** An empty source of `kind`, keeping the region and account of `previous` when switching. */
function newSource(kind: AwsSource["type"], previous: AwsSource | null): AwsSource {
  const shared = { region: previous?.region ?? null, account: previous?.account ?? null };
  switch (kind) {
    case "aws-secrets-manager":
      return { type: kind, secretId: "", jsonKey: null, ...shared };
    case "cognito-client-secret":
      return { type: kind, userPoolId: "", clientId: "", ...shared };
    case "api-gateway-key":
      return { type: kind, apiKeyId: "", ...shared };
  }
}

/**
 * The fields of an AWS-backed secret: what to read (per source type), plus the optional region
 * and account alias (the alias picks another AWS profile than the environment's, M17).
 */
function AwsSourceFields({
  source,
  onChange,
}: {
  source: AwsSource;
  onChange: (source: AwsSource) => void;
}) {
  // `{ ...source, field }` keeps the type: each branch below only sets fields of its own kind.
  const set = (patch: Partial<AwsSource>) => onChange({ ...source, ...patch } as AwsSource);
  const optional = (value: string) => value.trim() || null;
  // Side by side when the cell is wide enough (20rem), else stacked (a container query on the
  // cell, not the window).
  return (
    <div className="@container">
      <div className="flex flex-col divide-y @xs:flex-row @xs:divide-x @xs:divide-y-0">
        {source.type === "aws-secrets-manager" && (
          <>
            <CellInput
              className="@xs:flex-[2]"
              placeholder="secret id or ARN"
              value={source.secretId}
              onChange={(e) => set({ secretId: e.target.value })}
            />
            <CellInput
              className="@xs:flex-1"
              placeholder="JSON key (optional)"
              value={source.jsonKey ?? ""}
              onChange={(e) => set({ jsonKey: optional(e.target.value) })}
            />
          </>
        )}
        {source.type === "cognito-client-secret" && (
          <>
            <CellInput
              className="@xs:flex-[2]"
              placeholder="user pool id (eu-central-1_…)"
              value={source.userPoolId}
              onChange={(e) => set({ userPoolId: e.target.value })}
            />
            <CellInput
              className="@xs:flex-[2]"
              placeholder="app client id"
              value={source.clientId}
              onChange={(e) => set({ clientId: e.target.value })}
            />
          </>
        )}
        {source.type === "api-gateway-key" && (
          <CellInput
            className="@xs:flex-[2]"
            placeholder="API key id"
            value={source.apiKeyId}
            onChange={(e) => set({ apiKeyId: e.target.value })}
          />
        )}
        <CellInput
          className="@xs:flex-1"
          placeholder={
            source.type === "cognito-client-secret" ? "region (pool's)" : "region (profile's)"
          }
          value={source.region ?? ""}
          onChange={(e) => set({ region: optional(e.target.value) })}
        />
        <CellInput
          className="@xs:flex-1"
          placeholder="account (optional)"
          title="An AWS account alias, e.g. shared-services. Each person picks the profile for it once; without one, the environment's profile is used."
          value={source.account ?? ""}
          onChange={(e) => set({ account: optional(e.target.value) })}
        />
      </div>
    </div>
  );
}

/** Edits one environment. Changes are kept here until "Save". */
function EnvForm({ dir, scope, entry, onRenamed, onDuplicate }: EnvFormProps) {
  const original = entry.environment;
  const draftKey = envDraftKey(scope, original.name);
  const stored = useEnvs((s) => s.editorDrafts[draftKey]);
  const setStored = useEnvs((s) => s.setEditorDraft);
  const draft = stored?.draft ?? original;
  /** Secret values typed in this session, by variable name. */
  const pending = stored?.pending ?? NO_PENDING;
  /** Keeps an entry in the store only while something differs from the saved environment. */
  const store = (next: EnvDraft) => {
    const changed =
      JSON.stringify(next.draft) !== JSON.stringify(original) ||
      Object.keys(next.pending).length > 0;
    setStored(draftKey, changed ? next : null);
  };
  const setDraft = (change: EnvModel | ((d: EnvModel) => EnvModel)) => {
    const current = useEnvs.getState().editorDrafts[draftKey];
    const base = current?.draft ?? original;
    store({
      draft: typeof change === "function" ? change(base) : change,
      pending: current?.pending ?? {},
    });
  };
  const setPending = (
    change: Record<string, string> | ((p: Record<string, string>) => Record<string, string>),
  ) => {
    const current = useEnvs.getState().editorDrafts[draftKey];
    const base = current?.pending ?? {};
    store({
      draft: current?.draft ?? original,
      pending: typeof change === "function" ? change(base) : change,
    });
  };
  /** Secret values the user revealed, by variable name. */
  const [revealed, setRevealed] = useState<Record<string, string>>({});
  const [deleting, setDeleting] = useState(false);
  /** Like Bruno: plain variables and secrets are edited on separate tabs. */
  const [view, setView] = useSubTab(`env:${JSON.stringify(scope)}:${original.name}`, "vars");
  const keychain = useEnvs((s) => s.backend?.keychain !== false);

  const dirty =
    JSON.stringify(draft) !== JSON.stringify(original) || Object.keys(pending).length > 0;

  const updateVar = (index: number, patch: Partial<EnvVar>) =>
    setDraft((d) => {
      if (index === d.variables.length) {
        const added: EnvVar = {
          name: "",
          value: "",
          secret: false,
          source: null,
          enabled: true,
          ...patch,
        };
        return { ...d, variables: [...d.variables, added] };
      }
      return { ...d, variables: d.variables.map((v, i) => (i === index ? { ...v, ...patch } : v)) };
    });

  const reveal = async (name: string) => {
    if (revealed[name] !== undefined) {
      setRevealed((r) => Object.fromEntries(Object.entries(r).filter(([key]) => key !== name)));
      return;
    }
    try {
      const value = await unwrap(commands.revealSecret(scope, original.name, name));
      setRevealed((r) => ({ ...r, [name]: value ?? "" }));
    } catch (e) {
      showError(e);
    }
  };

  const save = async () => {
    try {
      const names = draft.variables.map((v) => v.name.trim()).filter(Boolean);
      if (new Set(names).size !== names.length) {
        toast.error("Variable names must be unique within an environment.");
        return;
      }
      const cleaned = { ...draft, variables: draft.variables.filter((v) => v.name.trim()) };
      await unwrap(commands.saveEnvironment(scope, original.name, forDisk(cleaned)));
      const ops = secretOps(original, cleaned, pending);
      for (const { variable, value } of ops.set) {
        await unwrap(commands.setSecret(scope, cleaned.name, variable, value));
      }
      for (const variable of ops.delete) {
        await unwrap(commands.deleteSecret(scope, cleaned.name, variable));
      }
      if (cleaned.name !== original.name) onRenamed(cleaned.name);
      await useEnvs.getState().load(dir);
      // Only now: dropping the draft before the reload would briefly show the old values.
      setStored(draftKey, null);
      setRevealed({});
      toast.success(`Saved "${cleaned.name}"`);
    } catch (e) {
      showError(e);
    }
  };

  // Ctrl/Cmd+S saves, like in request tabs. A ref so the listener always calls the latest
  // `save` (which sees the current draft) without re-registering on every keystroke.
  const saveRef = useRef<() => void>(() => {});
  useEffect(() => {
    saveRef.current = () => {
      if (dirty) void save();
    };
  });
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "s") {
        e.preventDefault();
        saveRef.current();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const remove = async () => {
    try {
      await unwrap(commands.deleteEnvironment(scope, original.name));
      setStored(draftKey, null);
      await useEnvs.getState().load(dir);
    } catch (e) {
      showError(e);
    }
  };

  const secrets = view === "secrets";
  // Rows of the current tab with their position in `draft.variables`; the extra row adds a new
  // variable of this tab's kind at the end.
  const rows = [
    ...draft.variables.map((v, index) => ({ v, index })).filter(({ v }) => v.secret === secrets),
    {
      v: {
        name: "",
        value: secrets ? null : "",
        secret: secrets,
        source: null,
        enabled: true,
      } as EnvVar,
      index: draft.variables.length,
    },
  ];
  const secretCount = draft.variables.filter((v) => v.secret).length;
  // Stable row ids per tab (variables and secrets are separate lists), see useRowIds.
  const varIds = useRowIds(draft.variables.length - secretCount);
  const secretIds = useRowIds(secretCount);
  const rowIds = secrets ? secretIds : varIds;

  return (
    <div className="flex max-w-4xl flex-col gap-3">
      {/* Wraps in a narrow pane: name and colors first, the buttons move to the next line. */}
      <div className="flex flex-wrap items-end gap-x-2 gap-y-3">
        <label className="flex w-64 max-w-full flex-col gap-1 text-sm">
          Name
          <Input
            className="h-8"
            value={draft.name}
            onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))}
          />
        </label>
        <div className="flex flex-col gap-1 text-sm">
          Color
          <div className="flex min-h-8 flex-wrap items-center gap-1">
            <button
              type="button"
              title="No color"
              aria-label="No color"
              onClick={() => setDraft((d) => ({ ...d, color: null }))}
              className={cn(
                "flex size-5 items-center justify-center rounded-full text-muted-foreground hover:text-foreground",
                draft.color === null && "text-foreground ring-2 ring-ring ring-offset-1",
              )}
            >
              <Ban className="size-5" />
            </button>
            {ENV_COLORS.map((c) => (
              <button
                key={c.name}
                type="button"
                title={c.name}
                aria-label={c.name}
                onClick={() => setDraft((d) => ({ ...d, color: c.name }))}
                className={cn(
                  "size-5 rounded-full",
                  c.dot,
                  draft.color === c.name && "ring-2 ring-ring ring-offset-1",
                )}
              />
            ))}
          </div>
        </div>
        <div className="ml-auto flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={!dirty}
            onClick={() => {
              setStored(draftKey, null);
            }}
          >
            Revert
          </Button>
          <Button size="sm" disabled={!dirty} title="Save (Ctrl/Cmd+S)" onClick={() => void save()}>
            Save
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="size-8"
            title={dirty ? "Duplicate (copies the saved version)" : "Duplicate"}
            aria-label="Duplicate environment"
            onClick={onDuplicate}
          >
            <Copy />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="size-8"
            aria-label="Delete environment"
            onClick={() => setDeleting(true)}
          >
            <Trash2 />
          </Button>
        </div>
      </div>

      <Tabs value={view} onValueChange={setView}>
        <TabsList>
          <TabsTrigger value="vars">Variables ({draft.variables.length - secretCount})</TabsTrigger>
          <TabsTrigger value="secrets">Secrets ({secretCount})</TabsTrigger>
        </TabsList>
      </Tabs>
      {/* Secrets with an account alias use that account's profile (Settings → AWS). */}
      {draft.variables.some((v) => v.secret && v.source && !v.source.account) && (
        <AwsProfilePicker scope={scope} environment={original.name} />
      )}
      {secrets && (
        <p className="-mt-1 text-xs text-muted-foreground">
          Secret values are stored in {keychain ? "the OS keychain" : "an encrypted file"} on this
          machine. Only the names are saved in the environment file, so teammates see which secrets
          they need to fill in. A secret can also come from AWS: a Secrets Manager secret, a Cognito
          app client secret or an API Gateway key (choose it as the source).
        </p>
      )}

      <GridTable
        reorder={{
          ids: rowIds.ids,
          onMove: (from, to) => {
            rowIds.move(from, to);
            // Rows are this tab's variables only; move within them, keep the others in place.
            setDraft((d) => {
              const positions = rows.slice(0, -1).map((r) => r.index);
              const moved = arrayMove(
                positions.map((i) => d.variables[i]),
                from,
                to,
              );
              const variables = [...d.variables];
              positions.forEach((pos, k) => {
                const v = moved[k];
                if (v) variables[pos] = v;
              });
              return { ...d, variables };
            });
          },
        }}
        columns={[
          { label: "", width: "2.25rem" },
          { label: secrets ? "Secret" : "Variable", width: "30%" },
          ...(secrets ? [{ label: "Source", width: "7.5rem" }] : []),
          { label: "Value" },
          { label: "", width: "2.25rem" },
        ]}
      >
        {rows.map(({ v, index }, rowIndex) => {
          const isNew = index === draft.variables.length;
          const wasSet = entry.setSecrets.includes(v.name);
          const shown = revealed[v.name];
          return (
            <GridRow
              key={isNew ? rowIds.addId : rowIds.ids[rowIndex]}
              muted={!v.enabled}
              sortIndex={isNew ? undefined : rowIndex}
            >
              <GridCell center>
                <Checkbox
                  className="align-middle"
                  checked={v.enabled}
                  disabled={isNew}
                  onCheckedChange={(checked) => updateVar(index, { enabled: checked === true })}
                  aria-label="Enabled"
                />
              </GridCell>
              <GridCell>
                <CellInput
                  placeholder={isNew ? (secrets ? "Add secret" : "Add variable") : "name"}
                  value={v.name}
                  onChange={(e) =>
                    updateVar(index, { name: e.target.value, secret: secrets, value: v.value })
                  }
                />
              </GridCell>
              {secrets && (
                <GridCell>
                  <Select
                    value={(v.source?.type ?? "keychain") satisfies SourceKind}
                    disabled={isNew}
                    onValueChange={(kind: SourceKind) =>
                      updateVar(index, {
                        source: kind === "keychain" ? null : newSource(kind, v.source ?? null),
                      })
                    }
                  >
                    <SelectTrigger
                      size="sm"
                      className="h-8 w-full rounded-none border-0 shadow-none"
                      aria-label="Secret source"
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="keychain">
                        {keychain ? "Keychain" : "Local file"}
                      </SelectItem>
                      {SOURCE_KINDS.map((k) => (
                        <SelectItem key={k.value} value={k.value}>
                          AWS · {k.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </GridCell>
              )}
              <GridCell>
                {v.secret && v.source ? (
                  <AwsSourceFields
                    source={v.source}
                    onChange={(source) => updateVar(index, { source })}
                  />
                ) : v.secret ? (
                  <div className="flex items-center gap-1 pr-1">
                    <CellInput
                      type={shown !== undefined ? "text" : "password"}
                      // Secret values are keyed by name, so a new secret needs its name first.
                      disabled={isNew}
                      placeholder={
                        isNew ? "enter a name first" : wasSet ? "•••••••• (stored)" : "unset"
                      }
                      value={pending[v.name] ?? shown ?? ""}
                      onChange={(e) => setPending((p) => ({ ...p, [v.name]: e.target.value }))}
                    />
                    {!isNew && !wasSet && pending[v.name] === undefined && (
                      <Badge variant="outline">unset</Badge>
                    )}
                    {wasSet && pending[v.name] === undefined && (
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-7"
                        aria-label={shown !== undefined ? "Hide value" : "Reveal value"}
                        onClick={() => void reveal(v.name)}
                      >
                        {shown !== undefined ? <EyeOff /> : <Eye />}
                      </Button>
                    )}
                  </div>
                ) : (
                  <CellInput
                    placeholder="value"
                    value={v.value ?? ""}
                    onChange={(e) => updateVar(index, { value: e.target.value })}
                  />
                )}
              </GridCell>
              <GridCell center>
                {!isNew && (
                  <Button
                    variant="ghost"
                    size="icon"
                    className="size-7"
                    aria-label="Remove variable"
                    onClick={() => {
                      rowIds.remove(rowIndex);
                      setDraft((d) => ({
                        ...d,
                        variables: d.variables.filter((_, i) => i !== index),
                      }));
                    }}
                  >
                    <Trash2 />
                  </Button>
                )}
              </GridCell>
            </GridRow>
          );
        })}
      </GridTable>

      <ConfirmDialog
        open={deleting}
        title={`Delete environment "${original.name}"?`}
        description="The environment file and its stored secret values will be deleted."
        confirmLabel="Delete"
        destructive
        onOpenChange={setDeleting}
        onConfirm={() => void remove()}
      />
    </div>
  );
}
