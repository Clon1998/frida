import { open as openFile } from "@tauri-apps/plugin-dialog";
import { useSubTab } from "@/stores/subTabs";
import { FolderOpen, Trash2 } from "lucide-react";
import { KeyValueTable } from "@/components/request/KeyValueTable";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { CollectionModel } from "@/lib/model";
import { type CollectionTab, useTabs } from "@/stores/tabs";
import { toast } from "sonner";
import { commands } from "@/bindings";
import { showError, unwrap } from "@/lib/api";
import { VariablesContext } from "@/components/editor/variables";
import { useInspection } from "@/components/inspector/useInspection";
import { ConflictBanner } from "./ConflictBanner";
import { CookiesSection } from "./CookiesSection";
import type { Auth } from "@/bindings";
import { AuthEditor } from "./AuthEditor";
import { ScriptsSection, TestsNote, TextSection, VarsSection } from "./sections";

/** Collection settings, opened as a tab. Saved in `collection.yaml`, shared through git. */
export function CollectionEditor({ dir, tab }: { dir: string; tab: CollectionTab }) {
  const [subTab, setSubTab] = useSubTab(`collection:${dir}`, "headers");
  const updateDraft = useTabs((s) => s.updateDraft);
  const collection = tab.draft;
  const update = (change: (c: CollectionModel) => CollectionModel) =>
    updateDraft<CollectionTab>(dir, tab.path, change);
  // Variables available in here, for highlighting and `{{` autocompletion in the fields below.
  const { variables } = useInspection(dir, "", null);

  const settings = collection.settings;
  const setTls = (tls: Partial<CollectionModel["settings"]["tls"]>) =>
    update((c) => ({ ...c, settings: { ...c.settings, tls: { ...c.settings.tls, ...tls } } }));

  const addCaCert = async () => {
    const path = await openFile({
      multiple: false,
      directory: false,
      title: "Choose a CA certificate (PEM)",
      filters: [{ name: "PEM certificate", extensions: ["pem", "crt", "cer"] }],
    });
    if (typeof path === "string") setTls({ caCertPaths: [...settings.tls.caCertPaths, path] });
  };

  return (
    <VariablesContext.Provider value={variables}>
      <div className="flex h-full flex-col">
        <ConflictBanner dir={dir} tab={tab} />
        <Tabs value={subTab} onValueChange={setSubTab} className="min-h-0 flex-1 gap-0">
          <TabsList className="mx-2 mt-2">
            <TabsTrigger value="headers">Headers</TabsTrigger>
            <TabsTrigger value="auth">Auth</TabsTrigger>
            <TabsTrigger value="vars">Vars</TabsTrigger>
            <TabsTrigger value="script">Script</TabsTrigger>
            <TabsTrigger value="tests">Tests</TabsTrigger>
            <TabsTrigger value="docs">Docs</TabsTrigger>
            <TabsTrigger value="settings">Settings</TabsTrigger>
            <TabsTrigger value="cookies">Cookies</TabsTrigger>
          </TabsList>
          <TabsContent value="headers" className="overflow-auto p-3">
            <p className="mb-2 text-xs text-muted-foreground">
              Sent with every request in the collection.
            </p>
            <KeyValueTable
              rows={collection.headers}
              namePlaceholder="Header"
              onChange={(headers) => update((c) => ({ ...c, headers }))}
            />
          </TabsContent>
          <TabsContent value="auth" className="overflow-auto p-3">
            <AuthEditor
              auth={collection.auth as Auth}
              canInherit={false}
              context={{ dir, path: "", request: null }}
              onChange={(auth) => update((c) => ({ ...c, auth: auth as CollectionModel["auth"] }))}
            />
          </TabsContent>
          <TabsContent value="vars" className="overflow-auto p-3">
            <VarsSection
              vars={collection.vars}
              onChange={(vars) => update((c) => ({ ...c, vars }))}
            />
            <Button
              variant="outline"
              size="sm"
              className="mt-4"
              title="Variables (fri.setVar) and in-memory environment changes (fri.setEnvVar) that scripts made during this session"
              onClick={() =>
                void unwrap(commands.clearRuntimeVars(dir)).then(
                  () => toast.success("Script variables cleared"),
                  showError,
                )
              }
            >
              Clear runtime variables
            </Button>
          </TabsContent>
          <TabsContent value="cookies" className="overflow-auto p-3">
            <CookiesSection dir={dir} />
          </TabsContent>
          <TabsContent value="script" className="min-h-0 p-3">
            <ScriptsSection
              scripts={collection.scripts}
              onChange={(scripts) => update((c) => ({ ...c, scripts }))}
            />
          </TabsContent>
          <TabsContent value="tests" className="min-h-0 p-3">
            <TextSection
              value={collection.tests}
              note={<TestsNote />}
              language="javascript"
              onChange={(tests) => update((c) => ({ ...c, tests }))}
            />
          </TabsContent>
          <TabsContent value="docs" className="min-h-0 p-3">
            <TextSection
              value={collection.docs}
              onChange={(docs) => update((c) => ({ ...c, docs }))}
            />
          </TabsContent>
          <TabsContent value="settings" className="overflow-auto p-3">
            <div className="flex max-w-xl flex-col gap-4 text-sm">
              <label className="flex flex-col gap-1.5">
                Name
                <Input
                  value={collection.name}
                  onChange={(e) => update((c) => ({ ...c, name: e.target.value }))}
                />
              </label>
              <label className="flex max-w-xs flex-col gap-1.5">
                Default timeout (ms)
                <Input
                  type="number"
                  min={1}
                  placeholder="30000"
                  value={settings.timeoutMs ?? ""}
                  onChange={(e) => {
                    const timeoutMs =
                      e.target.value === "" ? null : Math.max(1, Number(e.target.value));
                    update((c) => ({ ...c, settings: { ...c.settings, timeoutMs } }));
                  }}
                />
              </label>
              <label className="flex items-center gap-2">
                <Checkbox
                  checked={settings.tls.verify}
                  onCheckedChange={(checked) => setTls({ verify: checked === true })}
                />
                Verify TLS certificates
              </label>
              {!settings.tls.verify && (
                <p className="-mt-2 text-xs text-amber-600 dark:text-amber-400">
                  Any certificate is accepted, including invalid or self-signed ones. Only use this
                  for local development.
                </p>
              )}
              <div className="flex flex-col gap-1.5">
                <span>Extra CA certificates</span>
                <span className="text-xs text-muted-foreground">
                  Trusted in addition to the system certificates. Paths may be relative to the
                  collection folder.
                </span>
                {settings.tls.caCertPaths.map((path, index) => (
                  <div key={index} className="flex items-center gap-2">
                    <Input
                      className="h-8 font-mono text-xs"
                      value={path}
                      onChange={(e) =>
                        setTls({
                          caCertPaths: settings.tls.caCertPaths.map((p, i) =>
                            i === index ? e.target.value : p,
                          ),
                        })
                      }
                    />
                    <Button
                      variant="ghost"
                      size="icon"
                      className="size-8"
                      aria-label="Remove certificate"
                      onClick={() =>
                        setTls({
                          caCertPaths: settings.tls.caCertPaths.filter((_, i) => i !== index),
                        })
                      }
                    >
                      <Trash2 />
                    </Button>
                  </div>
                ))}
                <Button
                  variant="outline"
                  size="sm"
                  className="self-start"
                  onClick={() => void addCaCert()}
                >
                  <FolderOpen /> Add certificate…
                </Button>
              </div>
            </div>
          </TabsContent>
        </Tabs>
      </div>
    </VariablesContext.Provider>
  );
}
