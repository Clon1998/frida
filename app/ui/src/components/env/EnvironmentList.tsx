import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Check, CircleOff, Copy } from "lucide-react";
import { commands, type EnvScope } from "@/bindings";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { showError, unwrap } from "@/lib/api";
import { envColor } from "@/lib/envColors";
import { type EnvEntry, envDraftKey, useEnvs } from "@/stores/envs";
import { cn } from "cn";

interface EnvironmentListProps {
  dir: string;
  scope: EnvScope;
  entries: EnvEntry[];
  selected: string | null;
  onSelect: (name: string) => void;
  onDuplicate: (name: string) => void;
}

/**
 * The environments of one scope in the editor's sidebar. Drag an entry to reorder: the order is
 * saved as `seq` in the environment files and used everywhere (also the environment picker).
 */
export function EnvironmentList({
  dir,
  scope,
  entries,
  selected,
  onSelect,
  onDuplicate,
}: EnvironmentListProps) {
  const sensors = useSensors(
    // A few pixels of movement before dragging starts, so a click still selects.
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const names = entries.map((e) => e.environment.name);

  const move = async (from: number, to: number) => {
    const moved = arrayMove(entries, from, to);
    // Show the new order right away, so the entry doesn't jump back until the reload.
    useEnvs.setState(scope.kind === "global" ? { global: moved } : { collection: moved });
    try {
      await unwrap(
        commands.reorderEnvironments(
          scope,
          moved.map((e) => e.environment.name),
        ),
      );
    } catch (e) {
      showError(e);
    }
    await useEnvs.getState().load(dir);
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragEnd={({ active, over }) => {
        if (over && active.id !== over.id)
          void move(names.indexOf(String(active.id)), names.indexOf(String(over.id)));
      }}
    >
      <SortableContext items={names} strategy={verticalListSortingStrategy}>
        {entries.map((e) => (
          <SortableEnvironment
            key={e.environment.name}
            entry={e}
            scope={scope}
            selected={selected === e.environment.name}
            onSelect={() => onSelect(e.environment.name)}
            onDuplicate={() => onDuplicate(e.environment.name)}
          />
        ))}
      </SortableContext>
    </DndContext>
  );
}

function SortableEnvironment({
  entry,
  scope,
  selected,
  onSelect,
  onDuplicate,
}: {
  entry: EnvEntry;
  scope: EnvScope;
  selected: boolean;
  onSelect: () => void;
  onDuplicate: () => void;
}) {
  const env = entry.environment;
  const unsaved = useEnvs((s) => s.editorDrafts[envDraftKey(scope, env.name)] !== undefined);
  const active = useEnvs(
    (s) => (scope.kind === "global" ? s.activeGlobal : s.activeCollection) === env.name,
  );
  const setActive = useEnvs((s) => s.select);
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: env.name,
  });
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <button
          type="button"
          ref={setNodeRef}
          style={{ transform: CSS.Translate.toString(transform), transition }}
          onClick={onSelect}
          title={active ? "Active environment. Drag to reorder" : "Drag to reorder"}
          className={cn(
            "truncate rounded px-2 py-1.5 text-left text-sm hover:bg-accent",
            selected && "bg-accent font-medium",
            isDragging && "relative z-10 bg-background shadow-md",
          )}
          {...attributes}
          {...listeners}
        >
          <span className="flex items-center gap-2">
            <span
              className={cn(
                "size-2 shrink-0 rounded-full",
                envColor(env.color)?.dot ?? "border border-muted-foreground/60",
              )}
            />
            <span className="truncate">{env.name}</span>
            <span className="ml-auto flex shrink-0 items-center gap-1.5">
              {unsaved && (
                <span className="size-1.5 rounded-full bg-foreground" title="Unsaved changes" />
              )}
              {active && (
                <span
                  className="flex items-center gap-0.5 text-[10px] font-medium text-success"
                  aria-label="Active"
                >
                  <Check className="size-3" /> active
                </span>
              )}
            </span>
          </span>
        </button>
      </ContextMenuTrigger>
      <ContextMenuContent>
        {active ? (
          <ContextMenuItem onSelect={() => void setActive(scope, null)}>
            <CircleOff /> Deactivate
          </ContextMenuItem>
        ) : (
          <ContextMenuItem onSelect={() => void setActive(scope, env.name)}>
            <Check /> Set as active
          </ContextMenuItem>
        )}
        <ContextMenuItem onSelect={onDuplicate}>
          <Copy /> Duplicate
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
