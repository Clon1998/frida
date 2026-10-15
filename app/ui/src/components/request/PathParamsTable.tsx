import type { PathParam as RawPathParam } from "@/bindings";
import type { Complete } from "@/lib/model";

type PathParam = Complete<RawPathParam>;
import { Input } from "@/components/ui/input";

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
    <div className="flex flex-col gap-1">
      {params.map((param, index) => (
        <div key={param.name} className="flex items-center gap-2">
          <Input className="h-8 font-mono text-xs" value={param.name} readOnly tabIndex={-1} />
          <Input
            className="h-8 font-mono text-xs"
            placeholder="Value"
            value={param.value}
            onChange={(e) =>
              onChange(params.map((p, i) => (i === index ? { ...p, value: e.target.value } : p)))
            }
          />
        </div>
      ))}
    </div>
  );
}
