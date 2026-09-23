import { useEffect, useRef, useState } from "react";
import {
  checkAccessibilityPermission,
  openAccessibilitySettings,
} from "../lib/permissions";
import {
  type DownloadProgress,
  type ModelSummary,
  type ServerStatus,
  cancelModelDownload,
  formatBytes,
  getActiveModel,
  inferenceHealth,
  inferenceStatus,
  listModels,
  onDownloadProgress,
  pauseModelDownload,
  recommendedModelId,
  resumeModelDownload,
  setActiveModel,
  startInference,
  startModelDownload,
  stopInference,
} from "../lib/model";
import { BUILTIN_TEMPLATES } from "../lib/templates";
import {
  codeToKey,
  displayCombo,
  getHotkey,
  modifiersFromEvent,
  resetHotkey,
  setHotkey,
  type HotkeyConfig,
} from "../lib/hotkey";

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

  // Inference + model selection state, lifted here so both the General tab
  // (where the user starts/stops the server and picks the active model)
  // and the Model tab (where downloads happen) can read+write it.
  const [models, setModels] = useState<ModelSummary[] | null>(null);
  const [recommendedId, setRecommendedId] = useState<string | null>(null);
  const [server, setServer] = useState<ServerStatus | null>(null);
  const [serverBusy, setServerBusy] = useState<"idle" | "starting" | "stopping">(
    "idle",
  );
  const [activeModelId, setActiveModelId] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [progressById, setProgressById] = useState<
    Record<string, DownloadProgress>
  >({});
  const [pausedById, setPausedById] = useState<Record<string, boolean>>({});
  const [downloadErrorById, setDownloadErrorById] = useState<
    Record<string, string | null>
  >({});

  useEffect(() => {
    refresh();
    void loadModels();
  }, []);

  async function refresh() {
    const status = await checkAccessibilityPermission(!hasPrompted);
    setHasPrompted(true);
    setPermissionGranted(status.granted);
    setPermissionDetail(status.detail);
    if (!status.granted) onRevoked();
  }

  async function loadModels() {
    try {
      const [list, recommended, initialStatus, health, active] = await Promise.all([
        listModels(),
        recommendedModelId(),
        inferenceStatus(),
        inferenceHealth(),
        getActiveModel(),
      ]);
      setModels(list);
      setRecommendedId(recommended);
      setServer({ ...initialStatus, loading: health?.status === "loading" });
      setActiveModelId(active.id);
    } catch (err) {
      console.error("model load failed", err);
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

  async function onSelectModel(modelId: string) {
    try {
      await setActiveModel(modelId);
      setActiveModelId(modelId);
      // If the server is running, restart it on the new model.
      if (server?.running) {
        await stopInference();
        const status = await startInference();
        setServer({ ...status, loading: true });
      }
    } catch (err) {
      console.error("select active model failed", err);
    }
  }

  const downloadedModels = (models ?? []).filter((m) => m.downloaded);

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
            downloadedModels={downloadedModels}
            server={server}
            serverBusy={serverBusy}
            activeModelId={activeModelId}
            onStartServer={onStartServer}
            onStopServer={onStopServer}
            onSelectModel={onSelectModel}
          />
        )}
        {tab === "model" && (
          <ModelTab
            models={models}
            recommendedId={recommendedId}
            activeModelId={activeModelId}
            progressById={progressById}
            pausedById={pausedById}
            downloadErrorById={downloadErrorById}
            busyId={busyId}
            setBusyId={setBusyId}
            setPausedById={setPausedById}
            setProgressById={setProgressById}
            setDownloadErrorById={setDownloadErrorById}
            setModels={setModels}
          />
        )}
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
  downloadedModels,
  server,
  serverBusy,
  activeModelId,
  onStartServer,
  onStopServer,
  onSelectModel,
}: {
  permissionGranted: boolean | null;
  permissionDetail: string | null;
  onRecheck: () => Promise<void>;
  onOpenSettings: () => Promise<void>;
  downloadedModels: ModelSummary[];
  server: ServerStatus | null;
  serverBusy: "idle" | "starting" | "stopping";
  activeModelId: string | null;
  onStartServer: () => Promise<void>;
  onStopServer: () => Promise<void>;
  onSelectModel: (id: string) => Promise<void>;
}) {
  return (
    <div className="space-y-6">
      <InferenceSection
        downloadedModels={downloadedModels}
        server={server}
        serverBusy={serverBusy}
        activeModelId={activeModelId}
        onStartServer={onStartServer}
        onStopServer={onStopServer}
        onSelectModel={onSelectModel}
      />

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

      <HotkeySection />

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

/**
 * Inference server controls. The first thing a user sees on opening the
 * General tab — a status row + a model dropdown + a single play/stop
 * icon button. The button shape (▶ / ■) keeps the visual weight low so
 * the row reads as a status indicator with an action, not as a form.
 */
function InferenceSection({
  downloadedModels,
  server,
  serverBusy,
  activeModelId,
  onStartServer,
  onStopServer,
  onSelectModel,
}: {
  downloadedModels: ModelSummary[];
  server: ServerStatus | null;
  serverBusy: "idle" | "starting" | "stopping";
  activeModelId: string | null;
  onStartServer: () => Promise<void>;
  onStopServer: () => Promise<void>;
  onSelectModel: (id: string) => Promise<void>;
}) {
  const running = server?.running ?? false;
  const starting = serverBusy === "starting";
  const stopping = serverBusy === "stopping";
  const busy = starting || stopping;
  const hasModel = downloadedModels.length > 0;

  const statusLine = (() => {
    if (running && server) {
      return server.loading
        ? `Loading model on ${server.host}:${server.port}…`
        : `Running on ${server.host}:${server.port}`;
    }
    return hasModel ? "Stopped" : "Download a model to start the inference server";
  })();

  const buttonLabel = running ? "Stop inference server" : "Start inference server";

  return (
    <section>
      <h2 className="mb-3 text-base font-medium">Inference server</h2>
      <div className="flex items-center justify-between gap-3 rounded-lg border border-neutral-200 p-4 dark:border-neutral-800">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-sm font-medium">
            <span
              aria-hidden
              className={`inline-block h-2.5 w-2.5 rounded-full ${
                running
                  ? server?.loading
                    ? "bg-amber-400"
                    : "bg-emerald-500"
                  : "bg-neutral-300 dark:bg-neutral-700"
              }`}
            />
            <span>{statusLine}</span>
          </div>
          <div className="mt-0.5 text-xs text-neutral-500 dark:text-neutral-400">
            {hasModel
              ? "Pick a downloaded model. Switching restarts the server."
              : "Download a model from the Model tab to enable inference."}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <select
            value={activeModelId ?? ""}
            onChange={(e) => onSelectModel(e.target.value)}
            disabled={!hasModel || busy}
            className="rounded-md border border-neutral-300 bg-white px-2 py-1.5 text-xs disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-900"
          >
            {hasModel ? (
              downloadedModels.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.display_name}
                </option>
              ))
            ) : (
              <option value="">No model downloaded</option>
            )}
          </select>
          <button
            onClick={running ? onStopServer : onStartServer}
            disabled={busy || (!running && !hasModel)}
            aria-label={buttonLabel}
            title={buttonLabel}
            className="inline-flex h-9 w-9 items-center justify-center rounded-md border border-neutral-300 bg-neutral-900 text-white hover:bg-neutral-700 disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-100 dark:text-neutral-900 dark:hover:bg-neutral-300"
          >
            {starting ? (
              <span className="text-xs">…</span>
            ) : stopping ? (
              <span className="text-xs">…</span>
            ) : running ? (
              <svg
                viewBox="0 0 24 24"
                fill="currentColor"
                aria-hidden
                className="h-4 w-4"
              >
                <rect x="6" y="6" width="12" height="12" rx="1.5" />
              </svg>
            ) : (
              <svg
                viewBox="0 0 24 24"
                fill="currentColor"
                aria-hidden
                className="h-4 w-4"
              >
                <path d="M8 5v14l11-7z" />
              </svg>
            )}
          </button>
        </div>
      </div>
    </section>
  );
}

function HotkeySection() {
  const [config, setConfig] = useState<HotkeyConfig | null>(null);
  // Recording state as a single discriminated union so the UI always
  // reflects the latest keypress unambiguously.
  type RecordingState =
    | { kind: "idle" }
    | { kind: "modifiers"; modifiers: string[] }
    | { kind: "combo"; modifiers: string[]; key: string };
  const [recording, setRecording] = useState<RecordingState>({ kind: "idle" });
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    getHotkey()
      .then((info) => setConfig(info.config))
      .catch((err) => setError(`failed to load: ${err}`));
  }, []);

  // Track recording as a ref so the document-level listener always sees
  // the latest value without having to re-bind the listener on every
  // state change.
  const recordingRef = useRef(recording);
  recordingRef.current = recording;

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const rec = recordingRef.current;
      if (rec.kind === "idle") return;

      if (e.key === "Escape") {
        e.preventDefault();
        setError(null);
        setRecording({ kind: "idle" });
        return;
      }

      const key = codeToKey(e.code);
      const mods = modifiersFromEvent(e);

      if (key === null) {
        // Modifier-only press. Update live preview (e.g. "⌘…" then
        // "⌘⌥…"). preventDefault stops the OS from interpreting bare
        // modifier taps (e.g. ⌥ alone on macOS inserts special chars).
        e.preventDefault();
        setError(null);
        setRecording({ kind: "modifiers", modifiers: mods });
        return;
      }

      // Non-modifier key pressed. A complete combo needs at least one
      // modifier.
      if (mods.length === 0) {
        e.preventDefault();
        setError("Pick at least one modifier (⌘, ⌥, ⌃, or ⇧).");
        return;
      }
      e.preventDefault();
      setError(null);
      setRecording({ kind: "combo", modifiers: mods, key });
    }
    // Listen at the window level with capture phase so we catch the
    // event before any focused element (e.g. the Change button) can
    // swallow it.
    window.addEventListener("keydown", onKey, { capture: true });
    return () => window.removeEventListener("keydown", onKey, { capture: true });
  }, []);

  const pendingCombo =
    recording.kind === "combo" ? recording : null;
  const liveMods =
    recording.kind === "modifiers" ? recording.modifiers : [];

  async function save() {
    if (!pendingCombo) return;
    setBusy(true);
    setError(null);
    try {
      const info = await setHotkey(pendingCombo);
      setConfig(info.config);
      setRecording({ kind: "idle" });
    } catch (err) {
      setError(`Couldn't register: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  async function reset() {
    setBusy(true);
    setError(null);
    try {
      const info = await resetHotkey();
      setConfig(info.config);
      setRecording({ kind: "idle" });
    } catch (err) {
      setError(`Reset failed: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section>
      <h2 className="mb-3 text-base font-medium">Hotkey</h2>
      <p className="mb-2 text-sm text-neutral-500 dark:text-neutral-400">
        Highlight text anywhere and press the hotkey to rewrite.
      </p>

      <div className="flex items-start gap-3 rounded-lg border border-neutral-200 p-4 dark:border-neutral-800">
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          {recording.kind !== "idle" ? (
            <>
              <div className="text-sm font-medium text-neutral-900 dark:text-neutral-100">
                {pendingCombo ? (
                  <>
                    Press a new combination…{" "}
                    <span className="text-neutral-500 dark:text-neutral-400">
                      (saved combo:{" "}
                      {displayCombo(pendingCombo.modifiers, pendingCombo.key)})
                    </span>
                  </>
                ) : liveMods.length > 0 ? (
                  <span>
                    {displayCombo(liveMods, "")}
                    <span className="ml-1 animate-pulse text-neutral-400">…</span>
                  </span>
                ) : (
                  "Press a new combination…"
                )}
              </div>
              <div className="text-xs text-neutral-500 dark:text-neutral-400">
                Modifiers + one regular key. Press{" "}
                <kbd className="rounded border border-neutral-300 px-1 text-[10px] dark:border-neutral-700">
                  Esc
                </kbd>{" "}
                to cancel.
              </div>
            </>
          ) : config ? (
            <>
              <div className="text-sm font-medium text-neutral-900 dark:text-neutral-100">
                <kbd className="rounded border border-neutral-300 bg-neutral-100 px-2 py-1 font-mono text-xs dark:border-neutral-700 dark:bg-neutral-800">
                  {displayCombo(config.modifiers, config.key)}
                </kbd>
              </div>
              <div className="mt-1 text-xs text-neutral-500 dark:text-neutral-400">
                Click Change to record a new combination.
              </div>
            </>
          ) : (
            <div className="text-sm text-neutral-500">Loading…</div>
          )}
        </div>

        <div className="flex shrink-0 gap-2">
          {recording.kind !== "idle" ? (
            <>
              <button
                onClick={save}
                disabled={!pendingCombo || busy}
                className="rounded-md bg-neutral-900 px-3 py-1.5 text-xs font-medium text-white hover:bg-neutral-700 disabled:opacity-50 dark:bg-neutral-100 dark:text-neutral-900 dark:hover:bg-neutral-300"
              >
                {pendingCombo
                  ? `Save ${displayCombo(pendingCombo.modifiers, pendingCombo.key)}`
                  : "Save"}
              </button>
              <button
                onClick={() => setRecording({ kind: "idle" })}
                disabled={busy}
                className="rounded-md border border-neutral-300 px-3 py-1.5 text-xs font-medium hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-900"
              >
                Cancel
              </button>
            </>
          ) : (
            <>
              <button
                onClick={() => setRecording({ kind: "modifiers", modifiers: [] })}
                className="rounded-md border border-neutral-300 px-3 py-1.5 text-xs font-medium hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-900"
              >
                Change
              </button>
              <button
                onClick={reset}
                disabled={busy}
                className="rounded-md border border-neutral-300 px-3 py-1.5 text-xs font-medium hover:bg-neutral-100 disabled:opacity-50 dark:border-neutral-700 dark:hover:bg-neutral-900"
              >
                Reset
              </button>
            </>
          )}
        </div>
      </div>

      {error && (
        <p className="mt-2 text-xs text-amber-600 dark:text-amber-400">
          {error}
        </p>
      )}
    </section>
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

function ModelTab({
  models,
  recommendedId,
  activeModelId,
  progressById,
  pausedById,
  downloadErrorById,
  busyId,
  setBusyId,
  setPausedById,
  setProgressById,
  setDownloadErrorById,
  setModels,
}: {
  models: ModelSummary[] | null;
  recommendedId: string | null;
  activeModelId: string | null;
  progressById: Record<string, DownloadProgress>;
  pausedById: Record<string, boolean>;
  downloadErrorById: Record<string, string | null>;
  busyId: string | null;
  setBusyId: (id: string | null) => void;
  setPausedById: React.Dispatch<
    React.SetStateAction<Record<string, boolean>>
  >;
  setProgressById: React.Dispatch<
    React.SetStateAction<Record<string, DownloadProgress>>
  >;
  setDownloadErrorById: React.Dispatch<
    React.SetStateAction<Record<string, string | null>>
  >;
  setModels: (models: ModelSummary[]) => void;
}) {
  useEffect(() => {
    // Subscribe to download progress events to update per-model cards.
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    onDownloadProgress((p) => {
      if (cancelled) return;
      setProgressById((prev) => ({ ...prev, [p.modelId]: p }));
      if (p.state === "completed" || p.state === "failed") {
        listModels().then(setModels).catch(() => {});
      }
    }).then((u) => {
      unlisten = u;
    });
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [setModels, setProgressById]);

  async function onDownload(modelId: string) {
    setBusyId(modelId);
    setPausedById((prev) => ({ ...prev, [modelId]: false }));
    setDownloadErrorById((prev) => ({ ...prev, [modelId]: null }));
    try {
      await startModelDownload(modelId);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setDownloadErrorById((prev) => ({ ...prev, [modelId]: message }));
      console.error("download failed", err);
    } finally {
      setBusyId(null);
    }
  }

  async function onPause(modelId: string) {
    setPausedById((prev) => ({ ...prev, [modelId]: true }));
    try {
      await pauseModelDownload();
    } catch (err) {
      console.error("pause failed", err);
    }
  }

  async function onResume(modelId: string) {
    setPausedById((prev) => ({ ...prev, [modelId]: false }));
    try {
      await resumeModelDownload();
    } catch (err) {
      console.error("resume failed", err);
    }
  }

  async function onCancel(modelId: string) {
    setPausedById((prev) => ({ ...prev, [modelId]: false }));
    try {
      await cancelModelDownload();
    } catch (err) {
      console.error("cancel failed", err);
    }
  }

  if (models === null) {
    return <p className="text-sm text-neutral-500">Loading model registry…</p>;
  }

  return (
    <div className="space-y-5">
      <section>
        <h2 className="mb-3 text-base font-medium">Models</h2>
        <p className="text-sm text-neutral-500 dark:text-neutral-400">
          Download a model to enable the inference server. You can manage the
          active model from the General tab.
        </p>
      </section>

      <section className="space-y-3">
        {models.map((m) => {
          const progress = progressById[m.id];
          const downloading = busyId === m.id;
          const paused = pausedById[m.id] === true;
          const isRecommended = m.id === recommendedId;
          const isActive = activeModelId === m.id;
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
                  {isActive && m.downloaded && (
                    <span className="rounded bg-emerald-100 px-1.5 py-0.5 text-[10px] font-medium uppercase text-emerald-700 dark:bg-emerald-900 dark:text-emerald-200">
                      Active
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
                    {paused ? "Paused" : progress.state} ·{" "}
                    {formatBytes(progress.bytesDownloaded)} /{" "}
                    {formatBytes(progress.totalBytes)}
                  </div>
                )}
                {downloadErrorById[m.id] && !downloading && !m.downloaded && (
                  <div className="mt-2 text-xs text-red-600 dark:text-red-400">
                    Download failed: {downloadErrorById[m.id]}
                  </div>
                )}
              </div>
              <div className="ml-3 flex shrink-0 gap-2">
                {m.downloaded ? (
                  <span className="rounded-md border border-green-200 px-2.5 py-1 text-xs font-medium text-green-700 dark:border-green-800 dark:text-green-300">
                    Ready
                  </span>
                ) : downloading ? (
                  <>
                    <button
                      onClick={() => (paused ? onResume(m.id) : onPause(m.id))}
                      className="rounded-md border border-neutral-300 px-3 py-1.5 text-xs font-medium hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-900"
                    >
                      {paused ? "Resume" : "Pause"}
                    </button>
                    <button
                      onClick={() => onCancel(m.id)}
                      className="rounded-md border border-red-200 px-3 py-1.5 text-xs font-medium text-red-700 hover:bg-red-50 dark:border-red-900 dark:text-red-300 dark:hover:bg-red-950"
                    >
                      Cancel
                    </button>
                  </>
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