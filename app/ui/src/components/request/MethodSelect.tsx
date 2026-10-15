import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { METHODS, methodColor } from "@/lib/methods";
import { cn } from "cn";

interface MethodSelectProps {
  value: string;
  onChange: (method: string) => void;
}

/**
 * A dropdown of the standard HTTP methods. The file format allows any method, so a custom one
 * (e.g. `PROPFIND` from an import) is shown as an extra entry instead of being lost.
 */
export function MethodSelect({ value, onChange }: MethodSelectProps) {
  const methods = METHODS.includes(value) ? METHODS : [...METHODS, value];
  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger
        aria-label="HTTP method"
        className={cn("h-9 w-28 shrink-0 font-mono text-sm font-semibold", methodColor(value))}
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {methods.map((m) => (
          <SelectItem key={m} value={m} className={cn("font-mono font-semibold", methodColor(m))}>
            {m}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
