import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { formatKeys, SHORTCUTS } from "@/lib/shortcuts";
import { useUi } from "@/stores/ui";

/** Ctrl/Cmd+/: every keyboard shortcut. */
export function ShortcutsDialog() {
  const open = useUi((s) => s.shortcutsOpen);
  const setOpen = useUi((s) => s.setShortcutsOpen);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogDescription>Right-click items in the sidebar and tabs for more.</DialogDescription>
        </DialogHeader>
        <table className="w-full text-sm">
          <tbody>
            {SHORTCUTS.map((s) => (
              <tr key={`${s.keys}-${s.action}`} className="border-b last:border-0">
                <td className="py-1.5 pr-4 whitespace-nowrap">
                  <kbd className="rounded border bg-muted px-1.5 py-0.5 font-mono text-xs">
                    {formatKeys(s.keys)}
                  </kbd>
                </td>
                <td className="py-1.5 text-muted-foreground">{s.action}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </DialogContent>
    </Dialog>
  );
}
