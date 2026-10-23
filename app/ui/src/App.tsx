import { useEffect } from "react";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AppShell } from "@/components/workspace/AppShell";
import { StartScreen } from "@/components/workspace/StartScreen";
import { SettingsDialog } from "@/components/settings/SettingsDialog";
import { UpdateDialog } from "@/components/update/UpdateDialog";
import { applyAppearance, applyTheme } from "@/lib/theme";
import { useUpdate } from "@/stores/update";
import { useWorkspace } from "@/stores/workspace";

export default function App() {
  const hasWorkspace = useWorkspace((s) => s.workspace !== null);
  const reopening = useWorkspace((s) => s.reopening);
  const start = useWorkspace((s) => s.start);
  const preferences = useWorkspace((s) => s.settings?.preferences);
  const theme = preferences?.theme ?? "system";

  useEffect(() => applyTheme(theme), [theme]);
  useEffect(() => {
    if (preferences) applyAppearance(preferences);
  }, [preferences]);
  useEffect(() => {
    void start();
  }, [start]);
  // Rust decides whether the check is due (switched on, once a day, not in dev builds).
  useEffect(() => {
    void useUpdate.getState().checkOnStart();
  }, []);

  return (
    <TooltipProvider>
      {/* Nothing while the last workspace opens again, so the start screen doesn't flash. */}
      {hasWorkspace ? <AppShell /> : !reopening && <StartScreen />}
      <SettingsDialog />
      <UpdateDialog />
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}
