import { Input } from "@/components/ui/input";
import { methodColor } from "@/lib/methods";
import { cn } from "cn";

const COMMON = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

interface MethodSelectProps {
  value: string;
  onChange: (method: string) => void;
}

/**
 * A text input with suggestions, so custom methods such as `PROPFIND` can be typed while the
 * common ones are one click away.
 */
export function MethodSelect({ value, onChange }: MethodSelectProps) {
  return (
    <>
      <Input
        list="http-methods"
        aria-label="HTTP method"
        className={cn(
          "h-9 w-28 shrink-0 font-mono text-sm font-semibold uppercase",
          methodColor(value),
        )}
        value={value}
        onChange={(e) => onChange(e.target.value.toUpperCase())}
        spellCheck={false}
      />
      <datalist id="http-methods">
        {COMMON.map((m) => (
          <option key={m} value={m} />
        ))}
      </datalist>
    </>
  );
}
