import { arrayMove } from "@dnd-kit/sortable";
import { Trash2 } from "lucide-react";
import { useState } from "react";
import type { KeyValue as RawKeyValue } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Textarea } from "@/components/ui/textarea";
import { bulkToRows, rowsToBulk } from "@/lib/bulk";
import type { Complete } from "@/lib/model";
import { CellInput, GridCell, GridRow, GridTable, VarCellInput } from "./GridTable";

type KeyValue = Complete<RawKeyValue>;

interface KeyValueTableProps {
  rows: KeyValue[];
  onChange: (rows: KeyValue[]) => void;
  namePlaceholder?: string;
  valuePlaceholder?: string;
}

/**
 * Editable `name | value` rows with an enabled checkbox, as a table. Typing into the empty last
 * row adds a new row. "Bulk edit" switches to a text area with one `name: value` per line.
 */
export function KeyValueTable({
  rows,
  onChange,
  namePlaceholder = "Name",
  valuePlaceholder = "Value",
}: KeyValueTableProps) {
  // While bulk editing, the text is kept as typed (parsing on every key press would otherwise
  // reformat lines under the cursor). `null` means table mode.
  const [bulkText, setBulkText] = useState<string | null>(null);

  const update = (index: number, patch: Partial<KeyValue>) => {
    if (index === rows.length) {
      onChange([...rows, { name: "", value: "", enabled: true, ...patch }]);
    } else {
      onChange(rows.map((row, i) => (i === index ? { ...row, ...patch } : row)));
    }
  };
  const remove = (index: number) => onChange(rows.filter((_, i) => i !== index));

  // One extra, empty row at the end for adding.
  const shown = [...rows, { name: "", value: "", enabled: true }];

  return (
    <div className="flex flex-col gap-1">
      <div className="flex justify-end">
        <Button
          variant="link"
          size="sm"
          className="h-6 px-0 text-xs"
          onClick={() => setBulkText(bulkText === null ? rowsToBulk(rows) : null)}
        >
          {bulkText === null ? "Bulk edit" : "Table edit"}
        </Button>
      </div>

      {bulkText !== null ? (
        <Textarea
          className="min-h-40 font-mono text-xs"
          spellCheck={false}
          placeholder={"name: value\n//disabled: value"}
          value={bulkText}
          onChange={(e) => {
            setBulkText(e.target.value);
            onChange(bulkToRows(e.target.value));
          }}
        />
      ) : (
        <GridTable
          reorder={{
            count: rows.length,
            onMove: (from, to) => onChange(arrayMove(rows, from, to)),
          }}
          columns={[
            { label: "", width: "2.25rem" },
            { label: namePlaceholder, width: "35%" },
            { label: valuePlaceholder },
            { label: "", width: "2.25rem" },
          ]}
        >
          {shown.map((row, index) => {
            const isNew = index === rows.length;
            return (
              <GridRow key={index} muted={!row.enabled} sortIndex={isNew ? undefined : index}>
                <GridCell center>
                  <Checkbox
                    className="align-middle"
                    checked={row.enabled}
                    disabled={isNew}
                    onCheckedChange={(checked) => update(index, { enabled: checked === true })}
                    aria-label="Enabled"
                  />
                </GridCell>
                <GridCell>
                  <CellInput
                    placeholder={isNew ? `Add ${namePlaceholder.toLowerCase()}` : namePlaceholder}
                    value={row.name}
                    onChange={(e) => update(index, { name: e.target.value })}
                  />
                </GridCell>
                <GridCell>
                  <VarCellInput
                    placeholder={valuePlaceholder}
                    value={row.value}
                    onChange={(value) => update(index, { value })}
                  />
                </GridCell>
                <GridCell center>
                  {!isNew && (
                    <Button
                      variant="ghost"
                      size="icon"
                      className="size-7"
                      onClick={() => remove(index)}
                      aria-label="Remove row"
                    >
                      <Trash2 />
                    </Button>
                  )}
                </GridCell>
              </GridRow>
            );
          })}
        </GridTable>
      )}
    </div>
  );
}
