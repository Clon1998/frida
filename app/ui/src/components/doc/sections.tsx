import type { ReactNode } from "react";
import type { Auth, KeyValue as RawKeyValue, Scripts as RawScripts } from "@/bindings";
import { CodeEditor } from "@/components/editor/CodeEditor";
import { KeyValueTable } from "@/components/request/KeyValueTable";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { Complete } from "@/lib/model";

/**
 * Sub-tab contents shared by the request, folder and collection editors. Everything here is
 * saved to the YAML file. Some of it is only applied when sending in later milestones, which
 * {@link NotAppliedYet} says right where the user edits it.
 */

export function NotAppliedYet({ children }: { children: ReactNode }) {
  return (
    <p className="mb-3 rounded-md border border-dashed px-3 py-2 text-xs text-muted-foreground">
      {children}
    </p>
  );
}

type KeyValue = Complete<RawKeyValue>;

export function VarsSection({
  vars,
  onChange,
}: {
  vars: KeyValue[];
  onChange: (vars: KeyValue[]) => void;
}) {
  return (
    <>
      <NotAppliedYet>
        Saved now; <code>{"{{variables}}"}</code> are resolved when sending once environments and
        variables arrive (next milestone).
      </NotAppliedYet>
      <KeyValueTable rows={vars} onChange={onChange} namePlaceholder="Variable" />
    </>
  );
}

type Scripts = Complete<RawScripts>;

export function ScriptsSection({
  scripts,
  onChange,
}: {
  scripts: Scripts;
  onChange: (scripts: Scripts) => void;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col gap-2">
      <NotAppliedYet>Saved now; scripts run once the scripting milestone is done.</NotAppliedYet>
      <span className="text-xs font-medium text-muted-foreground">Pre-request</span>
      <CodeEditor
        className="min-h-24 flex-1 overflow-hidden rounded-md border"
        value={scripts.pre}
        onChange={(pre) => onChange({ ...scripts, pre })}
      />
      <span className="text-xs font-medium text-muted-foreground">Post-response</span>
      <CodeEditor
        className="min-h-24 flex-1 overflow-hidden rounded-md border"
        value={scripts.post}
        onChange={(post) => onChange({ ...scripts, post })}
      />
    </div>
  );
}

export function TextSection({
  value,
  onChange,
  note,
}: {
  value: string;
  onChange: (value: string) => void;
  note?: string;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      {note && <NotAppliedYet>{note}</NotAppliedYet>}
      <CodeEditor
        className="min-h-24 flex-1 overflow-hidden rounded-md border"
        value={value}
        onChange={onChange}
      />
    </div>
  );
}

/**
 * Auth mode only, for now: inherit or none. The scheme editors (Basic, Bearer, API key, OAuth2)
 * come with the auth milestone. Existing custom schemes from the file are kept untouched.
 */
type AuthModel = Complete<Auth>;

export function AuthSection({
  auth,
  onChange,
  canInherit,
}: {
  auth: AuthModel;
  onChange: (auth: AuthModel) => void;
  canInherit: boolean;
}) {
  return (
    <div className="flex flex-col gap-3">
      <NotAppliedYet>
        Basic, Bearer, API key and OAuth2 editors come with the auth milestone. Until then auth
        isn't applied when sending.
      </NotAppliedYet>
      <Select
        value={auth.mode}
        onValueChange={(mode) => {
          if (mode === "inherit" || mode === "none") onChange({ mode });
        }}
      >
        <SelectTrigger size="sm" className="w-56">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {canInherit && <SelectItem value="inherit">Inherit from parent</SelectItem>}
          <SelectItem value="none">No auth</SelectItem>
          {auth.mode === "custom" && (
            <SelectItem value="custom">Custom ({auth.schemes.length} schemes)</SelectItem>
          )}
        </SelectContent>
      </Select>
    </div>
  );
}
