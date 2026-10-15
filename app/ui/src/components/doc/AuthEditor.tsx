import { KeyRound, Loader2, Plus, RefreshCw, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import {
  type ApiKeyLocation,
  type Auth,
  type AuthScheme,
  commands,
  type OAuth2ClientAuth,
  type OAuth2Config,
  type OAuth2Grant,
  type TokenInfo,
} from "@/bindings";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { showError, unwrap } from "@/lib/api";
import type { RequestModel } from "@/lib/model";

type SchemeType = AuthScheme["type"];
type Mode = "inherit" | "none" | SchemeType;

/** Where the edited auth lives, for "Get new token". */
export interface AuthContext {
  dir: string;
  /** The request, folder or `""` (collection) path. */
  path: string;
  /** The editor's current request, for request tabs. */
  request: RequestModel | null;
}

interface AuthEditorProps {
  auth: Auth;
  onChange: (auth: Auth) => void;
  /** Collections have no parent to inherit from. */
  canInherit: boolean;
  context: AuthContext;
}

const MODES: { value: Mode; label: string }[] = [
  { value: "inherit", label: "Inherit from parent" },
  { value: "none", label: "No auth" },
  { value: "basic", label: "Basic auth" },
  { value: "bearer", label: "Bearer token" },
  { value: "apikey", label: "API key" },
  { value: "oauth2", label: "OAuth 2.0" },
];

function newScheme(type: SchemeType): AuthScheme {
  switch (type) {
    case "basic":
      return { type, username: "", password: "" };
    case "bearer":
      return { type, token: "" };
    case "apikey":
      return { type, in: "header", name: "x-api-key", value: "" };
    case "oauth2":
      return {
        type,
        grant: "client_credentials",
        tokenUrl: "",
        authUrl: "",
        redirectUri: "",
        clientId: "",
        clientSecret: "",
        scope: "",
        usePkce: true,
        username: "",
        password: "",
        clientAuth: "basic_header",
        tokenPlacement: { in: "header", name: "Authorization", prefix: "Bearer" },
        preset: null,
      };
  }
}

/**
 * The Auth tab (PLAN.md §4.5): a primary scheme plus any number of API keys, applied in order,
 * so e.g. OAuth2 + API key sends both. `{{variables}}` work in every field.
 */
export function AuthEditor({ auth, onChange, canInherit, context }: AuthEditorProps) {
  const schemes = auth.mode === "custom" ? auth.schemes : [];
  // The primary scheme is the first one; extra API keys follow it.
  const mode: Mode = auth.mode === "custom" ? (schemes[0]?.type ?? "none") : auth.mode;

  const setMode = (next: Mode) => {
    if (next === "inherit" || next === "none") {
      onChange({ mode: next });
      return;
    }
    const extras = schemes.slice(1).filter((s) => s.type === "apikey");
    onChange({ mode: "custom", schemes: [newScheme(next), ...extras] });
  };
  const setSchemes = (next: AuthScheme[]) =>
    onChange(next.length === 0 ? { mode: "none" } : { mode: "custom", schemes: next });
  const updateScheme = (index: number, scheme: AuthScheme) =>
    setSchemes(schemes.map((s, i) => (i === index ? scheme : s)));

  return (
    <div className="flex max-w-3xl flex-col gap-4 text-sm">
      <Select value={mode} onValueChange={(v) => setMode(v as Mode)}>
        <SelectTrigger size="sm" className="w-60">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {MODES.filter((m) => canInherit || m.value !== "inherit").map((m) => (
            <SelectItem key={m.value} value={m.value}>
              {m.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {mode === "inherit" && (
        <p className="text-xs text-muted-foreground">
          Uses the auth of the nearest folder that defines one, or the collection's.
        </p>
      )}

      {schemes.map((scheme, index) => (
        <section key={index} className="rounded-md border p-3">
          <div className="mb-3 flex items-center justify-between">
            <span className="font-medium">
              {index === 0
                ? MODES.find((m) => m.value === scheme.type)?.label
                : "Additional API key"}
            </span>
            {index > 0 && (
              <Button
                variant="ghost"
                size="icon"
                className="size-7"
                aria-label="Remove"
                onClick={() => setSchemes(schemes.filter((_, i) => i !== index))}
              >
                <Trash2 />
              </Button>
            )}
          </div>
          <SchemeFields
            scheme={scheme}
            onChange={(s) => updateScheme(index, s)}
            context={context}
          />
        </section>
      ))}

      {auth.mode === "custom" && (
        <Button
          variant="outline"
          size="sm"
          className="self-start"
          onClick={() => setSchemes([...schemes, newScheme("apikey")])}
        >
          <Plus /> Add API key
        </Button>
      )}
    </div>
  );
}

function SchemeFields({
  scheme,
  onChange,
  context,
}: {
  scheme: AuthScheme;
  onChange: (s: AuthScheme) => void;
  context: AuthContext;
}) {
  switch (scheme.type) {
    case "basic":
      return (
        <Fields>
          <Field label="Username">
            <TextInput
              value={scheme.username}
              onChange={(username) => onChange({ ...scheme, username })}
            />
          </Field>
          <Field label="Password">
            <TextInput
              secret
              value={scheme.password}
              onChange={(password) => onChange({ ...scheme, password })}
            />
          </Field>
        </Fields>
      );
    case "bearer":
      return (
        <Fields>
          <Field label="Token">
            <TextInput
              secret
              value={scheme.token}
              onChange={(token) => onChange({ ...scheme, token })}
            />
          </Field>
        </Fields>
      );
    case "apikey":
      return (
        <Fields>
          <Field label="Add to">
            <LocationSelect
              value={scheme.in ?? "header"}
              onChange={(location) => onChange({ ...scheme, in: location })}
            />
          </Field>
          <Field label="Name">
            <TextInput value={scheme.name} onChange={(name) => onChange({ ...scheme, name })} />
          </Field>
          <Field label="Value">
            <TextInput
              secret
              value={scheme.value}
              onChange={(value) => onChange({ ...scheme, value })}
            />
          </Field>
        </Fields>
      );
    case "oauth2":
      return (
        <OAuth2Fields
          config={scheme}
          onChange={(c) => onChange({ ...c, type: "oauth2" })}
          context={context}
        />
      );
  }
}

function OAuth2Fields({
  config,
  onChange,
  context,
}: {
  config: OAuth2Config;
  onChange: (c: OAuth2Config) => void;
  context: AuthContext;
}) {
  const grant = config.grant ?? "client_credentials";
  const placement = config.tokenPlacement ?? {
    in: "header",
    name: "Authorization",
    prefix: "Bearer",
  };
  const set = (patch: Partial<OAuth2Config>) => onChange({ ...config, ...patch });

  return (
    <div className="flex flex-col gap-4">
      <Fields>
        <Field label="Grant type">
          <Select value={grant} onValueChange={(g) => set({ grant: g as OAuth2Grant })}>
            <SelectTrigger size="sm" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="client_credentials">Client credentials</SelectItem>
              <SelectItem value="authorization_code">Authorization code</SelectItem>
              <SelectItem value="password">Password</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        {grant === "authorization_code" && (
          <>
            <Field label="Authorization URL">
              <TextInput value={config.authUrl} onChange={(authUrl) => set({ authUrl })} />
            </Field>
            <Field label="Redirect URI">
              <TextInput
                value={config.redirectUri}
                placeholder="http://127.0.0.1:{port}/callback"
                onChange={(redirectUri) => set({ redirectUri })}
              />
            </Field>
            <Field label="Use PKCE">
              <Checkbox
                checked={config.usePkce ?? true}
                onCheckedChange={(c) => set({ usePkce: c === true })}
              />
            </Field>
          </>
        )}
        <Field label="Token URL">
          <TextInput value={config.tokenUrl} onChange={(tokenUrl) => set({ tokenUrl })} />
        </Field>
        <Field label="Client ID">
          <TextInput value={config.clientId} onChange={(clientId) => set({ clientId })} />
        </Field>
        <Field label="Client secret">
          <TextInput
            secret
            value={config.clientSecret}
            onChange={(clientSecret) => set({ clientSecret })}
          />
        </Field>
        <Field label="Scope">
          <TextInput
            value={config.scope}
            placeholder="space separated"
            onChange={(scope) => set({ scope })}
          />
        </Field>
        {grant === "password" && (
          <>
            <Field label="Username">
              <TextInput value={config.username} onChange={(username) => set({ username })} />
            </Field>
            <Field label="Password">
              <TextInput
                secret
                value={config.password}
                onChange={(password) => set({ password })}
              />
            </Field>
          </>
        )}
        <Field label="Client authentication">
          <Select
            value={config.clientAuth ?? "basic_header"}
            onValueChange={(v) => set({ clientAuth: v as OAuth2ClientAuth })}
          >
            <SelectTrigger size="sm" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="basic_header">Basic auth header</SelectItem>
              <SelectItem value="body">In the request body</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        <Field label="Send token in">
          <div className="flex gap-2">
            <LocationSelect
              value={placement.in ?? "header"}
              onChange={(location) => set({ tokenPlacement: { ...placement, in: location } })}
            />
            <TextInput
              value={placement.name}
              placeholder="Authorization"
              onChange={(name) => set({ tokenPlacement: { ...placement, name } })}
            />
            {(placement.in ?? "header") === "header" && (
              <TextInput
                value={placement.prefix}
                placeholder="Bearer"
                onChange={(prefix) => set({ tokenPlacement: { ...placement, prefix } })}
              />
            )}
          </div>
        </Field>
      </Fields>
      <TokenPanel context={context} />
    </div>
  );
}

/** "Get new token" plus the cached tokens (PLAN.md §8 token viewer). */
function TokenPanel({ context }: { context: AuthContext }) {
  const [tokens, setTokens] = useState<TokenInfo[] | null>(null);
  const [requestId, setRequestId] = useState<string | null>(null);
  const [shown, setShown] = useState<string | null>(null);

  const fetchNew = async () => {
    const id = crypto.randomUUID();
    setRequestId(id);
    try {
      setTokens(
        await unwrap(commands.fetchOauth2Tokens(id, context.dir, context.path, context.request)),
      );
      toast.success("New token received");
    } catch (e) {
      showError(e);
    } finally {
      setRequestId(null);
    }
  };
  const refreshList = async () => setTokens(await commands.listOauth2Tokens());
  const clear = async (token: TokenInfo | null) => {
    await commands.clearOauth2Tokens(token);
    await refreshList();
  };

  return (
    <div className="flex flex-col gap-2 border-t pt-3">
      <div className="flex items-center gap-2">
        {requestId ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() => void commands.cancelRequest(requestId)}
          >
            <Loader2 className="animate-spin" /> Waiting… Cancel
          </Button>
        ) : (
          <Button size="sm" onClick={() => void fetchNew()}>
            <KeyRound /> Get new token
          </Button>
        )}
        <Button size="sm" variant="outline" onClick={() => void refreshList()}>
          <RefreshCw /> {tokens === null ? "Show tokens" : "Refresh"}
        </Button>
        {tokens !== null && tokens.length > 0 && (
          <Button size="sm" variant="ghost" onClick={() => void clear(null)}>
            Clear all tokens
          </Button>
        )}
      </div>
      {tokens?.length === 0 && <p className="text-xs text-muted-foreground">No tokens yet.</p>}
      {tokens?.map((t) => {
        const id = `${t.tokenUrl}|${t.clientId}|${t.scope}|${t.grant}|${t.username}`;
        return (
          <div key={id} className="rounded-md border p-2 text-xs">
            <div className="flex items-center gap-2">
              <span className="font-medium">{t.clientId}</span>
              <span className="text-muted-foreground">{t.grant.replace("_", " ")}</span>
              {t.scope && <span className="text-muted-foreground">scope: {t.scope}</span>}
              <div className="flex-1" />
              <span className={t.expired ? "text-destructive" : "text-muted-foreground"}>
                {t.expiresAt ? `${t.expired ? "expired" : "expires"} ${t.expiresAt}` : "no expiry"}
              </span>
              <Button variant="ghost" size="sm" className="h-6" onClick={() => void clear(t)}>
                Clear
              </Button>
            </div>
            <button
              type="button"
              className="mt-1 block w-full truncate text-left font-mono text-muted-foreground hover:text-foreground"
              title="Click to show or hide the token"
              onClick={() => setShown(shown === id ? null : id)}
            >
              {shown === id ? t.accessToken : `${t.accessToken.slice(0, 12)}…`}
            </button>
            {shown === id && t.claims && (
              <pre className="mt-1 max-h-48 overflow-auto rounded bg-muted p-2 font-mono">
                {t.claims}
              </pre>
            )}
          </div>
        );
      })}
    </div>
  );
}

function Fields({ children }: { children: React.ReactNode }) {
  return <div className="grid grid-cols-[10rem_1fr] items-center gap-x-3 gap-y-2">{children}</div>;
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <>
      <span className="text-xs text-muted-foreground">{label}</span>
      <div>{children}</div>
    </>
  );
}

function TextInput({
  value,
  onChange,
  placeholder,
  secret = false,
}: {
  value: string | undefined;
  onChange: (value: string) => void;
  placeholder?: string;
  secret?: boolean;
}) {
  const [visible, setVisible] = useState(false);
  // Values that are a single {{variable}} are safe to show; literal secrets are hidden.
  const masked = secret && !visible && !(value ?? "").trim().startsWith("{{");
  return (
    <Input
      className="h-8 font-mono text-xs"
      type={masked ? "password" : "text"}
      value={value ?? ""}
      placeholder={placeholder ?? (secret ? "{{secretVariable}} recommended" : undefined)}
      onFocus={() => setVisible(true)}
      onBlur={() => setVisible(false)}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

function LocationSelect({
  value,
  onChange,
}: {
  value: ApiKeyLocation;
  onChange: (v: ApiKeyLocation) => void;
}) {
  return (
    <Select value={value} onValueChange={(v) => onChange(v as ApiKeyLocation)}>
      <SelectTrigger size="sm" className="w-28">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="header">Header</SelectItem>
        <SelectItem value="query">Query</SelectItem>
      </SelectContent>
    </Select>
  );
}
