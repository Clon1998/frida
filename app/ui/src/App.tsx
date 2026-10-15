import { useEffect } from "react";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AppShell } from "@/components/workspace/AppShell";
import { StartScreen } from "@/components/workspace/StartScreen";
import { followSystemTheme } from "@/lib/theme";
import { useWorkspace } from "@/stores/workspace";

export default function App() {
  const hasWorkspace = useWorkspace((s) => s.workspace !== null);
  const loadSettings = useWorkspace((s) => s.loadSettings);

  useEffect(() => followSystemTheme(), []);
  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  return (
    <TooltipProvider>
      {hasWorkspace ? <AppShell /> : <StartScreen />}
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}
