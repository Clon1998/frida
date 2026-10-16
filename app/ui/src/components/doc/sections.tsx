import type { ReactNode } from "react";
import type { KeyValue as RawKeyValue, Scripts as RawScripts } from "@/bindings";
import { type CodeLanguage, CodeEditor } from "@/components/editor/CodeEditor";
import { KeyValueTable } from "@/components/request/KeyValueTable";
import type { Complete } from "@/lib/model";

/**
 * Sub-tab contents shared by the request, folder and collection editors. Everything here is
 * saved to the YAML file.
 */

/** A short explanation above an editor. */
export function SectionNote({ children }: { children: ReactNode }) {
  return <p className="mb-2 text-xs text-muted-foreground">{children}</p>;
}

type KeyValue = Complete<RawKeyValue>;

export function VarsSection({
  vars,
  onChange,
}: {
  vars: KeyValue[];
  onChange: (vars: KeyValue[]) => void;
}) {
  return <KeyValueTable rows={vars} onChange={onChange} namePlaceholder="Variable" />;
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
      <SectionNote>
        JavaScript with Bruno&apos;s <code>req</code>, <code>res</code> and <code>bru</code> API.
        Collection scripts run first, then folders, then the request. <code>console.log</code>{" "}
        output goes to the console (Ctrl/Cmd+J).
      </SectionNote>
      <span className="text-xs font-medium text-muted-foreground">Pre-request</span>
      <CodeEditor
        className="min-h-24 flex-1 overflow-hidden rounded-md border"
        value={scripts.pre}
        language="javascript"
        onChange={(pre) => onChange({ ...scripts, pre })}
      />
      <span className="text-xs font-medium text-muted-foreground">Post-response</span>
      <CodeEditor
        className="min-h-24 flex-1 overflow-hidden rounded-md border"
        value={scripts.post}
        language="javascript"
        onChange={(post) => onChange({ ...scripts, post })}
      />
    </div>
  );
}

export function TextSection({
  value,
  onChange,
  note,
  language = "text",
}: {
  value: string;
  onChange: (value: string) => void;
  note?: ReactNode;
  language?: CodeLanguage;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      {note && <SectionNote>{note}</SectionNote>}
      <CodeEditor
        className="min-h-24 flex-1 overflow-hidden rounded-md border"
        value={value}
        language={language}
        onChange={onChange}
      />
    </div>
  );
}

/** The note above every Tests editor. */
export function TestsNote() {
  return (
    <>
      Runs after the post-response scripts, e.g.{" "}
      <code>test(&quot;ok&quot;, () =&gt; expect(res.status).to.equal(200))</code>. Results are in
      the response&apos;s Tests tab.
    </>
  );
}
