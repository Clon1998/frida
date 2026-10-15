import type { PathParam as RawPathParam } from "@/bindings";
import type { Complete } from "@/lib/model";
import { CellInput, GridCell, GridRow, GridTable } from "./GridTable";

type PathParam = Complete<RawPathParam>;

interface PathParamsTableProps {
  params: PathParam[];
  onChange: (params: PathParam[]) => void;
}

/** Values for the `:name` segments of the URL. Names come from the URL and can't be edited here. */
export function PathParamsTable({ params, onChange }: PathParamsTableProps) {
  if (params.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        Add <code>:name</code> segments to the URL path to define path params.
      </p>
    );
  }
  return (
    <GridTable columns={[{ label: "Name", width: "35%" }, { label: "Value" }]}>
      {params.map((param, index) => (
        <GridRow key={param.name}>
          <GridCell>
            <CellInput
              value={param.name}
              readOnly
              tabIndex={-1}
              className="text-muted-foreground"
            />
          </GridCell>
          <GridCell>
            <CellInput
              placeholder="Value"
              value={param.value}
              onChange={(e) =>
                onChange(params.map((p, i) => (i === index ? { ...p, value: e.target.value } : p)))
              }
            />
          </GridCell>
        </GridRow>
      ))}
    </GridTable>
  );
}
