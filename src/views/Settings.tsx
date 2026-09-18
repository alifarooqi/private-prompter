import { useEffect, useState } from "react";
import {
  checkAccessibilityPermission,
  openAccessibilitySettings,
} from "../lib/permissions";
import {
  type DownloadProgress,
  type ModelSummary,
  type ServerStatus,
  formatBytes,
  inferenceHealth,
  inferenceStatus,
  listModels,
  onDownloadProgress,
  recommendedModelId,
  startInference,
  startModelDownload,
  stopInference,
} from "../lib/model";
import { BUILTIN_TEMPLATES } from "../lib/templates";

interface Props {
  onRevoked: () => void;
}

type Tab = "general" | "model" | "templates" | "privacy" | "about";

/**
 * Settings window — the only window PrivatePrompter has.
 *
 * Phase 1 wires up the shell + permissions tab. Phase 2 fills the Model
 * tab (this commit). Subsequent phases fill Templates, Privacy, About.
 */
export function Settings({ onRevoked }: Props) {
  const [tab, setTab] = useState<Tab>("general");
  const [permissionGranted, setPermissionGranted] = useState<boolean | null>(
    null,
  );
  const [permissionDetail, setPermissionDetail] = useState<string | null>(null);
  const [hasPrompted, setHasPrompted] = useState(false);

  useEffect(() => {
    refresh();
  }, []);

  async function refresh() {
    const status = await checkAccessibilityPermission(!hasPrompted);
    setHasPrompted(true);
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
            ⌘⌥R
          </kbd>{" "}
          — highlight text anywhere, press to rewrite.
        </p>
        <p className="mt-1 text-xs text-neutral-500 dark:text-neutral-400">
          We tried ⌘⇧Space (Maccy default) and ⌘⌥Space (Spotlight
          window-search variant on some macOS). ⌘⌥R isn't bound by any
          first-party macOS shortcut. Custom hotkey UI is a Phase 9 polish
          item.
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
  const [models, setModels] = useState<ModelSummary[] | null>(null);
  const [recommendedId, setRecommendedId] = useState<string | null>(null);
  const [progressById, setProgressById] = useState<
    Record<string, DownloadProgress>
  >({});
  const [server, setServer] = useState<ServerStatus | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [serverBusy, setServerBusy] = useState<"idle" | "starting" | "stopping">(
    "idle",
  );

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    async function load() {
      try {
        const [list, recommended, initialStatus, health] = await Promise.all([
          listModels(),
          recommendedModelId(),
          inferenceStatus(),
          inferenceHealth(),
        ]);
        if (!cancelled) {
          setModels(list);
          setRecommendedId(recommended);
          setServer({ ...initialStatus, loading: health?.status === "loading" });
        }
        unlisten = await onDownloadProgress((p) => {
          setProgressById((prev) => ({ ...prev, [p.modelId]: p }));
          if (p.state === "completed" || p.state === "failed") {
            listModels().then(setModels).catch(() => {});
          }
        });
      } catch (err) {
        console.error("model load failed", err);
      }
    }

    void load();

    // Poll /health while a server is up so the UI stays in sync with
    // crashes and "model loading" → "ready" transitions.
    const poll = window.setInterval(async () => {
      if (cancelled) return;
      try {
        const [status, health] = await Promise.all([
          inferenceStatus(),
          inferenceHealth(),
        ]);
        if (!cancelled) {
          setServer({
            ...status,
            loading: status.running && health?.status === "loading",
          });
        }
      } catch (err) {
        console.error("inference status poll failed", err);
      }
    }, 3000);

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
      window.clearInterval(poll);
    };
  }, []);

  async function onDownload(modelId: string) {
    setBusyId(modelId);
    try {
      await startModelDownload(modelId);
    } catch (err) {
      console.error("download failed", err);
    } finally {
      setBusyId(null);
    }
  }

  async function onStartServer() {
    setServerBusy("starting");
    try {
      const status = await startInference();
      setServer({ ...status, loading: true });
    } catch (err) {
      console.error("start inference failed", err);
    } finally {
      setServerBusy("idle");
    }
  }

  async function onStopServer() {
    setServerBusy("stopping");
    try {
      await stopInference();
      const status = await inferenceStatus();
      setServer({ ...status, loading: false });
    } catch (err) {
      console.error("stop inference failed", err);
    } finally {
      setServerBusy("idle");
    }
  }

  if (models === null) {
    return <p className="text-sm text-neutral-500">Loading model registry…</p>;
  }

  return (
    <div className="space-y-5">
      <section>
        <h2 className="mb-3 text-base font-medium">Model</h2>
        <p className="text-sm text-neutral-500 dark:text-neutral-400">
          Highlighted text is rewritten by a small local LLM. Pick the model
          that matches your Mac's RAM. The default recommendation is based on
          <code className="ml-1 rounded bg-neutral-100 px-1 text-xs dark:bg-neutral-800">
            sysctl hw.memsize
          </code>
          .
        </p>
      </section>

      <section className="space-y-3">
        {models.map((m) => {
          const progress = progressById[m.id];
          const downloading = busyId === m.id;
          const isRecommended = m.id === recommendedId;
          return (
            <div
              key={m.id}
              className="flex items-center justify-between rounded-lg border border-neutral-200 p-4 dark:border-neutral-800"
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <div className="text-sm font-medium">{m.display_name}</div>
                  {isRecommended && (
                    <span className="rounded bg-blue-100 px-1.5 py-0.5 text-[10px] font-medium uppercase text-blue-700 dark:bg-blue-900 dark:text-blue-200">
                      Recommended
                    </span>
                  )}
                </div>
                <div className="text-xs text-neutral-500 dark:text-neutral-400">
                  {m.publisher} · {formatBytes(m.size_bytes)} ·{" "}
                  {Math.round(m.min_ram_bytes / 1024 ** 3)}+ GB RAM
                </div>
                {progress && (
                  <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800">
                    <div
                      className="h-full bg-neutral-900 transition-[width] dark:bg-neutral-100"
                      style={{
                        width: `${Math.min(100, (progress.bytesDownloaded / progress.totalBytes) * 100)}%`,
                      }}
                    />
                  </div>
                )}
                {progress && (
                  <div className="mt-1 text-xs text-neutral-500 dark:text-neutral-400">
                    {progress.state} ·{" "}
                    {formatBytes(progress.bytesDownloaded)} /{" "}
                    {formatBytes(progress.totalBytes)}
                  </div>
                )}
              </div>
              <div className="ml-3 shrink-0">
                {m.downloaded ? (
                  <span className="rounded-md border border-green-200 px-2.5 py-1 text-xs font-medium text-green-700 dark:border-green-800 dark:text-green-300">
                    Ready
                  </span>
                ) : downloading ? (
                  <button
                    disabled
                    className="rounded-md border border-neutral-300 px-3 py-1.5 text-xs font-medium opacity-50 dark:border-neutral-700"
                  >
                    Downloading…
                  </button>
                ) : (
                  <button
                    onClick={() => onDownload(m.id)}
                    className="rounded-md bg-neutral-900 px-3 py-1.5 text-xs font-medium text-white hover:bg-neutral-700 dark:bg-neutral-100 dark:text-neutral-900 dark:hover:bg-neutral-300"
                  >
                    Download
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </section>

      <section>
        <h2 className="mb-3 text-base font-medium">Inference server</h2>
        <div className="flex items-center justify-between rounded-lg border border-neutral-200 p-4 dark:border-neutral-800">
          <div className="text-sm">
            <div className="font-medium">
              {server?.running
                ? server.loading
                  ? `Loading model on ${server.host}:${server.port}…`
                  : `Running on ${server.host}:${server.port}`
                : "Not running"}
            </div>
            <div className="text-xs text-neutral-500 dark:text-neutral-400">
              The server starts automatically the first time you trigger a
              rewrite. You can also start it manually here.
            </div>
          </div>
          {server?.running ? (
            <button
              onClick={onStopServer}
              disabled={serverBusy !== "idle"}
              className="rounded-md border border-neutral-300 px-3 py-1.5 text-xs font-medium hover:bg-neutral-100 disabled:opacity-50 dark:border-neutral-700 dark:hover:bg-neutral-900"
            >
              {serverBusy === "stopping" ? "Stopping…" : "Stop server"}
            </button>
          ) : (
            <button
              onClick={onStartServer}
              disabled={serverBusy !== "idle" || !models.some((m) => m.downloaded)}
              className="rounded-md border border-neutral-300 px-3 py-1.5 text-xs font-medium hover:bg-neutral-100 disabled:opacity-50 dark:border-neutral-700 dark:hover:bg-neutral-900"
            >
              {serverBusy === "starting" ? "Starting…" : "Start server"}
            </button>
          )}
        </div>
      </section>
    </div>
  );
}

function TemplatesTab() {
  const [active, setActive] = useState<string>("prompt-master");

  return (
    <div className="space-y-5">
      <section>
        <h2 className="mb-3 text-base font-medium">Templates</h2>
        <p className="text-sm text-neutral-500 dark:text-neutral-400">
          Pick the meta-prompt that turns your highlighted text into the
          final prompt we send to the model. Edit-in-place is planned for
          Phase 7 (Polish).
        </p>
      </section>

      <section className="space-y-2">
        {BUILTIN_TEMPLATES.map((t) => (
          <label
            key={t.id}
            className={`flex cursor-pointer items-start gap-3 rounded-lg border p-4 transition ${
              active === t.id
                ? "border-neutral-900 dark:border-neutral-100"
                : "border-neutral-200 hover:border-neutral-400 dark:border-neutral-800 dark:hover:border-neutral-600"
            }`}
          >
            <input
              type="radio"
              name="template"
              value={t.id}
              checked={active === t.id}
              onChange={() => setActive(t.id)}
              className="mt-0.5"
            />
            <div className="min-w-0">
              <div className="text-sm font-medium">{t.display_name}</div>
              <div className="text-xs text-neutral-500 dark:text-neutral-400">
                {t.description}
              </div>
            </div>
          </label>
        ))}
      </section>
    </div>
  );
}

function PrivacyTab() {
  return (
    <div className="space-y-3">
      <h2 className="text-base font-medium">Privacy</h2>
      <p className="text-sm text-neutral-600 dark:text-neutral-300">
        PrivatePrompter makes no network calls after the initial model
        download. No analytics, no telemetry, no update pings beyond a
        manual "Check for updates" action. Highlighted text never leaves
        your device.
      </p>
      <p className="text-sm text-neutral-500 dark:text-neutral-400">
        Read the{" "}
        <a
          className="text-blue-600 underline dark:text-blue-400"
          href="https://github.com/<owner>/private-prompter/blob/main/docs/privacy.md"
        >
          privacy policy
        </a>
        .
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
        <a className="underline" href="https://github.com/nidhinjs/prompt-master">
          nidhinjs/prompt-master
        </a>{" "}
        (MIT). See <code>assets/NOTICE</code>.
      </p>
    </div>
  );
}