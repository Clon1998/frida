import { commands } from "@/bindings";
import { showError, unwrap } from "@/lib/api";
import { useDialogs } from "./dialogs";
import { type DiffSelection, useGit } from "./git";
import { GIT_TAB, isDirty, useTabs } from "./tabs";
import { useWorkspace } from "./workspace";

/**
 * Sidebar actions that touch both stores: they call a command, reload the tree and keep open
 * tabs pointing at the right files after renames, moves and deletes.
 */

const reload = (dir: string) => useWorkspace.getState().reloadTree(dir);
const tabs = () => useTabs.getState();

async function run<T>(action: () => Promise<T>): Promise<T | undefined> {
  try {
    return await action();
  } catch (e) {
    showError(e);
    return undefined;
  }
}

export const createRequest = (dir: string, parent: string, name: string) =>
  run(async () => {
    const path = await unwrap(commands.createRequest(dir, parent, name));
    if (parent) tabs().toggleExpanded(dir, parent, true);
    await reload(dir);
    await tabs().openTab(dir, path);
  });

export const createFolder = (dir: string, parent: string, name: string) =>
  run(async () => {
    const path = await unwrap(commands.createFolder(dir, parent, name));
    if (parent) tabs().toggleExpanded(dir, parent, true);
    tabs().toggleExpanded(dir, path, true);
    await reload(dir);
  });

export const renameItem = (dir: string, path: string, name: string) =>
  run(async () => {
    const newPath = await unwrap(commands.renameItem(dir, path, name));
    tabs().renamePaths(dir, path, newPath);
    // The name lives in the file; update the open tab so it doesn't look like an external edit.
    tabs().applyRename(dir, newPath, name.trim());
    await reload(dir);
  });

/** Renames the collection (its `name` in collection.yaml; the folder stays the same). */
export const renameCollection = (dir: string, name: string) =>
  run(async () => {
    const collection = await unwrap(commands.readCollection(dir));
    await unwrap(commands.saveCollection(dir, { ...collection, name: name.trim() }));
    tabs().applyRename(dir, "", name.trim());
    await useWorkspace.getState().reloadWorkspace();
  });

export const duplicateRequest = (dir: string, path: string) =>
  run(async () => {
    const copy = await unwrap(commands.duplicateRequest(dir, path));
    await reload(dir);
    await tabs().openTab(dir, copy);
  });

export const deleteItem = (dir: string, path: string) =>
  run(async () => {
    await unwrap(commands.deleteItem(dir, path));
    tabs().removePaths(dir, path);
    await reload(dir);
  });

export const moveItem = (dir: string, path: string, parent: string, index: number) =>
  run(async () => {
    const newPath = await unwrap(commands.moveItem(dir, path, parent, index));
    tabs().renamePaths(dir, path, newPath);
    if (parent) tabs().toggleExpanded(dir, parent, true);
    await reload(dir);
  });

export const revealItem = (dir: string, path: string) =>
  run(() => unwrap(commands.revealItem(dir, path)));

/**
 * Closes the tabs at `paths`. If any of them has unsaved changes, asks first (save, don't save
 * or cancel) in the app-wide CloseTabsDialog.
 */
export function closeTabsAsking(dir: string, paths: string[]) {
  const open = tabs().byCollection[dir]?.tabs ?? [];
  if (open.some((t) => paths.includes(t.path) && isDirty(t)))
    useDialogs.getState().openCloseTabs(dir, paths);
  else tabs().closeTabs(dir, paths);
}

/** Opens the changes tab of `dir`, showing `selection` if given (else what it showed last). */
export function openChanges(dir: string, selection?: DiffSelection) {
  if (selection) useGit.getState().select(dir, selection);
  void tabs().openTab(dir, GIT_TAB);
}
