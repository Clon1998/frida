import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Trash2 } from "lucide-react";
import type { MultipartField as RawMultipartField, MultipartKind } from "@/bindings";
import type { Complete } from "@/lib/model";
import { CellInput, GridCell, GridRow, GridTable } from "./GridTable";

type MultipartField = Complete<RawMultipartField>;
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
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
    <GridTable
      columns={[
        { label: "", width: "2.25rem" },
        { label: "Name", width: "30%" },
        { label: "Type", width: "6.5rem" },
        { label: "Value" },
        { label: "", width: "4.5rem" },
      ]}
    >
      {shown.map((field, index) => {
        const isNew = index === fields.length;
        return (
          <GridRow key={index} muted={!field.enabled}>
            <GridCell center>
              <Checkbox
                className="align-middle"
                checked={field.enabled}
                disabled={isNew}
                onCheckedChange={(checked) => update(index, { enabled: checked === true })}
                aria-label="Enabled"
              />
            </GridCell>
            <GridCell>
              <CellInput
                placeholder={isNew ? "Add field" : "Name"}
                value={field.name}
                onChange={(e) => update(index, { name: e.target.value })}
              />
            </GridCell>
            <GridCell>
              <Select
                value={field.type}
                onValueChange={(type) => update(index, { type: type as MultipartKind })}
              >
                <SelectTrigger size="sm" className="h-8 w-full rounded-none border-0 shadow-none">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="text">Text</SelectItem>
                  <SelectItem value="file">File</SelectItem>
                </SelectContent>
              </Select>
            </GridCell>
            <GridCell>
              <CellInput
                placeholder={field.type === "file" ? "Path to file" : "Value"}
                value={field.value}
                onChange={(e) => update(index, { value: e.target.value })}
              />
            </GridCell>
            <GridCell center>
              {field.type === "file" && (
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-7"
                  onClick={() => void pickFile(index)}
                  aria-label="Choose file"
                >
                  <FolderOpen />
                </Button>
              )}
              {!isNew && (
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-7"
                  onClick={() => onChange(fields.filter((_, i) => i !== index))}
                  aria-label="Remove field"
                >
                  <Trash2 />
                </Button>
              )}
            </GridCell>
          </GridRow>
        );
      })}
    </GridTable>
  );
}
