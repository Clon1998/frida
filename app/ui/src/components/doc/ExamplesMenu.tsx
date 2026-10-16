import { BookOpen } from "lucide-react";
import { useEffect, useState } from "react";
import { commands, type Snippet, type SnippetKind } from "@/bindings";
import { Button } from "@/components/ui/button";
import { appendSnippet } from "@/lib/snippets";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

/** Loaded once per app start; the list is fixed in the Rust code. */
let loading: Promise<Snippet[]> | null = null;

function useSnippets(kind: SnippetKind) {
  const [all, setAll] = useState<Snippet[]>([]);
  useEffect(() => {
    let cancelled = false;
    loading ??= commands.listSnippets();
    void loading.then(
      (list) => !cancelled && setAll(list),
      () => {
        loading = null; // try again next time
      },
    );
    return () => {
      cancelled = true;
    };
  }, []);
  return all.filter((s) => s.kind === kind);
}

const LABELS: Record<SnippetKind, string> = {
  preRequest: "Pre-request examples",
  postResponse: "Post-response examples",
  tests: "Test examples",
};

/**
 * "Examples": ready-made scripts or tests (`frida_script::snippets`, each one checked against a
 * test server). Picking one adds it to the end of the editor's text.
 */
export function ExamplesMenu({
  kind,
  value,
  onChange,
}: {
  kind: SnippetKind;
  value: string;
  onChange: (value: string) => void;
}) {
  const snippets = useSnippets(kind);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 gap-1 px-1.5 text-xs text-muted-foreground"
          aria-label={LABELS[kind]}
          disabled={snippets.length === 0}
        >
          <BookOpen className="size-3.5" /> Examples
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-96 w-80 overflow-auto">
        <DropdownMenuLabel className="text-xs text-muted-foreground">
          {LABELS[kind]}: added at the end
        </DropdownMenuLabel>
        {snippets.map((s) => (
          <DropdownMenuItem
            key={s.id}
            className="flex-col items-start gap-0.5"
            onSelect={() => onChange(appendSnippet(value, s.code))}
          >
            <span>{s.title}</span>
            <span className="text-xs text-muted-foreground">{s.description}</span>
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
