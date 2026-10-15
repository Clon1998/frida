import { ChevronDown, Layers, Settings } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { scopeOf, useEnvs } from "@/stores/envs";
import { ENVIRONMENTS_TAB, useTabs } from "@/stores/tabs";

const NONE = "__none__";

/**
 * One button for both environments, like Bruno: it shows what's active, and its menu switches
 * the collection and the global environment and opens the editor for either.
 */
export function EnvironmentPicker({ dir }: { dir: string }) {
  const { global, collection, activeGlobal, activeCollection, select, setEditorScope } = useEnvs();
  const openTab = useTabs((s) => s.openTab);

  const label =
    activeCollection && activeGlobal
      ? `${activeCollection} · ${activeGlobal}`
      : (activeCollection ?? activeGlobal ?? "No environment");

  const configure = (scope: "collection" | "global") => {
    setEditorScope(scope);
    void openTab(dir, ENVIRONMENTS_TAB);
  };

  const section = (kind: "collection" | "global") => {
    const entries = kind === "collection" ? collection : global;
    const active = kind === "collection" ? activeCollection : activeGlobal;
    return (
      <>
        <DropdownMenuLabel className="flex items-center justify-between text-xs text-muted-foreground">
          {kind === "collection" ? "Collection" : "Global"}
        </DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={active ?? NONE}
          onValueChange={(value) => void select(scopeOf(kind, dir), value === NONE ? null : value)}
        >
          <DropdownMenuRadioItem value={NONE}>No environment</DropdownMenuRadioItem>
          {entries.map((e) => (
            <DropdownMenuRadioItem key={e.environment.name} value={e.environment.name}>
              {e.environment.name}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        <DropdownMenuItem className="text-muted-foreground" onSelect={() => configure(kind)}>
          <Settings /> Configure {kind === "collection" ? "collection" : "global"} environments…
        </DropdownMenuItem>
      </>
    );
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          className="h-7 max-w-64 gap-1.5 text-xs"
          title="Environments"
        >
          <Layers className="size-3.5" />
          <span className="truncate">{label}</span>
          <ChevronDown className="size-3.5 text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-64">
        {section("collection")}
        <DropdownMenuSeparator />
        {section("global")}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
