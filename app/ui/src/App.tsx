import { useEffect } from "react";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AppShell } from "@/components/workspace/AppShell";
import { StartScreen } from "@/components/workspace/StartScreen";
import { SettingsDialog } from "@/components/settings/SettingsDialog";
import { applyAppearance, applyTheme } from "@/lib/theme";
import { useWorkspace } from "@/stores/workspace";

export default function App() {
  const hasWorkspace = useWorkspace((s) => s.workspace !== null);
  const loadSettings = useWorkspace((s) => s.loadSettings);
  const preferences = useWorkspace((s) => s.settings?.preferences);
  const theme = preferences?.theme ?? "system";

  useEffect(() => applyTheme(theme), [theme]);
  useEffect(() => {
    if (preferences) applyAppearance(preferences);
  }, [preferences]);
  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  return (
    <TooltipProvider>
      {hasWorkspace ? <AppShell /> : <StartScreen />}
      <SettingsDialog />
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}
