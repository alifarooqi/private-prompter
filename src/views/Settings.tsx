import { useEffect, useState } from "react";
import {
  checkAccessibilityPermission,
  openAccessibilitySettings,
} from "../lib/permissions";

interface Props {
  onRevoked: () => void;
}

type Tab = "general" | "model" | "templates" | "privacy" | "about";

/**
 * Settings window — the only window PrivatePrompter has. Phase 1 wires up the
 * shell, tabs, and the Accessibility permission panel. Subsequent phases fill
 * in the contents of each tab.
 */
export function Settings({ onRevoked }: Props) {
  const [tab, setTab] = useState<Tab>("general");
  const [permissionGranted, setPermissionGranted] = useState<boolean | null>(
    null,
  );
  const [permissionDetail, setPermissionDetail] = useState<string | null>(null);

  useEffect(() => {
    refresh();
  }, []);

  async function refresh() {
    const status = await checkAccessibilityPermission();
    setPermissionGranted(status.granted);
    setPermissionDetail(status.detail);
    if (!status.granted) onRevoked();
  }

  return (
    <div className="flex h-full bg-neutral-50 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
      <aside className="flex w-44 flex-col gap-1 border-r border-neutral-200 p-3 dark:border-neutral-800">
        <SidebarTab id="general" current={tab} setTab={setTab}>
          General
        </SidebarTab>
        <SidebarTab id="model" current={tab} setTab={setTab}>
          Model
        </SidebarTab>
        <SidebarTab id="templates" current={tab} setTab={setTab}>
          Templates
        </SidebarTab>
        <SidebarTab id="privacy" current={tab} setTab={setTab}>
          Privacy
        </SidebarTab>
        <SidebarTab id="about" current={tab} setTab={setTab}>
          About
        </SidebarTab>
      </aside>

      <main className="flex-1 overflow-y-auto px-6 py-5">
        {tab === "general" && (
          <GeneralTab
            permissionGranted={permissionGranted}
            permissionDetail={permissionDetail}
            onRecheck={refresh}
            onOpenSettings={openAccessibilitySettings}
          />
        )}
        {tab === "model" && <ModelTab />}
        {tab === "templates" && <TemplatesTab />}
        {tab === "privacy" && <PrivacyTab />}
        {tab === "about" && <AboutTab />}
      </main>
    </div>
  );
}

function SidebarTab({
  id,
  current,
  setTab,
  children,
}: {
  id: Tab;
  current: Tab;
  setTab: (t: Tab) => void;
  children: React.ReactNode;
}) {
  const active = id === current;
  return (
    <button
      onClick={() => setTab(id)}
      className={`rounded-md px-3 py-1.5 text-left text-sm transition ${
        active
          ? "bg-neutral-200 text-neutral-900 dark:bg-neutral-800 dark:text-neutral-100"
          : "text-neutral-600 hover:bg-neutral-100 dark:text-neutral-400 dark:hover:bg-neutral-900"
      }`}
    >
      {children}
    </button>
  );
}

function GeneralTab({
  permissionGranted,
  permissionDetail,
  onRecheck,
  onOpenSettings,
}: {
  permissionGranted: boolean | null;
  permissionDetail: string | null;
  onRecheck: () => Promise<void>;
  onOpenSettings: () => Promise<void>;
}) {
  return (
    <div className="space-y-6">
      <section>
        <h2 className="mb-3 text-base font-medium">Permissions</h2>
        <PermissionRow
          label="Accessibility"
          granted={permissionGranted}
          detail={permissionDetail}
          onRecheck={onRecheck}
          onOpenSettings={onOpenSettings}
        />
      </section>

      <section>
        <h2 className="mb-3 text-base font-medium">Hotkey</h2>
        <p className="text-sm text-neutral-500 dark:text-neutral-400">
          <kbd className="rounded border border-neutral-300 px-1 text-xs dark:border-neutral-700">
            ⌘⇧Space
          </kbd>{" "}
          — highlight text anywhere, press to rewrite. (Customizable in a
          later phase.)
        </p>
      </section>

      <section>
        <h2 className="mb-3 text-base font-medium">Startup</h2>
        <label className="flex items-center gap-2 text-sm text-neutral-600 dark:text-neutral-400">
          <input type="checkbox" disabled className="rounded" />
          Launch at login (Phase 9)
        </label>
      </section>
    </div>
  );
}

function PermissionRow({
  label,
  granted,
  detail,
  onRecheck,
  onOpenSettings,
}: {
  label: string;
  granted: boolean | null;
  detail: string | null;
  onRecheck: () => Promise<void>;
  onOpenSettings: () => Promise<void>;
}) {
  const status = granted === null ? "Checking…" : granted ? "Granted" : "Not granted";
  return (
    <div className="rounded-lg border border-neutral-200 p-4 dark:border-neutral-800">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-sm font-medium">{label}</div>
          <div className="text-xs text-neutral-500 dark:text-neutral-400">
            Status: {status}
            {detail && !granted ? ` — ${detail}` : null}
          </div>
        </div>
        <div className="flex gap-2">
          <button
            onClick={onOpenSettings}
            className="rounded-md border border-neutral-300 px-3 py-1.5 text-xs font-medium hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-900"
          >
            Open Settings
          </button>
          <button
            onClick={onRecheck}
            className="rounded-md bg-neutral-900 px-3 py-1.5 text-xs font-medium text-white hover:bg-neutral-700 dark:bg-neutral-100 dark:text-neutral-900 dark:hover:bg-neutral-300"
          >
            Recheck
          </button>
        </div>
      </div>
    </div>
  );
}

function ModelTab() {
  return (
    <div className="space-y-3">
      <h2 className="text-base font-medium">Model</h2>
      <p className="text-sm text-neutral-500 dark:text-neutral-400">
        Phase 2: choose a model, see download progress, switch models.
      </p>
    </div>
  );
}

function TemplatesTab() {
  return (
    <div className="space-y-3">
      <h2 className="text-base font-medium">Templates</h2>
      <p className="text-sm text-neutral-500 dark:text-neutral-400">
        Phase 7: browse and edit meta-prompt templates.
      </p>
    </div>
  );
}

function PrivacyTab() {
  return (
    <div className="space-y-3">
      <h2 className="text-base font-medium">Privacy</h2>
      <p className="text-sm text-neutral-500 dark:text-neutral-400">
        Phase 9: clear undo stack, clear cached prompts, opt-in diagnostics.
      </p>
      <p className="text-sm text-neutral-500 dark:text-neutral-400">
        Read the{" "}
        <a
          className="text-blue-600 underline dark:text-blue-400"
          href="https://github.com/<owner>/private-prompter/blob/main/docs/privacy.md"
        >
          privacy policy
        </a>{" "}
        for details.
      </p>
    </div>
  );
}

function AboutTab() {
  return (
    <div className="space-y-3">
      <h2 className="text-base font-medium">About</h2>
      <p className="text-sm text-neutral-600 dark:text-neutral-300">
        PrivatePrompter v0.1.0 — MIT licensed.
      </p>
      <ul className="space-y-1 text-sm text-blue-600 dark:text-blue-400">
        <li>
          <a
            className="underline"
            href="https://github.com/<owner>/private-prompter"
          >
            Source on GitHub
          </a>
        </li>
        <li>
          <a
            className="underline"
            href="https://github.com/<owner>/private-prompter/blob/main/docs/privacy.md"
          >
            Privacy policy
          </a>
        </li>
        <li>
          <a
            className="underline"
            href="https://github.com/<owner>/private-prompter/blob/main/CHANGELOG.md"
          >
            Changelog
          </a>
        </li>
      </ul>
      <p className="pt-2 text-xs text-neutral-500 dark:text-neutral-400">
        Default meta-prompt adapted from{" "}
        <a
          className="underline"
          href="https://github.com/nidhinjs/prompt-master"
        >
          nidhinjs/prompt-master
        </a>{" "}
        (MIT).
      </p>
    </div>
  );
}