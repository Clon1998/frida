import {
  DndContext,
  type DragEndEvent,
  type DragMoveEvent,
  PointerSensor,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import { AlertTriangle, ChevronRight, Folder, FolderOpen } from "lucide-react";
import { useState } from "react";
import type { TreeNode } from "@/bindings";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { ConfirmDialog } from "@/components/workspace/ConfirmDialog";
import { NameDialog } from "@/components/workspace/NameDialog";
import { methodColor } from "@/lib/methods";
import { type DropPosition, dropPosition, moveTarget } from "@/lib/tree";
import * as actions from "@/stores/actions";
import { useTabs } from "@/stores/tabs";
import { useWorkspace } from "@/stores/workspace";
import { cn } from "cn";

/** Which dialog is open, and for which item. */
type DialogState =
  | { kind: "new-request" | "new-folder"; parent: string }
  | { kind: "rename"; path: string; name: string }
  | { kind: "delete"; path: string; name: string; isFolder: boolean }
  | null;

interface DropState {
  over: string;
  position: DropPosition;
}

const NO_FOLDERS: string[] = [];
const NO_NODES: TreeNode[] = [];

export function CollectionTree({ dir }: { dir: string }) {
  const tree = useWorkspace((s) => s.trees[dir]) ?? NO_NODES;
  const expanded = useTabs((s) => s.byCollection[dir]?.expanded) ?? NO_FOLDERS;
  const active = useTabs((s) => s.byCollection[dir]?.active ?? null);
  const [dialog, setDialog] = useState<DialogState>(null);
  const [drop, setDrop] = useState<DropState | null>(null);

  // Start dragging only after the pointer moved a few pixels, so clicks still open requests.
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 4 } }));

  const onDragMove = (event: DragMoveEvent) => {
    const { over, activatorEvent, delta } = event;
    if (!over || !(activatorEvent instanceof PointerEvent)) {
      setDrop(null);
      return;
    }
    const pointerY = activatorEvent.clientY + delta.y;
    const isFolder = over.data.current?.isFolder === true;
    setDrop({ over: String(over.id), position: dropPosition(pointerY, over.rect, isFolder) });
  };

  const onDragEnd = (event: DragEndEvent) => {
    const dragged = String(event.active.id);
    const target = drop && moveTarget(tree, dragged, drop.over, drop.position);
    setDrop(null);
    if (target) void actions.moveItem(dir, dragged, target.parent, target.index);
  };

  const menu = {
    newRequest: (parent: string) => setDialog({ kind: "new-request", parent }),
    newFolder: (parent: string) => setDialog({ kind: "new-folder", parent }),
    rename: (node: TreeNode & { name: string }) =>
      setDialog({ kind: "rename", path: node.path, name: node.name }),
    remove: (path: string, name: string, isFolder: boolean) =>
      setDialog({ kind: "delete", path, name, isFolder }),
  };

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div className="min-h-0 flex-1 overflow-auto py-1 text-sm">
            <DndContext
              sensors={sensors}
              onDragMove={onDragMove}
              onDragEnd={onDragEnd}
              onDragCancel={() => setDrop(null)}
            >
              {tree.length === 0 && (
                <p className="px-3 py-2 text-xs text-muted-foreground">
                  No requests yet. Right-click to add one.
                </p>
              )}
              {tree.map((node) => (
                <TreeRow
                  key={node.path}
                  dir={dir}
                  node={node}
                  depth={0}
                  expanded={expanded}
                  active={active}
                  drop={drop}
                  menu={menu}
                />
              ))}
            </DndContext>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem onSelect={() => menu.newRequest("")}>New request</ContextMenuItem>
          <ContextMenuItem onSelect={() => menu.newFolder("")}>New folder</ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => void actions.revealItem(dir, "")}>
            Reveal in file manager
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <NameDialog
        open={dialog?.kind === "new-request" || dialog?.kind === "new-folder"}
        title={dialog?.kind === "new-folder" ? "New folder" : "New request"}
        label="Name"
        onOpenChange={(open) => !open && setDialog(null)}
        onConfirm={(name) => {
          if (dialog?.kind === "new-request") void actions.createRequest(dir, dialog.parent, name);
          if (dialog?.kind === "new-folder") void actions.createFolder(dir, dialog.parent, name);
        }}
      />
      <NameDialog
        open={dialog?.kind === "rename"}
        title="Rename"
        label="New name"
        confirmLabel="Rename"
        initial={dialog?.kind === "rename" ? dialog.name : ""}
        onOpenChange={(open) => !open && setDialog(null)}
        onConfirm={(name) => {
          if (dialog?.kind === "rename") void actions.renameItem(dir, dialog.path, name);
        }}
      />
      <ConfirmDialog
        open={dialog?.kind === "delete"}
        title={dialog?.kind === "delete" ? `Delete "${dialog.name}"?` : ""}
        description={
          dialog?.kind === "delete" && dialog.isFolder
            ? "The folder and every request in it will be deleted from disk."
            : "The request file will be deleted from disk."
        }
        confirmLabel="Delete"
        destructive
        onOpenChange={(open) => !open && setDialog(null)}
        onConfirm={() => {
          if (dialog?.kind === "delete") void actions.deleteItem(dir, dialog.path);
        }}
      />
    </>
  );
}

interface TreeRowProps {
  dir: string;
  node: TreeNode;
  depth: number;
  expanded: string[];
  active: string | null;
  drop: DropState | null;
  menu: {
    newRequest: (parent: string) => void;
    newFolder: (parent: string) => void;
    rename: (node: TreeNode & { name: string }) => void;
    remove: (path: string, name: string, isFolder: boolean) => void;
  };
}

function TreeRow({ dir, node, depth, expanded, active, drop, menu }: TreeRowProps) {
  const isFolder = node.kind === "folder";
  const isOpen = isFolder && expanded.includes(node.path);
  const toggleExpanded = useTabs((s) => s.toggleExpanded);
  const openTab = useTabs((s) => s.openTab);

  const draggable = useDraggable({ id: node.path, disabled: node.kind === "invalid" });
  const droppable = useDroppable({ id: node.path, data: { isFolder } });
  const dropHere = drop?.over === node.path ? drop.position : null;

  const onClick = () => {
    if (node.kind === "folder") toggleExpanded(dir, node.path);
    if (node.kind === "request") void openTab(dir, node.path);
  };

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            ref={(el) => {
              draggable.setNodeRef(el);
              droppable.setNodeRef(el);
            }}
            {...draggable.attributes}
            {...draggable.listeners}
            onClick={onClick}
            style={{ paddingLeft: 8 + depth * 14 }}
            className={cn(
              "flex h-7 cursor-default items-center gap-1.5 pr-2 select-none hover:bg-accent",
              node.path === active && "bg-accent font-medium",
              draggable.isDragging && "opacity-40",
              dropHere === "before" && "shadow-[inset_0_2px_0_var(--ring)]",
              dropHere === "after" && "shadow-[inset_0_-2px_0_var(--ring)]",
              dropHere === "inside" && "bg-ring/20",
            )}
          >
            {node.kind === "folder" && (
              <>
                <ChevronRight
                  className={cn("size-3.5 shrink-0 transition-transform", isOpen && "rotate-90")}
                />
                {isOpen ? (
                  <FolderOpen className="size-4 shrink-0 text-muted-foreground" />
                ) : (
                  <Folder className="size-4 shrink-0 text-muted-foreground" />
                )}
                <span className="truncate">{node.name}</span>
              </>
            )}
            {node.kind === "request" && (
              <>
                <span
                  className={cn(
                    "w-12 shrink-0 text-right font-mono text-[10px] font-semibold",
                    methodColor(node.method),
                  )}
                >
                  {node.method.length > 6 ? node.method.slice(0, 6) : node.method}
                </span>
                <span className="truncate">{node.name}</span>
              </>
            )}
            {node.kind === "invalid" && (
              <span
                className="flex items-center gap-1.5 truncate text-destructive"
                title={node.error}
              >
                <AlertTriangle className="size-4 shrink-0" />
                {node.path}
              </span>
            )}
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          {node.kind === "folder" && (
            <>
              <ContextMenuItem onSelect={() => menu.newRequest(node.path)}>
                New request
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => menu.newFolder(node.path)}>
                New folder
              </ContextMenuItem>
              <ContextMenuSeparator />
            </>
          )}
          {node.kind !== "invalid" && (
            <ContextMenuItem onSelect={() => menu.rename(node)}>Rename</ContextMenuItem>
          )}
          {node.kind === "request" && (
            <ContextMenuItem onSelect={() => void actions.duplicateRequest(dir, node.path)}>
              Duplicate
            </ContextMenuItem>
          )}
          <ContextMenuItem
            className="text-destructive focus:text-destructive"
            onSelect={() =>
              menu.remove(node.path, node.kind === "invalid" ? node.path : node.name, isFolder)
            }
          >
            Delete
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => void actions.revealItem(dir, node.path)}>
            Reveal in file manager
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {node.kind === "folder" &&
        isOpen &&
        node.children.map((child) => (
          <TreeRow
            key={child.path}
            dir={dir}
            node={child}
            depth={depth + 1}
            expanded={expanded}
            active={active}
            drop={drop}
            menu={menu}
          />
        ))}
    </>
  );
}
