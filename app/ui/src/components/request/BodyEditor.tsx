import { open } from "@tauri-apps/plugin-dialog";
import type { Body as RawBody, BodyMode } from "@/bindings";
import { CodeEditor } from "@/components/editor/CodeEditor";
import type { VariableMap } from "@/components/editor/variables";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { Complete } from "@/lib/model";
import { KeyValueTable } from "./KeyValueTable";

type Body = Complete<RawBody>;
import { MultipartTable } from "./MultipartTable";

const MODES: { value: BodyMode; label: string }[] = [
  { value: "none", label: "No body" },
  { value: "json", label: "JSON" },
  { value: "xml", label: "XML" },
  { value: "text", label: "Text" },
  { value: "form-urlencoded", label: "Form URL-encoded" },
  { value: "multipart", label: "Multipart form" },
  { value: "binary", label: "Binary file" },
];

interface BodyEditorProps {
  body: Body;
  onChange: (body: Body) => void;
  /** For `{{var}}` highlighting in the text editors. */
  variables?: VariableMap;
}

export function BodyEditor({ body, onChange, variables }: BodyEditorProps) {
  const patch = (p: Partial<Body>) => onChange({ ...body, ...p });

  const pickBinary = async () => {
    const path = await open({ multiple: false, directory: false });
    if (typeof path === "string") patch({ binaryPath: path });
  };

  return (
    <div className="flex h-full flex-col gap-2">
      <Select value={body.mode} onValueChange={(mode) => patch({ mode: mode as BodyMode })}>
        <SelectTrigger size="sm" className="w-48">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {MODES.map((m) => (
            <SelectItem key={m.value} value={m.value}>
              {m.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <div className="min-h-0 flex-1">
        {body.mode === "none" && (
          <p className="text-xs text-muted-foreground">This request has no body.</p>
        )}
        {body.mode === "json" && (
          <CodeEditor
            className="h-full overflow-hidden rounded-md border"
            language="json"
            variables={variables}
            value={body.json}
            onChange={(json) => patch({ json })}
          />
        )}
        {body.mode === "xml" && (
          <CodeEditor
            className="h-full overflow-hidden rounded-md border"
            language="xml"
            variables={variables}
            value={body.xml}
            onChange={(xml) => patch({ xml })}
          />
        )}
        {body.mode === "text" && (
          <CodeEditor
            className="h-full overflow-hidden rounded-md border"
            value={body.text}
            variables={variables}
            onChange={(text) => patch({ text })}
          />
        )}
        {body.mode === "form-urlencoded" && (
          <KeyValueTable
            rows={body.formUrlEncoded}
            onChange={(formUrlEncoded) => patch({ formUrlEncoded })}
          />
        )}
        {body.mode === "multipart" && (
          <MultipartTable fields={body.multipart} onChange={(multipart) => patch({ multipart })} />
        )}
        {body.mode === "binary" && (
          <div className="flex items-center gap-2">
            <Input
              className="h-8 font-mono text-xs"
              placeholder="Path to file (relative to the collection, or absolute)"
              value={body.binaryPath}
              onChange={(e) => patch({ binaryPath: e.target.value })}
            />
            <Button variant="outline" size="sm" onClick={() => void pickBinary()}>
              Choose…
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
