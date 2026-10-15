import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Trash2 } from "lucide-react";
import type { MultipartField as RawMultipartField, MultipartKind } from "@/bindings";
import type { Complete } from "@/lib/model";

type MultipartField = Complete<RawMultipartField>;
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

interface MultipartTableProps {
  fields: MultipartField[];
  onChange: (fields: MultipartField[]) => void;
}

/** Multipart form fields: text values or files (stored as a path). */
export function MultipartTable({ fields, onChange }: MultipartTableProps) {
  const update = (index: number, patch: Partial<MultipartField>) => {
    if (index === fields.length) {
      onChange([...fields, { name: "", type: "text", value: "", enabled: true, ...patch }]);
    } else {
      onChange(fields.map((f, i) => (i === index ? { ...f, ...patch } : f)));
    }
  };
  const pickFile = async (index: number) => {
    const path = await open({ multiple: false, directory: false });
    if (typeof path === "string") update(index, { value: path, type: "file" });
  };

  const shown: MultipartField[] = [...fields, { name: "", type: "text", value: "", enabled: true }];

  return (
    <div className="flex flex-col gap-1">
      {shown.map((field, index) => {
        const isNew = index === fields.length;
        return (
          <div key={index} className="flex items-center gap-2">
            <Checkbox
              checked={field.enabled}
              disabled={isNew}
              onCheckedChange={(checked) => update(index, { enabled: checked === true })}
              aria-label="Enabled"
            />
            <Input
              className="h-8 font-mono text-xs"
              placeholder="Name"
              value={field.name}
              onChange={(e) => update(index, { name: e.target.value })}
            />
            <Select
              value={field.type}
              onValueChange={(type) => update(index, { type: type as MultipartKind })}
            >
              <SelectTrigger size="sm" className="w-24 shrink-0">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="text">Text</SelectItem>
                <SelectItem value="file">File</SelectItem>
              </SelectContent>
            </Select>
            <Input
              className="h-8 font-mono text-xs"
              placeholder={field.type === "file" ? "Path to file" : "Value"}
              value={field.value}
              onChange={(e) => update(index, { value: e.target.value })}
            />
            {field.type === "file" && (
              <Button
                variant="ghost"
                size="icon"
                className="size-8 shrink-0"
                onClick={() => void pickFile(index)}
                aria-label="Choose file"
              >
                <FolderOpen />
              </Button>
            )}
            <Button
              variant="ghost"
              size="icon"
              className="size-8 shrink-0"
              disabled={isNew}
              onClick={() => onChange(fields.filter((_, i) => i !== index))}
              aria-label="Remove field"
            >
              <Trash2 />
            </Button>
          </div>
        );
      })}
    </div>
  );
}
