import { useState } from "react";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { showError, unwrap } from "@/lib/api";
import { useDialogs } from "@/stores/dialogs";
import { scopeOf } from "@/stores/envs";
import { cn } from "cn";
import { useAwsProfiles } from "./useAwsProfiles";

/**
 * Asks which AWS profile an environment's AWS secrets are read with (first use, PLAN.md §9.1).
 * The choice is remembered locally and can be changed in the environment editor.
 */
export function AwsProfileDialog() {
  const request = useDialogs((s) => s.awsProfile);
  const close = useDialogs((s) => s.close);
  return (
    <Dialog open={request !== null} onOpenChange={(o) => !o && close()}>
      <DialogContent className="sm:max-w-md">
        {request && <ProfileForm key={request.environment} />}
      </DialogContent>
    </Dialog>
  );
}

function ProfileForm() {
  const request = useDialogs((s) => s.awsProfile);
  const close = useDialogs((s) => s.close);
  const profiles = useAwsProfiles();
  const [chosen, setChosen] = useState<string | null>(null);
  if (!request) return null;

  const confirm = async () => {
    if (!chosen) return;
    try {
      const scope = scopeOf(request.global ? "global" : "collection", request.dir);
      await unwrap(commands.setAwsProfile(scope, request.environment, chosen));
      close();
      request.then();
    } catch (e) {
      showError(e);
    }
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>Choose an AWS profile</DialogTitle>
        <DialogDescription>
          Environment “{request.environment}” reads secrets from AWS Secrets Manager. Which profile
          should Frida use? You can change it later in the environment editor.
        </DialogDescription>
      </DialogHeader>
      {profiles === null ? (
        <p className="text-sm text-muted-foreground">Reading ~/.aws/config…</p>
      ) : profiles.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No profiles found in ~/.aws/config or ~/.aws/credentials. Set one up with{" "}
          <code>aws configure</code> or <code>aws configure sso</code>.
        </p>
      ) : (
        <div className="flex max-h-64 flex-col gap-1 overflow-auto" role="listbox">
          {profiles.map((p) => (
            <button
              key={p.name}
              type="button"
              role="option"
              aria-selected={chosen === p.name}
              onClick={() => setChosen(p.name)}
              onDoubleClick={() => {
                setChosen(p.name);
                void confirm();
              }}
              className={cn(
                "flex items-center justify-between rounded px-2 py-1.5 text-left text-sm hover:bg-accent",
                chosen === p.name && "bg-accent font-medium",
              )}
            >
              <span>{p.name}</span>
              <span className="text-xs text-muted-foreground">
                {[p.sso && "SSO", p.region].filter(Boolean).join(" · ")}
              </span>
            </button>
          ))}
        </div>
      )}
      <DialogFooter>
        <Button variant="ghost" onClick={close}>
          Cancel
        </Button>
        <Button disabled={!chosen} onClick={() => void confirm()}>
          Use profile and send
        </Button>
      </DialogFooter>
    </>
  );
}
