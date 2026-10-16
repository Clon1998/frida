import { useEffect, useState } from "react";
import { type AwsProfile, commands } from "@/bindings";
import { showError, unwrap } from "@/lib/api";

/** Loads the AWS profiles from `~/.aws/config` and `~/.aws/credentials` (`null` while loading). */
export function useAwsProfiles() {
  const [profiles, setProfiles] = useState<AwsProfile[] | null>(null);
  useEffect(() => {
    void unwrap(commands.listAwsProfiles()).then(setProfiles, (e) => {
      showError(e);
      setProfiles([]);
    });
  }, []);
  return profiles;
}
