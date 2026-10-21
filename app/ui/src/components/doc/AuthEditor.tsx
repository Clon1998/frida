import {
  ChevronDown,
  ChevronRight,
  Copy,
  KeyRound,
  Loader2,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { type CognitoDomain, cognitoUrls, parseCognito } from "@/lib/cognito";
import { useContext, useId, useState } from "react";
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
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { VariableInput } from "@/components/editor/VariableInput";
import { VariablesContext } from "@/components/editor/variables";
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
import { expiryLabel, timeAgo } from "@/lib/time";
import { cn } from "cn";

type SchemeType = AuthScheme["type"];
type Mode = "inherit" | "none" | SchemeType;

/** Where the edited auth lives, for "Get new token". */
export interface AuthContext {
  dir: string;
  /** The request, folder or `""` (collection) path. */
  path: string;
  /** The editor's current request, for request tabs. */
  request: RequestModel | null;
  /**
   * The auth as edited, unsaved values included: "Get new token" uses it instead of the saved
   * auth (folders and the collection have no `request`). Set by the AuthEditor itself.
   */
  auth?: Auth;
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
            context={{ ...context, auth }}
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
  const cognito = config.preset === "cognito";
  // Kept as typed (re-parsing a half-typed URL would scramble the fields).
  const [cognitoDomain, setCognitoDomain] = useState(() => parseCognito(config.tokenUrl ?? ""));
  /** Cognito preset: domain and region fill in the endpoints (PLAN.md §9.2). */
  const setCognito = (patch: Partial<CognitoDomain>) => {
    const next = { ...cognitoDomain, ...patch };
    setCognitoDomain(next);
    set({ preset: "cognito", ...cognitoUrls(next) });
  };

  return (
    <div className="flex flex-col gap-4">
      <Fields>
        <Field label="Preset">
          <Select
            value={cognito ? "cognito" : "none"}
            onValueChange={(p) => set({ preset: p === "cognito" ? "cognito" : null })}
          >
            <SelectTrigger size="sm" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="none">None</SelectItem>
              <SelectItem value="cognito">AWS Cognito</SelectItem>
            </SelectContent>
          </Select>
        </Field>
        {cognito && (
          <>
            <Field label="User pool domain">
              <TextInput
                value={cognitoDomain.domain}
                placeholder="my-app (prefix) or auth.example.com"
                onChange={(domain) => setCognito({ domain })}
              />
            </Field>
            <Field label="Region">
              <TextInput
                value={cognitoDomain.region}
                placeholder="eu-central-1"
                onChange={(region) => setCognito({ region })}
              />
            </Field>
          </>
        )}
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
          {/* Wraps in narrow panes instead of squeezing the inputs to nothing. */}
          <div className="flex flex-wrap gap-2">
            <LocationSelect
              value={placement.in ?? "header"}
              onChange={(location) => set({ tokenPlacement: { ...placement, in: location } })}
            />
            <div className="min-w-28 flex-1">
              <TextInput
                value={placement.name}
                placeholder="Authorization"
                onChange={(name) => set({ tokenPlacement: { ...placement, name } })}
              />
            </div>
            {(placement.in ?? "header") === "header" && (
              <div className="min-w-28 flex-1">
                <TextInput
                  value={placement.prefix}
                  placeholder="Bearer"
                  onChange={(prefix) => set({ tokenPlacement: { ...placement, prefix } })}
                />
              </div>
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
  const [othersOpen, setOthersOpen] = useState(false);
  const { dir, path, request } = context;
  const auth = context.auth ?? null;
  const item = request ? "this request" : path === "" ? "the collection" : "this folder";

  const fetchNew = async () => {
    const id = crypto.randomUUID();
    setRequestId(id);
    try {
      setTokens(await unwrap(commands.fetchOauth2Tokens(id, dir, path, request, auth)));
      toast.success("New token received");
    } catch (e) {
      // The token request itself, with the provider's answer, is in the console.
      showError(e);
    } finally {
      setRequestId(null);
    }
  };
  const refreshList = async () => {
    try {
      setTokens(await unwrap(commands.listOauth2Tokens(dir, path, request, auth)));
    } catch (e) {
      showError(e);
    }
  };
  const clear = async (token: TokenInfo | null) => {
    await commands.clearOauth2Tokens(token);
    await refreshList();
  };

  const current = tokens?.filter((t) => t.current) ?? [];
  const others = tokens?.filter((t) => !t.current) ?? [];

  return (
    <div className="flex flex-col gap-2 border-t pt-3">
      <div className="flex flex-wrap items-center gap-2">
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
      {tokens !== null && (
        <>
          <p className="text-xs font-medium text-muted-foreground">
            For {item}, with the active environments
          </p>
          {current.length === 0 && (
            <p className="text-xs text-muted-foreground">
              No token yet. &quot;Get new token&quot; fetches one, and so does sending.
            </p>
          )}
          {current.map((t) => (
            <TokenCard key={tokenId(t)} token={t} onClear={() => void clear(t)} />
          ))}
          {others.length > 0 && (
            <>
              <Button
                size="sm"
                variant="ghost"
                className="self-start px-1 text-muted-foreground"
                aria-expanded={othersOpen}
                onClick={() => setOthersOpen(!othersOpen)}
              >
                {othersOpen ? <ChevronDown /> : <ChevronRight />} Other tokens ({others.length})
              </Button>
              {othersOpen &&
                others.map((t) => (
                  <TokenCard key={tokenId(t)} token={t} onClear={() => void clear(t)} />
                ))}
            </>
          )}
        </>
      )}
    </div>
  );
}

const tokenId = (t: TokenInfo) => `${t.tokenUrl}|${t.clientId}|${t.scope}|${t.grant}|${t.username}`;

/**
 * One cached token: where it came from (environment, token URL), its expiry, and the token
 * itself, with explicit Copy and Details (full token and decoded JWT claims) buttons.
 */
function TokenCard({ token: t, onClear }: { token: TokenInfo; onClear: () => void }) {
  const [details, setDetails] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(t.accessToken);
    toast.success("Token copied");
  };
  return (
    <div className="rounded-md border p-2 text-xs" data-testid="oauth-token">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <Badge variant="outline" title="The active environments when the token was fetched">
          {t.environment ?? "no environment"}
        </Badge>
        <span className="font-medium">{t.clientId}</span>
        <span className="text-muted-foreground">{t.grant.replaceAll("_", " ")}</span>
        {t.scope && <span className="text-muted-foreground">scope: {t.scope}</span>}
        <span
          className={cn("ml-auto", t.expired ? "text-destructive" : "text-muted-foreground")}
          title={t.expiresAt ?? undefined}
        >
          {expiryLabel(t.expiresAt)}
        </span>
      </div>
      <p className="mt-1 truncate text-muted-foreground" title={t.tokenUrl}>
        {t.tokenUrl}
      </p>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <code className="min-w-0 flex-1 truncate font-mono text-muted-foreground">
          {t.accessToken}
        </code>
        <Button variant="ghost" size="sm" className="h-6" onClick={() => void copy()}>
          <Copy /> Copy
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-6"
          aria-expanded={details}
          onClick={() => setDetails(!details)}
        >
          {details ? <ChevronDown /> : <ChevronRight />} {t.claims ? "Decode" : "Details"}
        </Button>
        <Button variant="ghost" size="sm" className="h-6" onClick={onClear}>
          Clear
        </Button>
      </div>
      {details && (
        <div className="mt-1 flex flex-col gap-1">
          <p className="text-muted-foreground">
            Fetched {timeAgo(t.fetchedAt)} · {t.tokenType}
            {t.hasRefreshToken && " · has a refresh token"}
          </p>
          <pre className="max-h-24 overflow-auto rounded bg-muted p-2 font-mono break-all whitespace-pre-wrap">
            {t.accessToken}
          </pre>
          {t.claims ? (
            <pre
              aria-label="JWT claims"
              className="max-h-64 overflow-auto rounded bg-muted p-2 font-mono"
            >
              {t.claims}
            </pre>
          ) : (
            <p className="text-muted-foreground">Not a JWT, so there are no claims to decode.</p>
          )}
        </div>
      )}
    </div>
  );
}

function Fields({ children }: { children: React.ReactNode }) {
  // Labels above the fields while the form is narrow, beside them from 28rem (a container
  // query: it follows the pane's width, not the window's). `minmax(0, 1fr)`: a plain `1fr`
  // column never gets narrower than its content (a long URL) and would overflow the card.
  return (
    <div className="@container">
      <div className="grid grid-cols-[minmax(0,1fr)] gap-x-3 gap-y-1 @md:grid-cols-[10rem_minmax(0,1fr)] @md:items-center @md:gap-y-2">
        {children}
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  // The field is named after its label (`role="group"` + `aria-labelledby`), so screen readers
  // (and the UI tests) find "Token URL" etc. even though the editors aren't plain inputs.
  const id = useId();
  return (
    <>
      <span id={id} className="pt-1 text-xs text-muted-foreground @md:pt-0">
        {label}
      </span>
      <div className="min-w-0" role="group" aria-labelledby={id}>
        {children}
      </div>
    </>
  );
}

/**
 * A form field for values that may contain `{{variables}}`: inside editor tabs a
 * {@link VariableInput} (highlighting, `{{` autocompletion), elsewhere a plain input. Secret
 * fields with a literal value show dots while not being edited.
 */
function TextInput({
  value = "",
  onChange,
  placeholder,
  secret = false,
}: {
  value: string | undefined;
  onChange: (value: string) => void;
  placeholder?: string;
  secret?: boolean;
}) {
  const variables = useContext(VariablesContext);
  const [focused, setFocused] = useState(false);
  const hint = placeholder ?? (secret ? "{{secretVariable}} recommended" : undefined);

  if (variables !== undefined) {
    return (
      <VariableInput
        className="min-h-8 font-mono text-xs md:text-xs"
        value={value}
        placeholder={hint}
        secret={secret}
        onChange={onChange}
      />
    );
  }
  // Outside editor tabs (see VariablesContext). One element; only its type changes.
  const masked = secret && !focused && !value.trim().startsWith("{{");
  return (
    <Input
      className="h-8 font-mono text-xs md:text-xs"
      type={masked ? "password" : "text"}
      value={value}
      placeholder={hint}
      onFocus={() => setFocused(true)}
      onBlur={() => setFocused(false)}
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
