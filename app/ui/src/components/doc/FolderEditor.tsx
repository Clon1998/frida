import { KeyValueTable } from "@/components/request/KeyValueTable";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { FolderModel } from "@/lib/model";
import { type FolderTab, useTabs } from "@/stores/tabs";
import { ConflictBanner } from "./ConflictBanner";
import type { Auth } from "@/bindings";
import { AuthEditor } from "./AuthEditor";
import { ScriptsSection, TestsNote, TextSection, VarsSection } from "./sections";

/** Folder settings, opened as a tab: inherited by every request in the folder. */
export function FolderEditor({ dir, tab }: { dir: string; tab: FolderTab }) {
  const updateDraft = useTabs((s) => s.updateDraft);
  const folder = tab.draft;
  const update = (change: (f: FolderModel) => FolderModel) =>
    updateDraft<FolderTab>(dir, tab.path, change);

  return (
    <div className="flex h-full flex-col">
      <ConflictBanner dir={dir} tab={tab} />
      <p className="px-3 pt-3 text-xs text-muted-foreground">
        Settings for every request in this folder and its subfolders. Requests can override them.
      </p>
      <Tabs defaultValue="headers" className="min-h-0 flex-1 gap-0">
        <TabsList className="mx-2 mt-2">
          <TabsTrigger value="headers">Headers</TabsTrigger>
          <TabsTrigger value="auth">Auth</TabsTrigger>
          <TabsTrigger value="vars">Vars</TabsTrigger>
          <TabsTrigger value="script">Script</TabsTrigger>
          <TabsTrigger value="tests">Tests</TabsTrigger>
          <TabsTrigger value="docs">Docs</TabsTrigger>
          <TabsTrigger value="settings">Settings</TabsTrigger>
        </TabsList>
        <TabsContent value="headers" className="overflow-auto p-3">
          <KeyValueTable
            rows={folder.headers}
            namePlaceholder="Header"
            onChange={(headers) => update((f) => ({ ...f, headers }))}
          />
        </TabsContent>
        <TabsContent value="auth" className="overflow-auto p-3">
          <AuthEditor
            auth={folder.auth as Auth}
            canInherit
            context={{ dir, path: tab.path, request: null }}
            onChange={(auth) => update((f) => ({ ...f, auth: auth as FolderModel["auth"] }))}
          />
        </TabsContent>
        <TabsContent value="vars" className="overflow-auto p-3">
          <VarsSection vars={folder.vars} onChange={(vars) => update((f) => ({ ...f, vars }))} />
        </TabsContent>
        <TabsContent value="script" className="min-h-0 p-3">
          <ScriptsSection
            scripts={folder.scripts}
            onChange={(scripts) => update((f) => ({ ...f, scripts }))}
          />
        </TabsContent>
        <TabsContent value="tests" className="min-h-0 p-3">
          <TextSection
            value={folder.tests}
            note={<TestsNote />}
            language="javascript"
            onChange={(tests) => update((f) => ({ ...f, tests }))}
          />
        </TabsContent>
        <TabsContent value="docs" className="min-h-0 p-3">
          <TextSection value={folder.docs} onChange={(docs) => update((f) => ({ ...f, docs }))} />
        </TabsContent>
        <TabsContent value="settings" className="overflow-auto p-3">
          <label className="flex max-w-xs flex-col gap-1.5 text-sm">
            Timeout (ms)
            <Input
              type="number"
              min={1}
              placeholder="Inherit from parent"
              value={folder.settings.timeoutMs ?? ""}
              onChange={(e) => {
                const timeoutMs =
                  e.target.value === "" ? null : Math.max(1, Number(e.target.value));
                update((f) => ({ ...f, settings: { timeoutMs } }));
              }}
            />
          </label>
        </TabsContent>
      </Tabs>
    </div>
  );
}
