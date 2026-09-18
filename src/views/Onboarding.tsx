import { useState } from "react";
import {
  checkAccessibilityPermission,
  openAccessibilitySettings,
} from "../lib/permissions";

interface Props {
  onComplete: () => void;
}

/**
 * Onboarding view: shown the first time the user opens PrivatePrompter (and
 * any subsequent time they revoke the Accessibility permission).
 *
 * Phase 1 only asks for Accessibility. Apple Events permission is requested
 * implicitly by macOS when we send Apple Events after Accessibility is
 * granted, and notification permission is requested the first time we show
 * the "Undo" notification in Phase 8.
 */
export function Onboarding({ onComplete }: Props) {
  const [busy, setBusy] = useState(false);
  const [detail, setDetail] = useState<string | null>(null);

  async function recheck() {
    setBusy(true);
    setDetail(null);
    const status = await checkAccessibilityPermission();
    setDetail(status.detail);
    setBusy(false);
    if (status.granted) onComplete();
  }

  async function openSettings() {
    setBusy(true);
    try {
      await openAccessibilitySettings();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col bg-neutral-50 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
      <header className="border-b border-neutral-200 px-6 py-4 dark:border-neutral-800">
        <h1 className="text-lg font-semibold">Welcome to PrivatePrompter</h1>
        <p className="text-sm text-neutral-500 dark:text-neutral-400">
          A few permissions and you're ready to go.
        </p>
      </header>

      <main className="flex-1 space-y-6 overflow-y-auto px-6 py-6">
        <section>
          <h2 className="mb-2 text-base font-medium">
            1. Accessibility access
          </h2>
          <p className="text-sm leading-relaxed text-neutral-600 dark:text-neutral-300">
            PrivatePrompter needs Accessibility permission so it can copy
            highlighted text and paste the rewritten prompt back into the
            active application when you press the global hotkey. macOS shows
            this prompt the first time you trigger a rewrite; we also want to
            pre-flight it here.
          </p>
          <p className="mt-2 text-sm leading-relaxed text-neutral-600 dark:text-neutral-300">
            Open <strong>System Settings → Privacy & Security → Accessibility</strong>,
            toggle <strong>PrivatePrompter</strong> on, then come back and
            click <em>Recheck</em>.
          </p>
          <div className="mt-4 flex gap-2">
            <button
              onClick={openSettings}
              disabled={busy}
              className="rounded-md bg-neutral-900 px-3 py-1.5 text-sm font-medium text-white hover:bg-neutral-700 disabled:opacity-50 dark:bg-neutral-100 dark:text-neutral-900 dark:hover:bg-neutral-300"
            >
              Open System Settings
            </button>
            <button
              onClick={recheck}
              disabled={busy}
              className="rounded-md border border-neutral-300 px-3 py-1.5 text-sm font-medium hover:bg-neutral-100 disabled:opacity-50 dark:border-neutral-700 dark:hover:bg-neutral-900"
            >
              {busy ? "Checking…" : "Recheck"}
            </button>
          </div>
          {detail && (
            <p className="mt-3 text-xs text-amber-600 dark:text-amber-400">
              {detail}
            </p>
          )}
        </section>

        <section>
          <h2 className="mb-2 text-base font-medium">2. What's next</h2>
          <p className="text-sm leading-relaxed text-neutral-600 dark:text-neutral-300">
            Once Accessibility is granted, PrivatePrompter will download a
            small local LLM (~1 GB) the first time you use it, then you're
            ready to highlight text anywhere and press{" "}
            <kbd className="rounded border border-neutral-300 px-1 text-xs dark:border-neutral-700">
              ⌘⇧Space
            </kbd>
            .
          </p>
        </section>
      </main>
    </div>
  );
}