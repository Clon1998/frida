import { useState } from "react";
import { inputBoxClassName } from "@/components/ui/input";
import { cn } from "cn";
import { CodeEditor } from "./CodeEditor";

/**
 * A shadcn `<Input>` whose content is a one-line code editor: same box, but with `{{variable}}`
 * highlighting, tooltips and `{{` autocompletion (variables come from `VariablesContext`).
 *
 * With `secret`, a literal value shows as dots while the field isn't focused (like a password
 * input). Values starting with `{{` are only a variable reference and are always shown. The
 * dots are CSS (`-webkit-text-security`, supported by every Tauri webview), so the field never
 * swaps elements and keeps focus while typing.
 */
export function VariableInput({
  value,
  onChange,
  placeholder,
  secret = false,
  className,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  secret?: boolean;
  className?: string;
}) {
  const [focused, setFocused] = useState(false);
  const masked = secret && !focused && value !== "" && !value.trim().startsWith("{{");
  return (
    <div
      data-slot="input"
      data-masked={masked || undefined}
      className={cn(
        inputBoxClassName,
        // The editor brings its own line height; this keeps the box at the input's height.
        "flex h-auto min-h-9 items-center py-0",
        "focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/50",
        // `!`: CodeMirror adds its theme styles at runtime, after ours.
        "[&_.cm-editor]:w-full [&_.cm-editor]:bg-transparent! [&_.cm-editor]:text-[length:inherit]!",
        masked && "[&_.cm-content]:[-webkit-text-security:disc]",
        className,
      )}
      onFocus={() => setFocused(true)}
      onBlur={(e) => {
        // Popups (autocompletion) live outside; only leaving the field itself counts.
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setFocused(false);
      }}
    >
      <CodeEditor
        singleLine
        className="w-full min-w-0"
        value={value}
        placeholder={placeholder}
        onChange={onChange}
      />
    </div>
  );
}
