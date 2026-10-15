import { useEffect, useMemo, useState } from "react";
import { commands, type ResolveReport } from "@/bindings";
import type { VariableMap } from "@/components/editor/variables";
import type { RequestModel } from "@/lib/model";
import { useEnvs } from "@/stores/envs";

/** How long to wait after the last edit before asking Rust to resolve variables again. */
const DEBOUNCE_MS = 250;

/**
 * Resolves the variables of the request being edited (debounced), whenever it or the active
 * environments change. Returns the report and a name → variable map for the editors.
 */
export function useInspection(dir: string, path: string, request: RequestModel) {
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

  const variables: VariableMap = useMemo(
    () => new Map((report?.vars ?? []).map((v) => [v.name, v])),
    [report],
  );
  return { report, variables };
}
