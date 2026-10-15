import { useState } from "react";
import { cn } from "cn";

interface EditableCrumbProps {
  value: string;
  onRename: (name: string) => void;
  className?: string;
  title?: string;
}

/**
 * A breadcrumb name that turns into an input when clicked. Enter or leaving the field renames;
 * Escape cancels. Unchanged or empty names are ignored.
 */
export function EditableCrumb({ value, onRename, className, title }: EditableCrumbProps) {
  const [editing, setEditing] = useState<string | null>(null);

  if (editing === null) {
    return (
      <button
        type="button"
        title={title ?? "Click to rename"}
        onClick={() => setEditing(value)}
        className={cn("truncate rounded px-1 hover:bg-accent", className)}
      >
        {value}
      </button>
    );
  }

  const commit = () => {
    const name = editing.trim();
    setEditing(null);
    if (name && name !== value) onRename(name);
  };

  return (
    <input
      autoFocus
      value={editing}
      size={Math.max(editing.length, 4)}
      onFocus={(e) => e.target.select()}
      onChange={(e) => setEditing(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") setEditing(null);
      }}
      className={cn(
        "rounded border bg-background px-1 outline-none focus:ring-2 focus:ring-ring/50",
        className,
      )}
    />
  );
}
