import type { InputHTMLAttributes, ReactNode } from "react";
import { cn } from "cn";

export interface Column {
  label: ReactNode;
  /** CSS width, e.g. "2rem" or "40%". Columns without one share the remaining space. */
  width?: string;
}

/**
 * A bordered table for editable rows, like Bruno and Postman: a header row, then one row per
 * entry with borderless inputs in the cells.
 */
export function GridTable({ columns, children }: { columns: Column[]; children: ReactNode }) {
  return (
    <div className="overflow-hidden rounded-md border">
      <table className="w-full table-fixed border-collapse text-xs">
        <colgroup>
          {columns.map((c, i) => (
            <col key={i} style={c.width ? { width: c.width } : undefined} />
          ))}
        </colgroup>
        <thead className="bg-muted/50">
          <tr>
            {columns.map((c, i) => (
              <th
                key={i}
                className="border-r border-b px-2 py-1.5 text-left font-medium text-muted-foreground last:border-r-0"
              >
                {c.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>{children}</tbody>
      </table>
    </div>
  );
}

export function GridRow({ children, muted = false }: { children: ReactNode; muted?: boolean }) {
  return (
    <tr className={cn("border-b last:border-b-0", muted && "text-muted-foreground")}>{children}</tr>
  );
}

export function GridCell({ children, center = false }: { children: ReactNode; center?: boolean }) {
  return (
    <td className={cn("h-8 border-r p-0 align-middle last:border-r-0", center && "text-center")}>
      {children}
    </td>
  );
}

/** A borderless input that fills its cell. */
export function CellInput({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      spellCheck={false}
      className={cn(
        "h-8 w-full bg-transparent px-2 font-mono outline-none placeholder:text-muted-foreground/50 focus:bg-accent/40 disabled:cursor-default",
        className,
      )}
      {...props}
    />
  );
}
