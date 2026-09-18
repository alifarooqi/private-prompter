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
│   ┌───────────┐  ┌──────────┐  ┌────────────┐  ┌──────────────┐  │
│   │ tray.rs   │  │hotkey.rs │  │clipboard.rs│  │ undo.rs      │  │
│   │ status bar│  │ ⌘⇧Space  │  │ arboard +  │  │ stack of     │  │
│   │ menu      │  │ pipeline │  │ enigo      │  │ last rewrites│  │
│   └───────────┘  └─────┬────┘  └────────────┘  └──────────────┘  │
│                       │                                          │
│   ┌────────────────────▼───────────────────────────────────┐    │
│   │  context::detect() — Tier 1 + Tier 2 + Tier 4          │    │
│   │  prompt::render() — Handlebars meta-prompt template    │    │
│   └─────────────┬──────────────────────────────────────────┘    │
│                 │                                               │
│   ┌─────────────▼────────────┐  ┌──────────────────────────┐    │
│   │ inference::client        │  │  model::downloader       │    │
│   │ streaming HTTP           │  │  HF res, SHA256 verify   │    │
│   │ /completion → tokens     │  │  progress events         │    │
│   └─────────────┬────────────┘  └──────────────────────────┘    │
│                 │                                               │
│   ┌─────────────▼────────────┐  ┌──────────────────────────┐    │
│   │ inference::server        │  │  model::registry         │    │
│   │ llama-server sidecar     │  │  bundled models.json     │    │
│   │ 127.0.0.1:<random port>  │  │  RAM-tier recommendations │    │
│   └──────────────────────────┘  └──────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘
                              │
                              │  stdio / HTTP
                              ▼
                    ┌──────────────────────┐
                    │  llama-server (sidecar)│
                    │  GGUF on disk          │
                    └──────────────────────┘
```

## Hotkey flow

`Cmd+Shift+Space` (configurable later) triggers this pipeline:

1. **Save original clipboard.** If anything goes wrong later we can restore.
2. **Simulate `Cmd+C`.** Reads the user's current selection into the clipboard. Requires Accessibility.
3. **Read clipboard** → raw text. Bail with notification if empty.
4. **Detect context.** Tier 1: frontmost app + (if browser) URL via Apple Events. Tier 2: URL/app → context profile. Tier 4: universal fallback.
5. **Render meta-prompt.** Default = vendored `prompt-master` with `{{input}}`, `{{profile_domain}}`, `{{profile_tone}}`, `{{profile_tools}}` placeholders.
6. **Stream inference.** Each token updates the clipboard live; we simulate `Cmd+V` once streaming completes.
7. **Push undo entry** with TTL = 10s. Persist to `~/Library/Application Support/PrivatePrompter/undo.json`.
8. **Show notification** with the new prompt's char count, template, and context domain.

If the hotkey is pressed again while the pipeline is running, **cancel** triggers and the in-flight request aborts.

## Cancellation

A single `CancellationToken` (atomic bool + Arc) is shared between:

- The hotkey callback (sets it on a second press).
- The streaming client (polls between chunks).
- The model downloader (polls between chunks).

When set, the in-flight work aborts cleanly and the original clipboard is restored from the undo stack.

## Bundled prompts

`assets/prompts/` ships three Handlebars templates:

- `prompt-master.md` — vendored from `nidhinjs/prompt-master` (MIT, credited in `assets/NOTICE`). The default.
- `universal.md` — tight rewrite, no domain assumptions.
- `concise.md` — shorten without losing meaning.

User templates live in `~/Library/Application Support/PrivatePrompter/templates/` and take precedence when names collide. Phase 7 will surface an editor in Settings → Templates.

## Phase map

| Phase | Status |
| --- | --- |
| 0 — Repo bootstrap | ✅ |
| 1 — Permissions + tray + settings shell | ✅ |
| 2 — Dynamic model management | ✅ |
| 3 — llama.cpp sidecar | ✅ |
| 5 — Global hotkey + clipboard | ✅ |
| 6 — Context detection (Tiers 1/2/4) | ✅ |
| 7 — Meta-prompt templates | ✅ |
| 8 — Streaming, cancellation, undo, notification | ✅ |
| 9 — Polish (logging, error toasts) | partial |
| 10 — Distribution (sign + notarize + release.yml) | ✅ |

Phase 4 (Personal Intelligence / RAG) deferred to v2 — see `plan/02.personal-intelligence.md`.