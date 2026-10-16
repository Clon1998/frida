import { create } from "zustand";
import {
  commands,
  type EnvScope,
  type EnvVariable_Deserialize,
  type Environment,
  type SecretBackendStatus,
} from "@/bindings";
import { showError, unwrap } from "@/lib/api";
import type { Complete } from "@/lib/model";

/** An environment variable as the editor works with it: every field present. */
export type EnvVar = Complete<EnvVariable_Deserialize>;

/** An environment as the editor works with it. */
export interface EnvModel {
  version: number;
  name: string;
  color: string | null;
  variables: EnvVar[];
}

export interface EnvEntry {
  environment: EnvModel;
  /** Secret variables that have a value in the keychain. */
  setSecrets: string[];
}

/**
 * Environments arrive with optional fields (a secret has no `value` at all, see the generated
 * `EnvVariable_Serialize`). This fills in the gaps once, so the editor never deals with
 * `undefined`.
 */
export function normalizeEnv(env: Environment): EnvModel {
  return {
    version: env.version,
    name: env.name,
    color: env.color ?? null,
    variables: (env.variables ?? []).map((v) => ({
      name: v.name,
      value: v.value ?? null,
      secret: v.secret ?? false,
      source: (v.source ?? null) as EnvVar["source"],
      enabled: v.enabled ?? true,
    })),
  };
}

interface EnvsState {
  /** For the collection folder the lists were loaded for. */
  dir: string | null;
  global: EnvEntry[];
  collection: EnvEntry[];
  activeGlobal: string | null;
  activeCollection: string | null;
  backend: SecretBackendStatus | null;
  /** Bumped whenever environments or the selection change, so the inspector refreshes. */
  version: number;
  /** Which scope the environment editor tab shows. */
  editorScope: "global" | "collection";
  setEditorScope: (scope: "global" | "collection") => void;
  /**
   * The environment selected in the editor, per scope. Kept here (not in the component) so it
   * survives switching to another tab and back.
   */
  editorSelected: { global: string | null; collection: string | null };
  setEditorSelected: (scope: "global" | "collection", name: string | null) => void;

  /**
   * Unsaved edits in the environment editor, by {@link envDraftKey}. In the store (not the
   * form) so they survive switching tabs and so the tab bar can show the unsaved dot. Only
   * environments with changes have an entry.
   */
  editorDrafts: Record<string, EnvDraft>;
  /** Stores `draft` for `key`, or removes it when `null` (saved, reverted, discarded). */
  setEditorDraft: (key: string, draft: EnvDraft | null) => void;
  clearEditorDrafts: () => void;

  load: (dir: string) => Promise<void>;
  select: (scope: EnvScope, name: string | null) => Promise<void>;
  unlock: (password: string) => Promise<boolean>;
}

/** An environment being edited: the variables, plus secret values typed but not saved yet. */
export interface EnvDraft {
  draft: EnvModel;
  pending: Record<string, string>;
}

export const envDraftKey = (scope: EnvScope, name: string) => `${JSON.stringify(scope)}:${name}`;

/** Whether the environment editor has unsaved changes. */
export const envEditorDirty = (s: Pick<EnvsState, "editorDrafts">) =>
  Object.keys(s.editorDrafts).length > 0;

export const scopeOf = (kind: "global" | "collection", dir: string): EnvScope =>
  kind === "global" ? { kind: "global" } : { kind: "collection", dir };

export const useEnvs = create<EnvsState>((set, get) => ({
  dir: null,
  global: [],
  collection: [],
  activeGlobal: null,
  activeCollection: null,
  backend: null,
  version: 0,
  editorScope: "collection",
  setEditorScope: (editorScope) => set({ editorScope }),
  editorSelected: { global: null, collection: null },
  editorDrafts: {},
  setEditorDraft: (key, draft) =>
    set((s) => {
      const others = Object.fromEntries(Object.entries(s.editorDrafts).filter(([k]) => k !== key));
      return { editorDrafts: draft ? { ...others, [key]: draft } : others };
    }),
  clearEditorDrafts: () => set({ editorDrafts: {} }),
  setEditorSelected: (scope, name) =>
    set((s) => ({ editorSelected: { ...s.editorSelected, [scope]: name } })),

  load: async (dir) => {
    try {
      const backend = await commands.secretBackendStatus();
      const [global, collection, active] = await Promise.all([
        unwrap(commands.listEnvironments({ kind: "global" })),
        unwrap(commands.listEnvironments({ kind: "collection", dir })),
        unwrap(commands.getActiveEnvironments(dir)),
      ]);
      const entries = (list: typeof global) =>
        list.map((e) => ({ environment: normalizeEnv(e.environment), setSecrets: e.setSecrets }));
      set((s) => ({
        dir,
        backend,
        global: entries(global),
        collection: entries(collection),
        activeGlobal: active.global,
        activeCollection: active.collection,
        version: s.version + 1,
      }));
    } catch (e) {
      showError(e);
    }
  },

  select: async (scope, name) => {
    try {
      await unwrap(commands.setActiveEnvironment(scope, name));
      set((s) =>
        scope.kind === "global"
          ? { activeGlobal: name, version: s.version + 1 }
          : { activeCollection: name, version: s.version + 1 },
      );
    } catch (e) {
      showError(e);
    }
  },

  unlock: async (password) => {
    try {
      await unwrap(commands.unlockSecretFile(password));
      const dir = get().dir;
      if (dir) await get().load(dir);
      return true;
    } catch (e) {
      showError(e);
      return false;
    }
  },
}));
