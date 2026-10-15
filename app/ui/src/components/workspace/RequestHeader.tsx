import { ChevronRight, Layers, Save } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { folderTrail } from "@/lib/tree";
import { type EnvEntry, scopeOf, useEnvs } from "@/stores/envs";
import { ENVIRONMENTS_TAB, isDirty, type Tab, tabTitle, useTabs } from "@/stores/tabs";
import { useActiveCollection, useWorkspace } from "@/stores/workspace";

/**
 * The bar above the editor: where the open item lives (collection › folders › item), a Save
 * button while it has unsaved changes, and the environment selectors.
 */
export function RequestHeader({ dir, tab }: { dir: string; tab: Tab | null }) {
  const collection = useActiveCollection();
  const tree = useWorkspace((s) => s.trees[dir]);
  const save = useTabs((s) => s.save);
  const openTab = useTabs((s) => s.openTab);
  const trail =
    tab && (tab.kind === "request" || tab.kind === "folder") && tree
      ? folderTrail(tree, tab.path)
      : [];
  const dirty = tab !== null && isDirty(tab);

  return (
    <div className="flex h-10 shrink-0 items-center gap-1 border-b px-3 text-sm">
      <span className="truncate text-muted-foreground">{collection?.name}</span>
      {trail.map((name, i) => (
        <span key={i} className="flex items-center gap-1 text-muted-foreground">
          <ChevronRight className="size-3.5" />
          {name}
        </span>
      ))}
      {tab && (
        <span className="flex min-w-0 items-center gap-1">
          <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
          <span className="truncate font-medium" title={tab.path}>
            {tab.kind === "collection" ? "Settings" : tabTitle(tab)}
            {tab.kind === "folder" && " settings"}
          </span>
        </span>
      )}
      {dirty && (
        <Button size="sm" variant="ghost" className="h-7" onClick={() => void save(dir, tab.path)}>
          <Save /> Save
        </Button>
      )}
      <div className="flex-1" />
      <EnvSelect dir={dir} kind="global" />
      <EnvSelect dir={dir} kind="collection" />
      <Button
        size="icon"
        variant="ghost"
        className="size-8"
        title="Manage environments"
        aria-label="Manage environments"
        onClick={() => void openTab(dir, ENVIRONMENTS_TAB)}
      >
        <Layers />
      </Button>
    </div>
  );
}

const NONE = "__none__";
const NO_ENVS: EnvEntry[] = [];

/** Selects the active global or collection environment. */
function EnvSelect({ dir, kind }: { dir: string; kind: "global" | "collection" }) {
  const envs = useEnvs((s) => (kind === "global" ? s.global : s.collection)) ?? NO_ENVS;
  const active = useEnvs((s) => (kind === "global" ? s.activeGlobal : s.activeCollection));
  const select = useEnvs((s) => s.select);
  const label = kind === "global" ? "Global" : "Collection";

  return (
    <Select
      value={active ?? NONE}
      onValueChange={(value) => void select(scopeOf(kind, dir), value === NONE ? null : value)}
    >
      <SelectTrigger size="sm" className="h-7 max-w-44 text-xs" title={`${label} environment`}>
        <span className="text-muted-foreground">{label}:</span>
        <SelectValue />
      </SelectTrigger>
      <SelectContent align="end">
        <SelectItem value={NONE}>No environment</SelectItem>
        {envs.map((e) => (
          <SelectItem key={e.environment.name} value={e.environment.name}>
            {e.environment.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
