import { useEffect, useState } from "react";
import { checkAccessibilityPermission } from "./lib/permissions";
import { Onboarding } from "./views/Onboarding";
import { Settings } from "./views/Settings";

type View = "loading" | "onboarding" | "settings";

/**
 * Phase 1 view router. Decides between onboarding and settings based on the
 * Accessibility permission, then renders the right view.
 *
 * The "about" view is reached from Settings → About in Phase 1.x; for now
 * it's a placeholder section inside Settings.
 */
export default function App() {
  const [view, setView] = useState<View>("loading");

  useEffect(() => {
    document.title = "PrivatePrompter";
    refresh();
  }, []);

  async function refresh() {
    const status = await checkAccessibilityPermission();
    setView(status.granted ? "settings" : "onboarding");
  }

  if (view === "loading") {
    return (
      <div className="flex h-full items-center justify-center bg-neutral-50 text-neutral-500 dark:bg-neutral-950 dark:text-neutral-400">
        <p className="text-sm">Loading…</p>
      </div>
    );
  }

  if (view === "onboarding") {
    return <Onboarding onComplete={() => setView("settings")} />;
  }

  return <Settings onRevoked={() => setView("onboarding")} />;
}