import { useEffect, useMemo, useState } from "react";
import { commands, type ResolveReport } from "@/bindings";
import type { VariableContext } from "@/components/editor/variables";
import type { RequestModel } from "@/lib/model";
import { useEnvs } from "@/stores/envs";

/** How long to wait after the last edit before asking Rust to resolve variables again. */
const DEBOUNCE_MS = 250;

/**
 * Resolves the variables of the request being edited (debounced), whenever it or the active
 * environments change. Returns the report and the variable context for the editors
 * (`null` until the first report arrives).
 *
 * For folder and collection tabs pass `request: null` and the folder path (`""` for the
 * collection): the report then lists the variables available inside it.
 */
export function useInspection(dir: string, path: string, request: RequestModel | null) {
  const envVersion = useEnvs((s) => s.version);
  const [report, setReport] = useState<ResolveReport | null>(null);

  useEffect(() => {
    let cancelled = false;
    const timer = setTimeout(() => {
      void commands.inspectRequest(dir, path, request).then((result) => {
        // Errors (e.g. a deleted file) just leave the last report in place.
        if (!cancelled && result.status === "ok") setReport(result.data);
      });
    }, DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [dir, path, request, envVersion]);

  const pathParamsKey = JSON.stringify(request?.params.path ?? []);
  const variables: VariableContext | null = useMemo(
    () =>
      report && {
        used: new Map(report.vars.map((v) => [v.name, v])),
        available: new Map((report.available ?? []).map((v) => [v.name, v])),
        pathParams: new Map(
          (JSON.parse(pathParamsKey) as { name: string; value: string }[]).map((p) => [
            p.name,
            p.value,
          ]),
        ),
      },
    [report, pathParamsKey],
  );
  return { report, variables };
}
