# Architecture

## Big picture

PrivatePrompter is a Tauri v2 desktop app. Rust owns the OS integration; TypeScript owns the UI.

```
┌──────────────────────────────────────────────────────────────────┐
│                       React + TypeScript (src/)                  │
│   Settings, Onboarding, Model, Templates, Privacy, About tabs    │
│   Tauri `invoke()` for everything that touches the OS           │
└─────────────────────────────┬────────────────────────────────────┘
                              │ Tauri IPC (invoke / emit / listen)
┌─────────────────────────────▼────────────────────────────────────┐
│                       Rust (src-tauri/src/)                      │
│                                                                  │
│   ┌───────────┐  ┌──────────┐  ┌─────────────┐  ┌─────────────┐  │
│   │ tray.rs   │  │hotkey.rs │  │clipboard.rs │  │ undo.rs     │  │
│   │ status    │  │ ⌘⌥R      │  │ AX          │  │ stack of    │  │
│   │ bar menu  │  │ pipeline │  │ selected    │  │ last        │  │
│   │           │  │          │  │ text I/O    │  │ rewrites    │  │
│   └───────────┘  └─────┬────┘  └─────────────┘  └─────────────┘  │
│                       │                                         │
│   ┌────────────────────▼──────────────────────────────────┐    │
│   │  context::detect() — Tier 1 + Tier 2 + Tier 4         │    │
│   │  prompt::render() — Handlebars meta-prompt template   │    │
│   └─────────────┬─────────────────────────────────────────┘    │
│                 │                                              │
│   ┌─────────────▼────────────┐  ┌──────────────────────────┐    │
│   │ inference::client        │  │  model::downloader       │    │
│   │ streaming HTTP + SSE     │  │  HF stream, SHA256 verify│    │
│   │ /completion → tokens     │  │  progress events         │    │
│   └─────────────┬────────────┘  └──────────────────────────┘    │
│                 │                                              │
│   ┌─────────────▼────────────┐  ┌──────────────────────────┐    │
│   │ inference::server        │  │  model::registry         │    │
│   │ llama-server sidecar     │  │  bundled models.json     │    │
│   │ 127.0.0.1:<random port>  │  │  RAM-tier recommendations │    │
│   └──────────────────────────┘  └──────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │  HTTP / SSE
                              ▼
                    ┌──────────────────────┐
                    │  llama-server (sidecar)│
                    │  GGUF on disk          │
                    └──────────────────────┘
```

## Hotkey flow

`⌘⌥R` (configurable via Settings → General once the Phase 9 hotkey UI lands) triggers this pipeline:

1. **Accessibility check.** Bail with a notification if our process isn't trusted — calling `AXUIElement*` without trust crashes the process (we tested).
2. **Read selected text via `kAXSelectedTextAttribute`.** No clipboard dance, no simulated `⌘C`.
3. **Detect context.** Tier 1: frontmost app + (if browser) URL via Apple Events. Tier 2: URL/app → context profile. Tier 4: universal fallback.
4. **Render meta-prompt.** Default = vendored `prompt-master` (MIT, credited in `assets/NOTICE`) with `{{input}}`, `{{profile_domain}}`, `{{profile_tone}}`, `{{profile_tools}}` placeholders.
5. **Stream inference from `llama-server`.** SSE response (`data: {"content": "..."}` lines, terminated by `data: [DONE]`). If the server isn't running we fall back to a placeholder rewrite so the rest of the pipeline is still exercisable without a model.
6. **Replace selection via `kAXSelectedTextAttribute`.** No clipboard write, no simulated `⌘V` — apps receive the change as if the user typed it.
7. **Push undo entry** with the rewritten text + metadata. Persist to `~/Library/Application Support/com.alifarooqi.privateprompter/undo.json`.
8. **Show notification** with the new prompt's char count, template, and detected context domain.

A second hotkey press while step 5 is in flight **cancels** the in-flight request. The user can also `⌘Z` in their host app to revert the replacement.

## Why no clipboard dance

The original design called for `⌘C → read clipboard → write clipboard → ⌘V`. We abandoned it for two reasons, both reproducible:

- **Input Monitoring required.** Simulating keystrokes via enigo's `CGEventPost` requires the calling process to hold Input Monitoring permission. Without it, the process is killed by the OS with no useful error.
- **Synthetic events are ignored.** Even with both Accessibility and IM granted, some apps reject `CGEvent`s that don't carry the hardware-source flag. We tested with TextEdit: manual `⌘V` worked, System Events `key code 9 using {command down}` did nothing.

`kAXSelectedTextAttribute` sidesteps both: it's the canonical way to read and replace selection in editable text fields. Trade-off is plain-text replacement only — fine for our use case (we replace the selection with a fresh rewrite, not styled text).

## Cancellation

A single `CancellationToken` (atomic bool + `Arc`) is shared between:

- The hotkey callback (sets it on a second press).
- The streaming client (polls between chunks).
- The model downloader (polls between chunks).

When set, the in-flight work aborts cleanly. The streaming client emits no further tokens; the downloader removes the `.part` file.

## App data directory

On macOS we resolve the data dir via `tauri::Manager::path().app_data_dir()`, which honors the bundle identifier:

```
~/Library/Application Support/com.alifarooqi.privateprompter/
├── models/<model-id>/<file>.gguf
├── templates/<name>.md
├── undo.json
└── (logs — Phase 9 file logging lands here)
```

macOS auto-renames data folders to this pattern when an app's writes don't match its bundle ID convention, so we use the bundle-ID form directly to avoid the rename (and the bug where downloaded models become invisible to the app on subsequent launches).

## Bundled prompts

`assets/prompts/` ships three Handlebars templates:

- `prompt-master.md` — vendored from `nidhinjs/prompt-master` (MIT). The default.
- `universal.md` — tight rewrite, no domain assumptions.
- `concise.md` — shorten without losing meaning.

User templates in `templates/` take precedence when names collide. The Settings → Templates tab (Phase 7 polish) will surface an editor.

## Phase map

| Phase | Status |
| --- | --- |
| 0 — Repo bootstrap, MIT license, OSS essentials | ✅ |
| 1 — Permissions probe + tray + settings shell | ✅ |
| 2 — Dynamic model management | ✅ |
| 3 — `llama.cpp` sidecar (bundled dylibs, OpenSSL off) | ✅ |
| 5 — Global hotkey + AX-based rewrite | ✅ |
| 6 — Context detection (Tiers 1/2/4) | ✅ |
| 7 — Meta-prompt templates | ✅ |
| 8 — Streaming SSE, cancellation, undo, notification | ✅ |
| 9 — Polish (file logging, error toasts, custom hotkey UI) | partial |
| 10 — Distribution (`release.yml`, `notarize.sh`) | ✅ |

Phase 4 (Personal Intelligence / RAG) deferred to v2 — see `plan/02.personal-intelligence.md`.

## Known sharp edges

These are issues we hit during the smoke test and either fixed or documented:

- **`⌘⇧Space` collides with Maccy**, **`⌘⌥Space` collides with Spotlight**. Use `⌘⌥R` (or pick your own once Phase 9 lands).
- **Models on disk must live under the bundle-ID-prefixed path.** The data-dir fix in `src-tauri/src/model/store.rs` resolves the path via Tauri so this is no longer a footgun.
- **`models.json` SHA256s are placeholders** (`00…00`). The downloader accepts that as "not yet pinned" so dev runs work; release pipeline needs real hashes.