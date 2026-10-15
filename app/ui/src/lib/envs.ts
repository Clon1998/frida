import type { EnvModel } from "@/stores/envs";

/** Keychain operations needed to save an environment edited in the UI. */
export interface SecretOps {
  set: { variable: string; value: string }[];
  delete: string[];
}

/**
 * Which secrets to write and delete when saving `draft`:
 * - every secret with a newly typed value (`pending`) is written;
 * - secrets that no longer exist as secrets (deleted, renamed, or switched to plain) are
 *   deleted, so no orphaned values stay in the keychain.
 *
 * Apply them *after* saving the file, under `draft.name` (a renamed environment's secrets have
 * already been moved by then).
 */
export function secretOps(
  original: EnvModel | null,
  draft: EnvModel,
  pending: Record<string, string>,
): SecretOps {
  const draftSecrets = new Set(draft.variables.filter((v) => v.secret).map((v) => v.name));
  return {
    set: draft.variables
      .filter((v) => v.secret && pending[v.name] !== undefined)
      .map((v) => ({ variable: v.name, value: pending[v.name] ?? "" })),
    delete: (original?.variables ?? [])
      .filter((v) => v.secret && !draftSecrets.has(v.name))
      .map((v) => v.name),
  };
}

/** The environment as written to disk: secret values removed (PLAN.md §6). */
export function forDisk(env: EnvModel): EnvModel {
  return {
    ...env,
    variables: env.variables.map((v) => (v.secret ? { ...v, value: null } : v)),
  };
}
