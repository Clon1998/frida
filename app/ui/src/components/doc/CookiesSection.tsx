import { RefreshCw, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { commands, type CookieView } from "@/bindings";
import { Button } from "@/components/ui/button";
import { showError, unwrap } from "@/lib/api";

/** The collection's cookie jar (PLAN.md §7): what will be sent, per domain, with clear buttons. */
export function CookiesSection({ dir }: { dir: string }) {
  const [cookies, setCookies] = useState<CookieView[]>([]);

  const reload = useCallback(async () => {
    try {
      setCookies(await unwrap(commands.listCookies(dir)));
    } catch (e) {
      showError(e);
    }
  }, [dir]);

  useEffect(() => {
    let cancelled = false;
    void unwrap(commands.listCookies(dir)).then(
      (list) => !cancelled && setCookies(list),
      showError,
    );
    return () => {
      cancelled = true;
    };
  }, [dir]);

  const clear = async (domain: string | null) => {
    try {
      await unwrap(commands.clearCookies(dir, domain));
      await reload();
    } catch (e) {
      showError(e);
    }
  };

  const domains = [...new Set(cookies.map((c) => c.domain))];

  return (
    <div className="flex flex-col gap-3 text-sm">
      <div className="flex items-center gap-2">
        <p className="flex-1 text-xs text-muted-foreground">
          Cookies set by responses are sent with later requests to the same site. They are kept in
          memory until Frida closes.
        </p>
        <Button variant="outline" size="sm" onClick={() => void reload()}>
          <RefreshCw /> Refresh
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={cookies.length === 0}
          onClick={() => void clear(null)}
        >
          Clear all
        </Button>
      </div>
      {cookies.length === 0 && <p className="text-xs text-muted-foreground">No cookies yet.</p>}
      {domains.map((domain) => (
        <section key={domain} className="rounded-md border">
          <div className="flex items-center justify-between border-b bg-muted/40 px-3 py-1.5">
            <span className="font-medium">{domain}</span>
            <Button variant="ghost" size="sm" className="h-6" onClick={() => void clear(domain)}>
              <Trash2 /> Clear
            </Button>
          </div>
          <table className="w-full text-xs">
            <tbody>
              {cookies
                .filter((c) => c.domain === domain)
                .map((c) => (
                  <tr key={`${c.path}:${c.name}`} className="border-b last:border-0">
                    <td className="px-3 py-1 font-mono font-medium">{c.name}</td>
                    <td className="max-w-80 truncate px-3 py-1 font-mono" title={c.value}>
                      {c.value}
                    </td>
                    <td className="px-3 py-1 text-muted-foreground">{c.path}</td>
                    <td className="px-3 py-1 text-muted-foreground">{c.expires ?? "session"}</td>
                    <td className="px-3 py-1 text-muted-foreground">
                      {[c.secure && "Secure", c.httpOnly && "HttpOnly"].filter(Boolean).join(" ")}
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </section>
      ))}
    </div>
  );
}
