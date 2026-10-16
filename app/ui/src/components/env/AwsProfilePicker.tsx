import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { commands, type EnvScope } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { showError, unwrap } from "@/lib/api";
import { useEnvs } from "@/stores/envs";
import { useAwsProfiles } from "./useAwsProfiles";

const NONE = "__none__";

/**
 * The AWS profile used for an environment's AWS secrets (PLAN.md §9.1), plus "refresh secrets".
 * The choice is personal, so it lives in local state, not in the environment file.
 */
export function AwsProfilePicker({ scope, environment }: { scope: EnvScope; environment: string }) {
  const profiles = useAwsProfiles();
  const [profile, setProfile] = useState<string | null>(null);

  useEffect(() => {
    void unwrap(commands.getAwsProfile(scope, environment)).then(setProfile, showError);
  }, [scope, environment]);

  const choose = async (value: string) => {
    const next = value === NONE ? null : value;
    try {
      await unwrap(commands.setAwsProfile(scope, environment, next));
      setProfile(next);
      // Values fetched with another profile may differ.
      await commands.refreshAwsSecrets();
      useEnvs.setState((s) => ({ version: s.version + 1 }));
    } catch (e) {
      showError(e);
    }
  };

  return (
    <div className="flex items-center gap-2 text-sm">
      <span className="text-muted-foreground">AWS profile</span>
      <Select value={profile ?? NONE} onValueChange={(v) => void choose(v)}>
        <SelectTrigger size="sm" className="w-56">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={NONE}>Not chosen</SelectItem>
          {(profiles ?? []).map((p) => (
            <SelectItem key={p.name} value={p.name}>
              {p.name}
              {p.sso ? " (SSO)" : ""}
              {p.region ? ` · ${p.region}` : ""}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {profiles?.length === 0 && (
        <span className="text-xs text-muted-foreground">No profiles in ~/.aws/config.</span>
      )}
      <Button
        variant="ghost"
        size="sm"
        title="Forget cached AWS values (they are kept 15 minutes)"
        onClick={() =>
          void commands.refreshAwsSecrets().then(() => {
            useEnvs.setState((s) => ({ version: s.version + 1 }));
            toast.success("AWS secrets will be fetched again on the next send");
          })
        }
      >
        <RefreshCw /> Refresh secrets
      </Button>
    </div>
  );
}
