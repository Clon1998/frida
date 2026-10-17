import { open as openFile } from "@tauri-apps/plugin-dialog";
import { Check, FolderOpen, Loader2 } from "lucide-react";
import { type ReactNode, useEffect, useId, useState } from "react";
import { toast } from "sonner";
import {
  type Accent,
  type AwsPreferences,
  commands,
  type Preferences,
  type ProxyMode,
  type ProxyPreferences,
  type ThemePreference,
} from "@/bindings";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { showError, unwrap } from "@/lib/api";
import { useUi } from "@/stores/ui";
import { useUpdate } from "@/stores/update";
import { useAwsProfiles } from "@/components/env/useAwsProfiles";
import { useWorkspace } from "@/stores/workspace";
import { cn } from "cn";

type Section = "appearance" | "network" | "git" | "aws" | "history" | "updates";

/**
 * The generated type marks every field optional (`#[serde(default)]`: they may be missing in
 * `settings.json`), but Rust always sends all of them.
 */
type Prefs = Required<Omit<Preferences, "aws" | "proxy">> & {
  aws: Required<AwsPreferences>;
  proxy: Required<ProxyPreferences>;
};

const PROXY_MODES: { value: ProxyMode; label: string }[] = [
  { value: "system", label: "System settings" },
  { value: "none", label: "No proxy" },
  { value: "manual", label: "Manual" },
];

const SECTIONS: { value: Section; label: string }[] = [
  { value: "appearance", label: "Appearance" },
  { value: "network", label: "Network" },
  { value: "git", label: "Git" },
  { value: "aws", label: "AWS" },
  { value: "history", label: "History" },
  { value: "updates", label: "Updates" },
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

            {section === "network" && (
              <NetworkSettings
                proxy={prefs.proxy}
                onChange={(patch) => update({ proxy: { ...prefs.proxy, ...patch } })}
              />
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
                  label="Accounts"
                  hint="Secrets can name an AWS account alias (e.g. shared-services) instead of using the environment's profile. Here you choose which of your profiles each alias is."
                >
                  <AwsAccounts />
                </Row>
                <Row
                  label="Keep fetched secrets"
                  hint="Secret values from AWS (Secrets Manager, Cognito, API Gateway) are kept in memory (never on disk) for this long, then fetched again."
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

            {section === "updates" && (
              <Rows>
                <Row
                  label="Automatic check"
                  hint="Looks for a new version on GitHub when Frida starts, at most once a day. Nothing is installed without asking."
                >
                  <label className="flex items-center gap-2 text-sm">
                    <Checkbox
                      checked={prefs.checkForUpdates}
                      onCheckedChange={(checked) => update({ checkForUpdates: checked === true })}
                    />
                    Check for updates on start
                  </label>
                </Row>
                <Row label="Version">
                  <UpdateCheck />
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
  // Named after its label, so screen readers (and the UI tests) can find "Theme" etc.
  const id = useId();
  return (
    <div className="flex flex-col gap-1.5" role="group" aria-labelledby={id}>
      <span id={id} className="text-sm font-medium">
        {label}
      </span>
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

/**
 * The HTTP proxy (PLAN.md M16) for every request Frida makes: sends, OAuth2 token calls,
 * `fri.sendRequest` and AWS. The password goes to the keychain, never into `settings.json`.
 */
function NetworkSettings({
  proxy,
  onChange,
}: {
  proxy: Prefs["proxy"];
  onChange: (patch: Partial<Prefs["proxy"]>) => void;
}) {
  return (
    <Rows>
      <Row label="Proxy">
        <div className="flex flex-wrap gap-1">
          {PROXY_MODES.map((m) => (
            <Button
              key={m.value}
              size="sm"
              variant={proxy.mode === m.value ? "default" : "outline"}
              onClick={() => onChange({ mode: m.value })}
            >
              {m.label}
            </Button>
          ))}
        </div>
        <p className="text-xs text-muted-foreground">
          {proxy.mode === "system" &&
            "Uses the operating system's proxy settings and the HTTPS_PROXY / NO_PROXY variables. AWS calls only see the variables."}
          {proxy.mode === "none" && "Every request goes directly to the server."}
          {proxy.mode === "manual" &&
            "Sends, OAuth2 token requests, script requests and AWS calls go through this proxy. Collections can opt out in their settings."}
        </p>
      </Row>
      {proxy.mode === "manual" && (
        <>
          <Row label="Proxy URL" hint="http://host:port, https://host:port or socks5://host:port.">
            <TextSetting
              key={`url:${proxy.url}`}
              value={proxy.url}
              placeholder="http://proxy.example.com:8080"
              onChange={(url) => onChange({ url })}
            />
          </Row>
          <Row label="Username" hint="Leave empty if the proxy needs no login.">
            <TextSetting
              key={`user:${proxy.username}`}
              value={proxy.username}
              placeholder="(none)"
              onChange={(username) => onChange({ username })}
            />
          </Row>
          {proxy.username !== "" && (
            <Row label="Password" hint="Stored in the system keychain.">
              <ProxyPassword />
            </Row>
          )}
          <Row
            label="Don't use the proxy for"
            hint="Host names, *.domain for a domain with its subdomains, IP addresses and ranges like 10.0.0.0/8. Separate them with commas or new lines."
          >
            <Textarea
              key={`bypass:${proxy.bypass}`}
              className="min-h-16 font-mono text-xs"
              defaultValue={proxy.bypass}
              spellCheck={false}
              onBlur={(e) => {
                if (e.target.value !== proxy.bypass) onChange({ bypass: e.target.value });
              }}
            />
          </Row>
        </>
      )}
    </Rows>
  );
}

/** A text setting, saved when leaving the field or pressing Enter. */
function TextSetting({
  value,
  placeholder,
  onChange,
}: {
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
}) {
  const [text, setText] = useState(value);
  const commit = () => {
    if (text.trim() !== value) onChange(text.trim());
  };
  return (
    <Input
      className="h-8 font-mono text-xs"
      value={text}
      placeholder={placeholder}
      spellCheck={false}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
      }}
    />
  );
}

/** The proxy password: shows whether one is stored, never the value. */
function ProxyPassword() {
  const [stored, setStored] = useState<boolean | null>(null);
  const [text, setText] = useState("");
  useEffect(() => {
    void unwrap(commands.hasProxyPassword()).then(setStored, () => setStored(false));
  }, []);
  const save = async (password: string | null) => {
    try {
      await unwrap(commands.setProxyPassword(password));
      setStored(password !== null);
      setText("");
      toast.success(password === null ? "Proxy password removed" : "Proxy password saved");
    } catch (e) {
      showError(e);
    }
  };
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Input
        type="password"
        className="h-8 w-56"
        value={text}
        placeholder={stored ? "•••••••• (saved)" : "Password"}
        autoComplete="off"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && text) void save(text);
        }}
      />
      <Button size="sm" variant="outline" disabled={!text} onClick={() => void save(text)}>
        Save
      </Button>
      {stored && (
        <Button size="sm" variant="ghost" onClick={() => void save(null)}>
          Remove
        </Button>
      )}
    </div>
  );
}

/** Account alias → AWS profile on this machine (PLAN.md M17), from `settings.json`. */
function AwsAccounts() {
  const accounts = useWorkspace((s) => s.settings?.awsAccounts) ?? {};
  const loadSettings = useWorkspace((s) => s.loadSettings);
  const profiles = useAwsProfiles();
  const [alias, setAlias] = useState("");
  const save = async (account: string, profile: string | null) => {
    try {
      await unwrap(commands.setAwsAccountProfile(account, profile));
      await loadSettings();
      // Values fetched with another profile may differ.
      await commands.refreshAwsSecrets();
    } catch (e) {
      showError(e);
    }
  };
  const entries = Object.entries(accounts);
  const profileSelect = (value: string | null, onChange: (p: string) => void, label: string) => (
    <Select value={value ?? undefined} onValueChange={onChange}>
      <SelectTrigger size="sm" className="w-48" aria-label={label}>
        <SelectValue placeholder="Choose a profile" />
      </SelectTrigger>
      <SelectContent>
        {(profiles ?? []).map((p) => (
          <SelectItem key={p.name} value={p.name}>
            {p.name}
            {p.sso ? " (SSO)" : ""}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
  return (
    <div className="flex flex-col gap-2">
      {entries.length === 0 && (
        <p className="text-xs text-muted-foreground">
          None yet. Frida asks the first time a secret uses an alias.
        </p>
      )}
      {entries.map(([account, profile]) => (
        <div key={account} className="flex flex-wrap items-center gap-2">
          <span className="w-40 truncate font-mono text-xs" title={account}>
            {account}
          </span>
          {profileSelect(profile, (p) => void save(account, p), `Profile for ${account}`)}
          <Button size="sm" variant="ghost" onClick={() => void save(account, null)}>
            Remove
          </Button>
        </div>
      ))}
      <div className="flex flex-wrap items-center gap-2">
        <Input
          className="h-8 w-40 font-mono text-xs"
          placeholder="new alias"
          value={alias}
          spellCheck={false}
          onChange={(e) => setAlias(e.target.value)}
        />
        {profileSelect(
          null,
          (p) => {
            if (!alias.trim()) return;
            void save(alias.trim(), p).then(() => setAlias(""));
          },
          "Profile for the new alias",
        )}
      </div>
    </div>
  );
}

/** The running version and "Check for updates" by hand. */
function UpdateCheck() {
  const [version, setVersion] = useState<string | null>(null);
  const phase = useUpdate((s) => s.phase);
  const available = useUpdate((s) => s.available);
  const checkNow = useUpdate((s) => s.checkNow);
  const setDialogOpen = useUpdate((s) => s.setDialogOpen);
  useEffect(() => {
    void commands.getAboutInfo().then((about) => setVersion(about.coreVersion));
  }, []);
  return (
    <div className="flex flex-wrap items-center gap-2">
      <span className="text-sm text-muted-foreground">Frida {version ?? "…"}</span>
      <Button
        size="sm"
        variant="outline"
        disabled={phase !== "idle"}
        onClick={() => void checkNow()}
      >
        {phase === "checking" && <Loader2 className="animate-spin" />}
        Check for updates
      </Button>
      {available && (
        <Button size="sm" onClick={() => setDialogOpen(true)}>
          Frida {available.version} available…
        </Button>
      )}
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
