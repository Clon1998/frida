import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";

interface NameDialogProps {
  open: boolean;
  title: string;
  label: string;
  initial?: string;
  confirmLabel?: string;
  onConfirm: (name: string) => void;
  onOpenChange: (open: boolean) => void;
}

/** A small dialog asking for a name (new request, new folder, rename, ...). */
export function NameDialog({ open, title, onOpenChange, ...form }: NameDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        {/* Mounted only while open, so the input starts from `initial` every time. */}
        {open && <NameForm {...form} onClose={() => onOpenChange(false)} />}
      </DialogContent>
    </Dialog>
  );
}

interface NameFormProps {
  label: string;
  initial?: string;
  confirmLabel?: string;
  onConfirm: (name: string) => void;
  onClose: () => void;
}

function NameForm({
  label,
  initial = "",
  confirmLabel = "Create",
  onConfirm,
  onClose,
}: NameFormProps) {
  const [name, setName] = useState(initial);
  const valid = name.trim().length > 0;

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (!valid) return;
        onConfirm(name.trim());
        onClose();
      }}
    >
      <label className="flex flex-col gap-1.5 text-sm">
        {label}
        <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <DialogFooter className="mt-4">
        <Button type="button" variant="outline" onClick={onClose}>
          Cancel
        </Button>
        <Button type="submit" disabled={!valid}>
          {confirmLabel}
        </Button>
      </DialogFooter>
    </form>
  );
}
