import type { ReactNode } from "react";
import type { KeyValue as RawKeyValue, Scripts as RawScripts, SnippetKind } from "@/bindings";
import { type CodeLanguage, CodeEditor } from "@/components/editor/CodeEditor";
import { KeyValueTable } from "@/components/request/KeyValueTable";
import type { Complete } from "@/lib/model";
import { ExamplesMenu } from "./ExamplesMenu";

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
        JavaScript with the <code>req</code>, <code>res</code> and <code>fri</code> API
        (Bruno&apos;s <code>bru</code> works too). Collection scripts run first, then folders, then
        the request. <code>console.log</code> output goes to the console (Ctrl/Cmd+J).
      </SectionNote>
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-muted-foreground">Pre-request</span>
        <ExamplesMenu
          kind="preRequest"
          value={scripts.pre}
          onChange={(pre) => onChange({ ...scripts, pre })}
        />
      </div>
      <CodeEditor
        className="min-h-24 flex-1 overflow-hidden rounded-md border"
        value={scripts.pre}
        language="javascript"
        onChange={(pre) => onChange({ ...scripts, pre })}
      />
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-muted-foreground">Post-response</span>
        <ExamplesMenu
          kind="postResponse"
          value={scripts.post}
          onChange={(post) => onChange({ ...scripts, post })}
        />
      </div>
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
  examples,
}: {
  value: string;
  onChange: (value: string) => void;
  note?: ReactNode;
  language?: CodeLanguage;
  /** Shows an "Examples" menu for this kind of script. */
  examples?: SnippetKind;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      {(note || examples) && (
        <div className="flex items-start justify-between gap-2">
          {note ? <SectionNote>{note}</SectionNote> : <span />}
          {examples && <ExamplesMenu kind={examples} value={value} onChange={onChange} />}
        </div>
      )}
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
