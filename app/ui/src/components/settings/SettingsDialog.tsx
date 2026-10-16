import { open as openFile } from "@tauri-apps/plugin-dialog";
import { Check, FolderOpen, Loader2 } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { toast } from "sonner";
import {
  type Accent,
  type AwsPreferences,
  commands,
  type Preferences,
  type ThemePreference,
} from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { unwrap } from "@/lib/api";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { cn } from "cn";

type Section = "appearance" | "git" | "aws" | "history";

/**
 * The generated type marks every field optional (`#[serde(default)]`: they may be missing in
 * `settings.json`), but Rust always sends all of them.
 */
type Prefs = Required<Omit<Preferences, "aws">> & { aws: Required<AwsPreferences> };

const SECTIONS: { value: Section; label: string }[] = [
  { value: "appearance", label: "Appearance" },
  { value: "git", label: "Git" },
  { value: "aws", label: "AWS" },
  { value: "history", label: "History" },
];

const THEMES: { value: ThemePreference; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

/** The swatch colors match the accent colors in index.css. */
const ACCENTS: { value: Accent; label: string; swatch: string }[] = [
  { value: "neutral", label: "Neutral", swatch: "bg-foreground" },
  { value: "blue", label: "Blue", swatch: "bg-blue-600" },
  { value: "green", label: "Green", swatch: "bg-green-600" },
  { value: "violet", label: "Violet", swatch: "bg-violet-600" },
  { value: "orange", label: "Orange", swatch: "bg-orange-600" },
  { value: "rose", label: "Rose", swatch: "bg-rose-600" },
];

/**
 * App settings (Ctrl/Cmd+,), stored per machine in `settings.json`. Every change is saved and
 * applied right away; there is no "Save" button.
 */
export function SettingsDialog() {
  const open = useUi((s) => s.settingsOpen);
  const setOpen = useUi((s) => s.setSettingsOpen);
  const prefs = useWorkspace((s) => s.settings?.preferences) as Prefs | undefined;
  const savePreferences = useWorkspace((s) => s.savePreferences);
  const [section, setSection] = useState<Section>("appearance");

  if (!prefs) return null;
  const update = (patch: Partial<Preferences>) => void savePreferences({ ...prefs, ...patch });
  const updateAws = (patch: Partial<Prefs["aws"]>) => update({ aws: { ...prefs.aws, ...patch } });

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="gap-0 p-0 sm:max-w-3xl">
        <DialogHeader className="border-b px-5 py-4">
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Stored on this computer only, never in the workspace. Changes apply right away.
          </DialogDescription>
        </DialogHeader>
        <div className="flex h-[min(28rem,calc(100dvh-9rem))] min-h-0">
          <nav className="flex w-40 shrink-0 flex-col gap-0.5 border-r p-2">
            {SECTIONS.map((s) => (
              <button
                key={s.value}
                type="button"
                onClick={() => setSection(s.value)}
                className={cn(
                  "rounded px-2 py-1.5 text-left text-sm hover:bg-accent",
                  section === s.value && "bg-accent font-medium",
                )}
              >
                {s.label}
              </button>
            ))}
          </nav>
          <div className="min-w-0 flex-1 overflow-auto p-5">
            {section === "appearance" && (
              <Rows>
                <Row label="Theme">
                  <div className="flex gap-1">
                    {THEMES.map((t) => (
                      <Button
                        key={t.value}
                        size="sm"
                        variant={prefs.theme === t.value ? "default" : "outline"}
                        onClick={() => update({ theme: t.value })}
                      >
                        {t.label}
                      </Button>
                    ))}
                  </div>
                </Row>
                <Row label="Accent color" hint="Buttons, focus rings and highlights.">
                  <div className="flex gap-2">
                    {ACCENTS.map((a) => (
                      <button
                        key={a.value}
                        type="button"
                        title={a.label}
                        aria-label={a.label}
                        aria-pressed={prefs.accent === a.value}
                        onClick={() => update({ accent: a.value })}
                        className={cn(
                          "flex size-7 items-center justify-center rounded-full ring-offset-2 ring-offset-background",
                          a.swatch,
                          prefs.accent === a.value && "ring-2 ring-ring",
                        )}
                      >
                        {prefs.accent === a.value && <Check className="size-4 text-background" />}
                      </button>
                    ))}
                  </div>
                </Row>
                <Row label="UI size">
                  <NumberSelect
                    value={prefs.uiScale}
                    options={[80, 90, 100, 110, 125, 150]}
                    format={(n) => `${n}%`}
                    onChange={(uiScale) => update({ uiScale })}
                  />
                </Row>
                <Row label="Editor font size" hint="URL bar, bodies, scripts and responses.">
                  <NumberSelect
                    value={prefs.editorFontSize}
                    options={[11, 12, 13, 14, 15, 16, 18, 20]}
                    format={(n) => `${n} px`}
                    onChange={(editorFontSize) => update({ editorFontSize })}
                  />
                </Row>
              </Rows>
            )}

            {section === "git" && (
              <Rows>
                <Row
                  label="Git program"
                  hint="Leave empty to use git from the PATH. Frida runs the git command line, so your SSH keys and credential helpers work as usual."
                >
                  <PathInput
                    key={prefs.gitPath ?? ""}
                    value={prefs.gitPath}
                    placeholder="git (from the PATH)"
                    browseTitle="Choose the git program"
                    onChange={(gitPath) => update({ gitPath })}
                  />
                  <GitCheck path={prefs.gitPath} />
                </Row>
              </Rows>
            )}

            {section === "aws" && (
              <Rows>
                <Row
                  label="Config file"
                  hint="Profiles and SSO settings. Empty: AWS_CONFIG_FILE, else ~/.aws/config."
                >
                  <PathInput
                    key={prefs.aws.configFile ?? ""}
                    value={prefs.aws.configFile}
                    placeholder="~/.aws/config"
                    browseTitle="Choose the AWS config file"
                    onChange={(configFile) => updateAws({ configFile })}
                  />
                </Row>
                <Row
                  label="Credentials file"
                  hint="Empty: AWS_SHARED_CREDENTIALS_FILE, else ~/.aws/credentials."
                >
                  <PathInput
                    key={prefs.aws.credentialsFile ?? ""}
                    value={prefs.aws.credentialsFile}
                    placeholder="~/.aws/credentials"
                    browseTitle="Choose the AWS credentials file"
                    onChange={(credentialsFile) => updateAws({ credentialsFile })}
                  />
                </Row>
                <Row
                  label="Keep fetched secrets"
                  hint="Secret values from AWS Secrets Manager are kept in memory (never on disk) for this long, then fetched again."
                >
                  <div className="flex items-center gap-2">
                    <NumberSelect
                      value={prefs.aws.cacheMinutes}
                      options={[1, 5, 15, 30, 60, 240, 480]}
                      format={(n) => (n < 60 ? `${n} min` : `${n / 60} h`)}
                      onChange={(cacheMinutes) => updateAws({ cacheMinutes })}
                    />
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() =>
                        void commands
                          .clearAwsCache()
                          .then(() => toast.success("AWS secrets will be fetched again"))
                      }
                    >
                      Forget fetched secrets
                    </Button>
                  </div>
                </Row>
              </Rows>
            )}

            {section === "history" && (
              <Rows>
                <Row
                  label="Entries to keep"
                  hint="Older requests are removed from the history after each send."
                >
                  <NumberSelect
                    value={prefs.historySize}
                    options={[100, 500, 1000, 5000, 10000]}
                    format={(n) => n.toLocaleString()}
                    onChange={(historySize) => update({ historySize })}
                  />
                </Row>
              </Rows>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Rows({ children }: { children: ReactNode }) {
  return <div className="flex flex-col gap-5">{children}</div>;
}

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-sm font-medium">{label}</span>
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
      {children}
    </div>
  );
}

/** A choice of numbers. A value set by hand in `settings.json` is listed too. */
function NumberSelect({
  value,
  options,
  format,
  onChange,
}: {
  value: number;
  options: number[];
  format: (n: number) => string;
  onChange: (n: number) => void;
}) {
  const all = options.includes(value) ? options : [...options, value].sort((a, b) => a - b);
  return (
    <Select value={String(value)} onValueChange={(v) => onChange(Number(v))}>
      <SelectTrigger size="sm" className="w-32">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {all.map((n) => (
          <SelectItem key={n} value={String(n)}>
            {format(n)}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

/** A file path: typed (saved when leaving the field or pressing Enter) or picked. */
function PathInput({
  value,
  placeholder,
  browseTitle,
  onChange,
}: {
  value: string | null;
  placeholder: string;
  browseTitle: string;
  onChange: (path: string | null) => void;
}) {
  // The field keeps what is typed; the parent remounts it (`key`) when the saved value changes.
  const [text, setText] = useState(value ?? "");
  const commit = (next: string) => {
    const path = next.trim() || null;
    if (path !== value) onChange(path);
  };
  const browse = async () => {
    const path = await openFile({ multiple: false, directory: false, title: browseTitle });
    if (typeof path === "string") commit(path);
  };
  return (
    <div className="flex gap-2">
      <Input
        className="h-8 font-mono text-xs"
        value={text}
        placeholder={placeholder}
        spellCheck={false}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => commit(text)}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit(text);
        }}
      />
      <Button size="sm" variant="outline" onClick={() => void browse()}>
        <FolderOpen /> Browse…
      </Button>
    </div>
  );
}

/** Runs `git --version` with the configured program, so a wrong path shows up at once. */
function GitCheck({ path }: { path: string | null }) {
  // The result belongs to the path it was checked for; a newer path shows "Checking…".
  const [result, setResult] = useState<{
    path: string | null;
    state: { kind: "ok"; version: string } | { kind: "error"; message: string };
  } | null>(null);
  useEffect(() => {
    let cancelled = false;
    unwrap(commands.checkGit(path)).then(
      (version) => !cancelled && setResult({ path, state: { kind: "ok", version } }),
      (e: { message?: string }) =>
        !cancelled &&
        setResult({ path, state: { kind: "error", message: e.message ?? String(e) } }),
    );
    return () => {
      cancelled = true;
    };
  }, [path]);
  const state = result?.path === path ? result.state : { kind: "checking" as const };
  return (
    <p className="text-xs">
      {state.kind === "checking" && (
        <span className="text-muted-foreground">
          <Loader2 className="mr-1 inline size-3 animate-spin" />
          Checking…
        </span>
      )}
      {state.kind === "ok" && <span className="text-success">git {state.version}</span>}
      {state.kind === "error" && <span className="text-destructive">{state.message}</span>}
    </p>
  );
}
