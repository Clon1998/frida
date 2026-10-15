import { Eye, EyeOff, Lock, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { commands, type EnvScope } from "@/bindings";
import { CellInput, GridCell, GridRow, GridTable } from "@/components/request/GridTable";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ConfirmDialog } from "@/components/workspace/ConfirmDialog";
import { NameDialog } from "@/components/workspace/NameDialog";
import { showError, unwrap } from "@/lib/api";
import { forDisk, secretOps } from "@/lib/envs";
import { type EnvEntry, type EnvModel, type EnvVar, scopeOf, useEnvs } from "@/stores/envs";
import { useActiveCollection } from "@/stores/workspace";
import { cn } from "cn";

type ScopeKind = "global" | "collection";

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
  const [selected, setSelected] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
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

  const locked = backend !== null && !backend.keychain && !backend.unlocked;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b px-3 py-2">
        <Tabs value={kind} onValueChange={(v) => setKind(v as ScopeKind)}>
          <TabsList>
            <TabsTrigger value="collection">Collection: {collection?.name}</TabsTrigger>
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
            {entries.map((e) => (
              <button
                key={e.environment.name}
                type="button"
                onClick={() => setSelected(e.environment.name)}
                className={cn(
                  "truncate rounded px-2 py-1.5 text-left text-sm hover:bg-accent",
                  current?.environment.name === e.environment.name && "bg-accent font-medium",
                )}
              >
                {e.environment.name}
              </button>
            ))}
            {entries.length === 0 && (
              <p className="px-2 py-1 text-xs text-muted-foreground">No environments yet.</p>
            )}
            <Button size="sm" variant="outline" className="mt-1" onClick={() => setCreating(true)}>
              <Plus /> New environment
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
              />
            )}
          </div>
        </div>
      )}

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
}

/** Edits one environment. Changes are kept here until "Save". */
function EnvForm({ dir, scope, entry, onRenamed }: EnvFormProps) {
  const original = entry.environment;
  const [draft, setDraft] = useState<EnvModel>(original);
  /** Secret values typed in this session, by variable name. */
  const [pending, setPending] = useState<Record<string, string>>({});
  /** Secret values the user revealed, by variable name. */
  const [revealed, setRevealed] = useState<Record<string, string>>({});
  const [deleting, setDeleting] = useState(false);
  /** Like Bruno: plain variables and secrets are edited on separate tabs. */
  const [view, setView] = useState<"vars" | "secrets">("vars");
  const keychain = useEnvs((s) => s.backend?.keychain !== false);

  const dirty =
    JSON.stringify(draft) !== JSON.stringify(original) || Object.keys(pending).length > 0;

  const updateVar = (index: number, patch: Partial<EnvVar>) =>
    setDraft((d) => {
      if (index === d.variables.length) {
        const added: EnvVar = { name: "", value: "", secret: false, source: null, ...patch };
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
      setPending({});
      setRevealed({});
      if (cleaned.name !== original.name) onRenamed(cleaned.name);
      await useEnvs.getState().load(dir);
      toast.success(`Saved "${cleaned.name}"`);
    } catch (e) {
      showError(e);
    }
  };

  const remove = async () => {
    try {
      await unwrap(commands.deleteEnvironment(scope, original.name));
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
      v: { name: "", value: secrets ? null : "", secret: secrets, source: null } as EnvVar,
      index: draft.variables.length,
    },
  ];
  const secretCount = draft.variables.filter((v) => v.secret).length;

  return (
    <div className="flex max-w-4xl flex-col gap-3">
      <div className="flex items-end gap-2">
        <label className="flex flex-col gap-1 text-sm">
          Name
          <Input
            className="h-8 w-64"
            value={draft.name}
            onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))}
          />
        </label>
        <div className="flex-1" />
        <Button
          variant="outline"
          size="sm"
          disabled={!dirty}
          onClick={() => {
            setDraft(original);
            setPending({});
          }}
        >
          Revert
        </Button>
        <Button size="sm" disabled={!dirty} onClick={() => void save()}>
          Save
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

      <Tabs value={view} onValueChange={(v) => setView(v as "vars" | "secrets")}>
        <TabsList>
          <TabsTrigger value="vars">Variables ({draft.variables.length - secretCount})</TabsTrigger>
          <TabsTrigger value="secrets">Secrets ({secretCount})</TabsTrigger>
        </TabsList>
      </Tabs>
      {secrets && (
        <p className="-mt-1 text-xs text-muted-foreground">
          Secret values are stored in {keychain ? "the OS keychain" : "an encrypted file"} on this
          machine. Only the names are saved in the environment file, so teammates see which secrets
          they need to fill in.
        </p>
      )}

      <GridTable
        columns={[
          { label: secrets ? "Secret" : "Variable", width: "30%" },
          { label: "Value" },
          { label: "", width: "2.25rem" },
        ]}
      >
        {rows.map(({ v, index }) => {
          const isNew = index === draft.variables.length;
          const wasSet = entry.setSecrets.includes(v.name);
          const shown = revealed[v.name];
          return (
            <GridRow key={index}>
              <GridCell>
                <CellInput
                  placeholder={isNew ? (secrets ? "Add secret" : "Add variable") : "name"}
                  value={v.name}
                  onChange={(e) =>
                    updateVar(index, { name: e.target.value, secret: secrets, value: v.value })
                  }
                />
              </GridCell>
              <GridCell>
                {v.secret ? (
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
                    {v.source && <Badge variant="secondary">AWS</Badge>}
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
                    onClick={() =>
                      setDraft((d) => ({
                        ...d,
                        variables: d.variables.filter((_, i) => i !== index),
                      }))
                    }
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
