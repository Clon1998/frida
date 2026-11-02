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
import {
  AlertTriangle,
  ChevronRight,
  Folder,
  FolderOpen,
  FolderPlus,
  Plus,
  Settings,
  SquareTerminal,
} from "lucide-react";
import { type ReactNode, useState } from "react";
import type { TreeNode } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ConfirmDialog } from "@/components/workspace/ConfirmDialog";
import { NameDialog } from "@/components/workspace/NameDialog";
import { methodColor } from "@/lib/methods";
import { countRequests, type DropPosition, dropPosition, moveTarget } from "@/lib/tree";
import * as actions from "@/stores/actions";
import { useDialogs } from "@/stores/dialogs";
import { runnerTabPath, useTabs } from "@/stores/tabs";
import { useUi } from "@/stores/ui";
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

/**
 * The requests and folders of a collection. On its own it fills the sidebar and scrolls. As a
 * `section` (the "all collections" view) it has a sticky header with the collection's name,
 * collapses, and grows with its content inside the list that scrolls all collections.
 */
export function CollectionTree({
  dir,
  section = false,
  dropEdge = null,
}: {
  dir: string;
  section?: boolean;
  /** While a section is dragged over this one: where it would go (the line to draw). */
  dropEdge?: "before" | "after" | null;
}) {
  const tree = useWorkspace((s) => s.trees[dir]) ?? NO_NODES;
  const expanded = useTabs((s) => s.byCollection[dir]?.expanded) ?? NO_FOLDERS;
  const active = useTabs((s) => s.byCollection[dir]?.active ?? null);
  const [dialog, setDialog] = useState<DialogState>(null);
  const [drop, setDrop] = useState<DropState | null>(null);
  const openTab = useTabs((s) => s.openTab);
  const name = useWorkspace((s) => s.workspace?.collections.find((c) => c.dir === dir)?.name);
  const isActiveCollection = useWorkspace((s) => s.activeCollectionDir === dir);
  const open = useUi((s) => s.openCollections[dir] ?? false);
  const setOpen = useUi((s) => s.setCollectionOpen);

  /**
   * In the "all collections" view, working in a collection selects it, so the tab bar,
   * environment, History and Git follow along (they are per collection).
   */
  const activate = () => {
    if (section && !isActiveCollection) useWorkspace.getState().setActiveCollection(dir);
  };

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

  /** Every action selects the collection first (see `activate`), also from the header's menus. */
  const selecting =
    <A extends unknown[]>(action: (...args: A) => void) =>
    (...args: A) => {
      activate();
      action(...args);
    };

  const menu = {
    newRequest: selecting((parent: string) => setDialog({ kind: "new-request", parent })),
    newFolder: selecting((parent: string) => setDialog({ kind: "new-folder", parent })),
    rename: selecting((node: TreeNode & { name: string }) =>
      setDialog({ kind: "rename", path: node.path, name: node.name }),
    ),
    remove: selecting((path: string, name: string, isFolder: boolean) =>
      setDialog({ kind: "delete", path, name, isFolder }),
    ),
    folderSettings: selecting((path: string) => void openTab(dir, path)),
    fromCurl: selecting((parent: string) => useDialogs.getState().openCurlImport(dir, parent)),
    copyCurl: selecting((path: string) => useDialogs.getState().openCurlExport(dir, path)),
    run: selecting((folder: string) => void openTab(dir, runnerTabPath(folder))),
    exportItem: selecting(
      (node: TreeNode & { name: string }) =>
        void actions.exportArchive(
          node.kind === "folder"
            ? { kind: "folder", collectionDir: dir, rel: node.path }
            : { kind: "request", collectionDir: dir, rel: node.path },
          node.name,
        ),
    ),
    importInto: selecting((parent: string) => useDialogs.getState().openArchiveImport(dir, parent)),
  };

  // The collection's own menu: right-click on the empty space below the tree, or on the header.
  const rootMenu = (
    <ContextMenuContent>
      <ContextMenuItem onSelect={() => menu.newRequest("")}>New request</ContextMenuItem>
      <ContextMenuItem onSelect={() => menu.newFolder("")}>New folder</ContextMenuItem>
      <ContextMenuItem onSelect={() => menu.fromCurl("")}>New request from curl…</ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => menu.run("")}>Run collection…</ContextMenuItem>
      <ContextMenuItem onSelect={() => menu.importInto("")}>Import Frida archive…</ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => void actions.revealItem(dir, "")}>
        Reveal in file manager
      </ContextMenuItem>
    </ContextMenuContent>
  );

  // The header of a section in the "all collections" view.
  const sectionHeader = (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        {/* The background mixes like the sidebar's (`bg-muted/30`), but opaque, as the
            header sticks over the rows (see `SectionFrame`). */}
        <div
          className={cn(
            "group flex h-8 items-center gap-1 border-b pr-2 pl-1 select-none",
            "bg-[color-mix(in_oklch,var(--muted)_30%,var(--background))]",
            isActiveCollection && "shadow-[inset_2px_0_0_var(--primary)]",
          )}
        >
          <button
            type="button"
            aria-expanded={open}
            className="flex h-full min-w-0 flex-1 items-center gap-1 text-left text-sm"
            // Only expands or collapses: looking into a collection doesn't switch to it.
            onClick={() => setOpen(dir, !open)}
          >
            <ChevronRight
              className={cn(
                "size-3.5 shrink-0 text-muted-foreground transition-transform",
                open && "rotate-90",
              )}
            />
            <span
              className={cn(
                "truncate",
                isActiveCollection ? "font-medium" : "text-muted-foreground",
              )}
            >
              {name}
            </span>
          </button>
          {!open && (
            <span
              className="text-xs text-muted-foreground tabular-nums group-hover:hidden"
              title="Requests in this collection"
            >
              {countRequests(tree)}
            </span>
          )}
          <span className="hidden items-center group-hover:flex has-[[data-state=open]]:flex">
            <AddMenu
              onRequest={() => menu.newRequest("")}
              onFolder={() => menu.newFolder("")}
              onCurl={() => menu.fromCurl("")}
            />
            <Button
              variant="ghost"
              size="icon"
              className="size-5"
              title="Collection settings"
              aria-label={`${name} settings`}
              onClick={() => menu.folderSettings("")}
            >
              <Settings className="size-3.5" />
            </Button>
          </span>
        </div>
      </ContextMenuTrigger>
      {rootMenu}
    </ContextMenu>
  );

  const content = (
    <>
      {!section && (
        <div className="flex h-8 shrink-0 items-center justify-between px-3">
          <span className="text-xs font-medium text-muted-foreground">Requests</span>
          <AddMenu
            onRequest={() => menu.newRequest("")}
            onFolder={() => menu.newFolder("")}
            onCurl={() => menu.fromCurl("")}
          />
        </div>
      )}
      {(!section || open) && (
        <ContextMenu>
          <ContextMenuTrigger asChild>
            <div
              className={cn("pb-1 text-sm", !section && "min-h-0 flex-1 overflow-auto")}
              data-testid="request-tree"
              onPointerDownCapture={activate}
            >
              <DndContext
                sensors={sensors}
                onDragMove={onDragMove}
                onDragEnd={onDragEnd}
                onDragCancel={() => setDrop(null)}
              >
                {tree.length === 0 && (
                  <div className="flex flex-col items-center gap-2 px-3 py-6 text-center">
                    <p className="text-xs text-muted-foreground">This collection is empty.</p>
                    <Button size="sm" onClick={() => menu.newRequest("")}>
                      <Plus /> New request
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => menu.newFolder("")}>
                      <FolderPlus /> New folder
                    </Button>
                  </div>
                )}
                {tree.map((node) => (
                  <TreeRow
                    key={node.path}
                    dir={dir}
                    node={node}
                    depth={section ? 1 : 0}
                    expanded={expanded}
                    active={active}
                    drop={drop}
                    menu={menu}
                  />
                ))}
              </DndContext>
            </div>
          </ContextMenuTrigger>
          {rootMenu}
        </ContextMenu>
      )}

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

  // A section's header sticks to the top of the scrolling list only while its own rows pass.
  return section ? (
    <SectionFrame dir={dir} name={name ?? ""} dropEdge={dropEdge} header={sectionHeader}>
      {content}
    </SectionFrame>
  ) : (
    content
  );
}

/**
 * A collection in the "all collections" view. The header sticks to the top of the scrolling
 * list while the section's own rows pass, and is the handle to drag the collection to another
 * place (the `DndContext` is in `Sidebar`; the rows have their own one inside).
 */
function SectionFrame({
  dir,
  name,
  dropEdge,
  header,
  children,
}: {
  dir: string;
  name: string;
  dropEdge: "before" | "after" | null;
  header: ReactNode;
  children: ReactNode;
}) {
  const { setNodeRef: dragRef, listeners, isDragging } = useDraggable({ id: dir });
  const { setNodeRef: dropRef } = useDroppable({ id: dir });
  return (
    <section
      ref={dropRef}
      aria-label={name}
      // The drop line is drawn above everything, as the opaque header would cover a border.
      className={cn(
        "relative",
        isDragging && "opacity-40",
        dropEdge === "before" &&
          "before:absolute before:inset-x-0 before:top-0 before:z-20 before:h-0.5 before:bg-ring",
        dropEdge === "after" &&
          "after:absolute after:inset-x-0 after:bottom-0 after:z-20 after:h-0.5 after:bg-ring",
      )}
    >
      {/* Only the listeners, not dnd-kit's `role="button"`: the header has its own buttons. */}
      <div ref={dragRef} {...listeners} className="sticky top-0 z-10">
        {header}
      </div>
      {children}
    </section>
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
    folderSettings: (path: string) => void;
    fromCurl: (parent: string) => void;
    copyCurl: (path: string) => void;
    /** Opens the collection runner for a folder. */
    run: (folder: string) => void;
    /** Saves a folder or request as a Frida archive. */
    exportItem: (node: TreeNode & { name: string }) => void;
    /** Imports a Frida archive into a folder. */
    importInto: (parent: string) => void;
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

  // Like Bruno: clicking a folder opens its settings tab and expands/collapses it. The chevron
  // only expands/collapses.
  // Single click opens a preview tab (replaced by the next single click); double click keeps
  // the tab open, like Bruno, Postman and VS Code.
  const onClick = () => {
    if (node.kind === "folder") {
      toggleExpanded(dir, node.path);
      void openTab(dir, node.path, { preview: true });
    }
    if (node.kind === "request") void openTab(dir, node.path, { preview: true });
  };
  const onDoubleClick = () => {
    if (node.kind === "folder") {
      // Undo the second click's toggle, so a double click doesn't collapse the folder again.
      toggleExpanded(dir, node.path, true);
    }
    if (node.kind !== "invalid") void openTab(dir, node.path);
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
            onDoubleClick={onDoubleClick}
            style={{ paddingLeft: 8 + depth * 14 }}
            className={cn(
              "group flex h-7 cursor-default items-center gap-1.5 pr-2 select-none hover:bg-accent",
              node.path === active && "bg-accent font-medium",
              draggable.isDragging && "opacity-40",
              dropHere === "before" && "shadow-[inset_0_2px_0_var(--ring)]",
              dropHere === "after" && "shadow-[inset_0_-2px_0_var(--ring)]",
              dropHere === "inside" && "bg-ring/20",
            )}
          >
            {node.kind === "folder" && (
              <>
                <button
                  type="button"
                  aria-label={isOpen ? "Collapse folder" : "Expand folder"}
                  className="flex size-4 shrink-0 items-center justify-center rounded hover:bg-muted"
                  onPointerDown={(e) => e.stopPropagation()}
                  onClick={(e) => {
                    e.stopPropagation();
                    toggleExpanded(dir, node.path);
                  }}
                >
                  <ChevronRight
                    className={cn("size-3.5 transition-transform", isOpen && "rotate-90")}
                  />
                </button>
                {isOpen ? (
                  <FolderOpen className="size-4 shrink-0 text-muted-foreground" />
                ) : (
                  <Folder className="size-4 shrink-0 text-muted-foreground" />
                )}
                <span className="flex-1 truncate">{node.name}</span>
                <span className="opacity-0 group-hover:opacity-100 has-[[data-state=open]]:opacity-100">
                  <AddMenu
                    onRequest={() => menu.newRequest(node.path)}
                    onFolder={() => menu.newFolder(node.path)}
                    onCurl={() => menu.fromCurl(node.path)}
                  />
                </span>
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
              <ContextMenuItem onSelect={() => menu.fromCurl(node.path)}>
                New request from curl…
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => menu.folderSettings(node.path)}>
                Settings…
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => menu.run(node.path)}>Run folder…</ContextMenuItem>
              <ContextMenuItem onSelect={() => menu.importInto(node.path)}>
                Import Frida archive here…
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
          {node.kind === "request" && (
            <ContextMenuItem onSelect={() => menu.copyCurl(node.path)}>
              Copy as curl
            </ContextMenuItem>
          )}
          {node.kind !== "invalid" && (
            <ContextMenuItem onSelect={() => menu.exportItem(node)}>Export…</ContextMenuItem>
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

/** A small "+" that offers a new request or folder. Clicks don't reach the row underneath. */
export function AddMenu({
  onRequest,
  onFolder,
  onCurl,
}: {
  onRequest: () => void;
  onFolder: () => void;
  onCurl: () => void;
}) {
  return (
    // Non-modal, so the name dialog it opens gets focus cleanly when the menu closes.
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="size-5"
          aria-label="Add request or folder"
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => e.stopPropagation()}
        >
          <Plus className="size-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" onClick={(e) => e.stopPropagation()}>
        <DropdownMenuItem onSelect={onRequest}>
          <Plus /> New request
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={onFolder}>
          <FolderPlus /> New folder
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={onCurl}>
          <SquareTerminal /> New request from curl…
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
