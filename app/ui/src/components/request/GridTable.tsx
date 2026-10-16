import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { GripVertical } from "lucide-react";
import { createContext, type InputHTMLAttributes, type ReactNode, useContext } from "react";
import { CodeEditor } from "@/components/editor/CodeEditor";
import { VariablesContext } from "@/components/editor/variables";
import { cn } from "cn";

export interface Column {
  label: ReactNode;
  /** CSS width, e.g. "2rem" or "40%". Columns without one share the remaining space. */
  width?: string;
}

/** The row ids of the surrounding reorderable table (`null`: not reorderable). */
const SortableIds = createContext<string[] | null>(null);

/** Makes a {@link GridTable}'s rows draggable. */
export interface Reorder {
  /**
   * One stable id per draggable row (the trailing "add" row has none), from `useRowIds`.
   * Use the same ids as the rows' React keys.
   */
  ids: string[];
  /** Move the row at `from` so it ends up at `to`. */
  onMove: (from: number, to: number) => void;
}

/**
 * A bordered table for editable rows, like Bruno and Postman: a header row, then one row per
 * entry with borderless inputs in the cells. With `reorder`, rows get a drag handle (and can be
 * moved with the keyboard: focus the handle, Space, arrow keys, Space).
 */
export function GridTable({
  columns,
  children,
  reorder,
}: {
  columns: Column[];
  children: ReactNode;
  reorder?: Reorder;
}) {
  const sensors = useSensors(
    // A few pixels of movement before dragging starts, so clicks on the handle still work.
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const shownColumns = reorder ? [{ label: "", width: "1.25rem" }, ...columns] : columns;
  const body = <tbody>{children}</tbody>;
  return (
    <div className="overflow-hidden rounded-md border">
      <table className="w-full table-fixed border-collapse text-xs">
        <colgroup>
          {shownColumns.map((c, i) => (
            <col key={i} style={c.width ? { width: c.width } : undefined} />
          ))}
        </colgroup>
        <thead className="bg-muted/50">
          <tr>
            {shownColumns.map((c, i) => (
              <th
                key={i}
                className="border-r border-b px-2 py-1.5 text-left font-medium text-muted-foreground last:border-r-0"
              >
                {c.label}
              </th>
            ))}
          </tr>
        </thead>
        {reorder ? (
          <DndContext
            sensors={sensors}
            collisionDetection={closestCenter}
            onDragEnd={({ active, over }) => {
              if (over && active.id !== over.id)
                reorder.onMove(
                  reorder.ids.indexOf(String(active.id)),
                  reorder.ids.indexOf(String(over.id)),
                );
            }}
          >
            <SortableContext items={reorder.ids} strategy={verticalListSortingStrategy}>
              <SortableIds.Provider value={reorder.ids}>{body}</SortableIds.Provider>
            </SortableContext>
          </DndContext>
        ) : (
          body
        )}
      </table>
    </div>
  );
}

/**
 * One row. In a reorderable table, rows with a `sortIndex` get a drag handle; rows without one
 * (the trailing "add" row) get an empty handle cell and stay where they are.
 */
export function GridRow({
  children,
  muted = false,
  sortIndex,
}: {
  children: ReactNode;
  muted?: boolean;
  sortIndex?: number;
}) {
  const ids = useContext(SortableIds);
  const className = cn("border-b bg-background last:border-b-0", muted && "text-muted-foreground");
  if (!ids) return <tr className={className}>{children}</tr>;
  const id = sortIndex === undefined ? undefined : ids[sortIndex];
  if (id === undefined) {
    return (
      <tr className={className}>
        <td className="border-r" />
        {children}
      </tr>
    );
  }
  return (
    <SortableRow id={id} className={className}>
      {children}
    </SortableRow>
  );
}

function SortableRow({
  id,
  className,
  children,
}: {
  id: string;
  className: string;
  children: ReactNode;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    setActivatorNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id });
  return (
    <tr
      ref={setNodeRef}
      className={cn(className, isDragging && "relative z-10 shadow-md")}
      style={{ transform: CSS.Translate.toString(transform), transition }}
    >
      <td className="border-r p-0 text-center align-middle">
        <button
          type="button"
          ref={setActivatorNodeRef}
          className="flex h-8 w-full cursor-grab items-center justify-center text-muted-foreground/60 hover:text-foreground active:cursor-grabbing"
          aria-label="Drag to reorder"
          title="Drag to reorder"
          {...attributes}
          {...listeners}
        >
          <GripVertical className="size-3.5" />
        </button>
      </td>
      {children}
    </tr>
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

/**
 * A cell input for values that may contain `{{variables}}`. Inside the request editor (which
 * provides variable info) it is a one-line code editor with highlighting and `{{`
 * autocompletion; elsewhere a plain input.
 */
export function VarCellInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}) {
  const variables = useContext(VariablesContext);
  if (!variables) {
    return (
      <CellInput
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    );
  }
  return (
    <CodeEditor
      singleLine
      compact
      className="w-full font-mono focus-within:bg-accent/40"
      value={value}
      onChange={onChange}
      placeholder={placeholder}
    />
  );
}
